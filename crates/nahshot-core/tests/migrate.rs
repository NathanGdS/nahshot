// Only `png` is used here.
#[allow(dead_code)]
mod common;

use std::fs;
use std::path::PathBuf;

use nahshot_core::library::Library;
use nahshot_core::migrate::AlbumsMigration;
use nahshot_core::model::{Print, Query};

use common::png;

/// A temp folder with a library root and its database, opened on demand.
struct Setup {
    dir: tempfile::TempDir,
}

impl Setup {
    fn new() -> Setup {
        Setup {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("root")
    }

    fn open(&self) -> Library {
        Library::open(self.root(), &self.dir.path().join("db.sqlite")).unwrap()
    }

    /// Writes `root\<rel>` as a PNG.
    fn write(&self, rel: &[&str]) -> PathBuf {
        let path = rel.iter().fold(self.root(), |p, part| p.join(part));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, png()).unwrap();
        path
    }
}

fn print_named(lib: &Library, name: &str) -> Print {
    lib.query(&Query::default())
        .unwrap()
        .into_iter()
        .find(|p| p.file_name == name)
        .unwrap_or_else(|| panic!("{name} not in the gallery"))
}

/// An old library: prints in `Álbuns\Bugs`, registered, tagged and favorited while
/// the album folder still had that name. The library is opened before the folder
/// exists, so nothing migrates until the next open.
fn old_library(s: &Setup) {
    let lib = s.open();
    s.write(&["Álbuns", "Bugs", "2026-10-01_08-00-00.png"]);
    s.write(&["Álbuns", "Bugs", "2026-10-02_08-00-00.png"]);
    lib.sync().unwrap();
    let p = print_named(&lib, "2026-10-01_08-00-00.png");
    lib.add_tag(&[p.id], "crash").unwrap();
    lib.set_favorite(&[p.id], true).unwrap();
}

#[test]
fn renames_the_old_folder_and_keeps_tags_and_favorites() {
    let s = Setup::new();
    old_library(&s);
    let lib = s.open();

    assert!(!s.root().join("Álbuns").exists());
    assert_eq!(lib.albums_dir(), s.root().join("Albums"));
    assert_eq!(lib.albums().unwrap(), vec!["Bugs".to_string()]);
    let p = print_named(&lib, "2026-10-01_08-00-00.png");
    assert_eq!(
        p.path,
        s.root()
            .join("Albums")
            .join("Bugs")
            .join("2026-10-01_08-00-00.png")
    );
    assert!(p.path.exists());
    assert_eq!(p.album.as_deref(), Some("Bugs"));
    assert_eq!(p.tags, vec!["crash".to_string()]);
    assert!(p.favorite);
    assert_eq!(lib.query(&Query::default()).unwrap().len(), 2);
    // A sync afterwards finds everything where the database says it is.
    let report = lib.sync().unwrap();
    assert_eq!((report.added, report.removed), (0, 0));
}

#[test]
fn nothing_to_do_without_the_old_folder() {
    let s = Setup::new();
    let mut lib = s.open();
    assert_eq!(
        lib.migrate_albums_dir().unwrap(),
        AlbumsMigration::NotNeeded
    );
    lib.create_album("Bugs").unwrap();
    assert!(s.root().join("Albums").join("Bugs").is_dir());
    assert!(!s.root().join("Álbuns").exists());
    // Reopening is a no-op too.
    drop(lib);
    let lib = s.open();
    assert_eq!(lib.albums().unwrap(), vec!["Bugs".to_string()]);
}

#[test]
fn merges_into_an_existing_albums_folder_without_overwriting() {
    let s = Setup::new();
    let mut lib = s.open();
    let a = s.write(&["Álbuns", "Bugs", "a.png"]);
    let clash_old = s.write(&["Álbuns", "Bugs", "b.png"]);
    s.write(&["Álbuns", "Memes", "c.png"]);
    let clash_new = s.write(&["Albums", "Bugs", "b.png"]);
    fs::write(&clash_new, b"the newer b.png").unwrap();
    lib.sync().unwrap();
    let a_id = lib
        .query(&Query::default())
        .unwrap()
        .into_iter()
        .find(|p| p.path == a)
        .unwrap()
        .id;
    lib.add_tag(&[a_id], "keep").unwrap();

    let result = lib.migrate_albums_dir().unwrap();

    assert_eq!(
        result,
        AlbumsMigration::Merged {
            left: vec![clash_old.clone()]
        }
    );
    let albums = s.root().join("Albums");
    assert!(albums.join("Bugs").join("a.png").exists());
    assert!(albums.join("Memes").join("c.png").exists());
    // Neither b.png was touched: the old one waits in Álbuns.
    assert!(clash_old.exists());
    assert_eq!(fs::read(&clash_new).unwrap(), b"the newer b.png");
    let moved = lib.get(a_id).unwrap();
    assert_eq!(moved.path, albums.join("Bugs").join("a.png"));
    assert_eq!(moved.album.as_deref(), Some("Bugs"));
    assert_eq!(moved.tags, vec!["keep".to_string()]);
    assert_eq!(
        lib.albums().unwrap(),
        vec!["Bugs".to_string(), "Memes".to_string()]
    );
    // The emptied album folder went away; the one with the leftover stays.
    assert!(!s.root().join("Álbuns").join("Memes").exists());
    assert!(s.root().join("Álbuns").join("Bugs").exists());
}

#[test]
fn merge_removes_the_old_folder_once_empty() {
    let s = Setup::new();
    let mut lib = s.open();
    s.write(&["Álbuns", "Bugs", "a.png"]);
    fs::create_dir_all(s.root().join("Albums")).unwrap();
    lib.sync().unwrap();
    assert_eq!(
        lib.migrate_albums_dir().unwrap(),
        AlbumsMigration::Merged { left: vec![] }
    );
    assert!(!s.root().join("Álbuns").exists());
    assert_eq!(print_named(&lib, "a.png").album.as_deref(), Some("Bugs"));
}

/// While a file inside `Álbuns` is open, Windows refuses to rename the folder.
#[cfg(windows)]
#[test]
fn keeps_using_the_old_folder_while_it_is_busy() {
    let s = Setup::new();
    let busy = {
        let lib = s.open();
        let busy = s.write(&["Álbuns", "Bugs", "2026-10-01_08-00-00.png"]);
        lib.sync().unwrap();
        busy
    };
    let handle = fs::File::open(&busy).unwrap();
    let lib = s.open();
    assert_eq!(lib.albums_dir(), s.root().join("Álbuns"));
    assert_eq!(lib.albums().unwrap(), vec!["Bugs".to_string()]);
    assert_eq!(
        print_named(&lib, "2026-10-01_08-00-00.png")
            .album
            .as_deref(),
        Some("Bugs")
    );
    lib.create_album("Memes").unwrap();
    assert!(s.root().join("Álbuns").join("Memes").is_dir());
    drop(lib);
    drop(handle);

    // Next time it works.
    let lib = s.open();
    assert_eq!(lib.albums_dir(), s.root().join("Albums"));
    assert!(!s.root().join("Álbuns").exists());
    assert_eq!(
        lib.albums().unwrap(),
        vec!["Bugs".to_string(), "Memes".to_string()]
    );
    let p = print_named(&lib, "2026-10-01_08-00-00.png");
    assert_eq!(p.album.as_deref(), Some("Bugs"));
    assert!(p.path.starts_with(s.root().join("Albums")));
}

#[test]
fn relocating_onto_an_old_library_migrates_it() {
    let s = Setup::new();
    let mut lib = s.open();
    let other = s.dir.path().join("other");
    let old = other.join("Álbuns").join("Bugs");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("a.png"), png()).unwrap();

    lib.relocate(other.clone(), false).unwrap();
    lib.sync().unwrap();

    assert!(!other.join("Álbuns").exists());
    assert_eq!(lib.albums_dir(), other.join("Albums"));
    assert_eq!(print_named(&lib, "a.png").album.as_deref(), Some("Bugs"));
}
