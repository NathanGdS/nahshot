use std::path::PathBuf;

use chrono::NaiveDateTime;
use nahshot_core::library::{album_of, Library};
use nahshot_core::model::{Filter, PrintId, Query, SortOrder};
use nahshot_core::CoreError;

fn lib() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(dir.path().join("root"), &dir.path().join("db.sqlite")).unwrap();
    (dir, lib)
}

fn t(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
}

fn t0() -> NaiveDateTime {
    t("2026-11-04 14:32:10")
}

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn ids(lib: &Library, q: &Query) -> Vec<PrintId> {
    lib.query(q).unwrap().into_iter().map(|p| p.id).collect()
}

fn query(filter: Filter, search: Option<&str>, order: SortOrder) -> Query {
    Query {
        filter,
        search: search.map(str::to_owned),
        order,
    }
}

#[test]
fn import_goes_to_month_folder_and_never_overwrites() {
    let (_d, lib) = lib();
    let a = lib.import_capture(&png(), t0()).unwrap();
    let b = lib.import_capture(&png(), t0()).unwrap();
    assert_eq!(
        a.path,
        lib.root().join("2026-11").join("2026-11-04_14-32-10.png")
    );
    assert_eq!(b.file_name, "2026-11-04_14-32-10_2.png");
    assert!(a.path.exists() && b.path.exists());
}

#[test]
fn import_recreates_deleted_root() {
    let (_d, lib) = lib();
    std::fs::remove_dir_all(lib.root()).ok();
    assert!(lib.import_capture(&png(), t0()).unwrap().path.exists());
}

#[test]
fn open_stores_absolute_root() {
    let (_d, lib) = lib();
    assert!(lib.root().is_absolute());
}

#[test]
fn album_move_is_a_real_folder_move() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.create_album("Bugs").unwrap();
    lib.move_to_album(&[p.id], "Bugs").unwrap();
    let p2 = lib.get(p.id).unwrap();
    assert_eq!(p2.album.as_deref(), Some("Bugs"));
    assert_eq!(
        p2.path,
        lib.root().join("Álbuns").join("Bugs").join(&p.file_name)
    );
    assert!(p2.path.exists() && !p.path.exists());
    lib.remove_from_album(&[p.id]).unwrap();
    assert_eq!(lib.get(p.id).unwrap().path, p.path);
    assert!(p.path.exists());
}

#[test]
fn move_into_album_with_same_name_file_gets_suffix() {
    let (_d, lib) = lib();
    let a = lib.import_capture(&png(), t0()).unwrap();
    // Same file name as `a`, but living in a different month folder.
    let b = lib
        .import_capture(&png(), t("2026-12-04 14:32:10"))
        .unwrap();
    let b = lib.rename(b.id, "2026-11-04_14-32-10").unwrap();
    assert_eq!(b.file_name, a.file_name);
    assert_ne!(b.path, a.path);

    lib.create_album("X").unwrap();
    lib.move_to_album(&[a.id, b.id], "X").unwrap();
    let (a2, b2) = (lib.get(a.id).unwrap(), lib.get(b.id).unwrap());
    assert_eq!(a2.file_name, "2026-11-04_14-32-10.png");
    assert_eq!(b2.file_name, "2026-11-04_14-32-10_2.png");
    assert!(a2.path.exists() && b2.path.exists());
}

#[test]
fn invalid_album_name_creates_nothing() {
    let (_d, lib) = lib();
    assert!(matches!(
        lib.create_album("a/b"),
        Err(CoreError::InvalidName(_))
    ));
    assert!(matches!(
        lib.create_album("con"),
        Err(CoreError::InvalidName(_))
    ));
    assert!(lib.albums().unwrap().is_empty());
}

#[test]
fn create_album_twice_fails_and_albums_are_sorted() {
    let (_d, lib) = lib();
    lib.create_album("Zeta").unwrap();
    lib.create_album("Alfa").unwrap();
    assert!(matches!(
        lib.create_album("Alfa"),
        Err(CoreError::AlbumExists(_))
    ));
    assert_eq!(lib.albums().unwrap(), vec!["Alfa", "Zeta"]);
}

#[test]
fn rename_album_updates_paths() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.create_album("A").unwrap();
    lib.move_to_album(&[p.id], "A").unwrap();
    assert_eq!(lib.rename_album("A", "B").unwrap(), "B");
    let p2 = lib.get(p.id).unwrap();
    assert_eq!(p2.album.as_deref(), Some("B"));
    assert!(p2.path.exists());
    assert_eq!(lib.albums().unwrap(), vec!["B"]);
}

#[test]
fn rename_album_onto_existing_fails() {
    let (_d, lib) = lib();
    lib.create_album("A").unwrap();
    lib.create_album("B").unwrap();
    assert!(matches!(
        lib.rename_album("A", "B"),
        Err(CoreError::AlbumExists(_))
    ));
}

#[test]
fn delete_album_returns_prints_to_month_folder() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.create_album("Bugs").unwrap();
    lib.move_to_album(&[p.id], "Bugs").unwrap();
    lib.delete_album("Bugs").unwrap();
    let p2 = lib.get(p.id).unwrap();
    assert_eq!(p2.path, p.path);
    assert!(p2.path.exists());
    assert!(!lib.root().join("Álbuns").join("Bugs").exists());
    assert!(lib.albums().unwrap().is_empty());
}

#[test]
fn delete_album_keeps_folder_with_foreign_files() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.create_album("Bugs").unwrap();
    lib.move_to_album(&[p.id], "Bugs").unwrap();
    let note = lib.root().join("Álbuns").join("Bugs").join("notes.txt");
    std::fs::write(&note, b"keep me").unwrap();
    lib.delete_album("Bugs").unwrap();
    assert!(note.exists());
    let p2 = lib.get(p.id).unwrap();
    assert!(p2.path.exists());
    assert_eq!(p2.album, None);
}

#[test]
fn rename_renames_file_on_disk() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    let r = lib.rename(p.id, "tela de login").unwrap();
    assert_eq!(r.file_name, "tela de login.png");
    assert_eq!(r.path, p.path.with_file_name("tela de login.png"));
    assert!(r.path.exists() && !p.path.exists());
    assert_eq!(lib.get(p.id).unwrap().file_name, "tela de login.png");
    assert!(matches!(
        lib.rename(p.id, "a?b"),
        Err(CoreError::InvalidName(_))
    ));
    assert!(r.path.exists());
}

#[test]
fn rename_onto_taken_name_gets_suffix() {
    let (_d, lib) = lib();
    let a = lib.import_capture(&png(), t0()).unwrap();
    let b = lib.import_capture(&png(), t0()).unwrap();
    lib.rename(a.id, "x").unwrap();
    let rb = lib.rename(b.id, "x").unwrap();
    assert_eq!(rb.file_name, "x_2.png");
}

#[test]
fn query_filters_search_and_order() {
    let (_d, lib) = lib();
    let p1 = lib
        .import_capture(&png(), t("2026-11-04 10:00:00"))
        .unwrap();
    let p2 = lib
        .import_capture(&png(), t("2026-11-04 11:00:00"))
        .unwrap();
    let p3 = lib
        .import_capture(&png(), t("2026-11-04 12:00:00"))
        .unwrap();
    lib.set_favorite(p2.id, true).unwrap();
    lib.add_tag(p3.id, "figma").unwrap();
    lib.create_album("Trabalho").unwrap();
    lib.move_to_album(&[p1.id], "Trabalho").unwrap();

    let newest = SortOrder::NewestFirst;
    let oldest = SortOrder::OldestFirst;
    assert_eq!(
        ids(&lib, &query(Filter::All, None, newest)),
        vec![p3.id, p2.id, p1.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::All, None, oldest)),
        vec![p1.id, p2.id, p3.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::Favorites, None, newest)),
        vec![p2.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::Tag("figma".into()), None, newest)),
        vec![p3.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::Album("Trabalho".into()), None, newest)),
        vec![p1.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::All, Some("FIG"), newest)),
        vec![p3.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::All, Some("trab"), newest)),
        vec![p1.id]
    );
    assert_eq!(
        ids(&lib, &query(Filter::All, Some("2026-11-04"), newest)).len(),
        3
    );
}

#[test]
fn delete_removes_from_gallery() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.delete(&[p.id]).unwrap();
    assert!(matches!(lib.get(p.id), Err(CoreError::PrintNotFound(_))));
    assert!(!p.path.exists());
}

#[test]
fn album_of_detects_direct_children_only() {
    let root = PathBuf::from("R");
    let inside = root.join("Álbuns").join("Bugs").join("a.png");
    assert_eq!(album_of(&root, &inside).as_deref(), Some("Bugs"));
    assert_eq!(album_of(&root, &root.join("2026-11").join("a.png")), None);
    assert_eq!(album_of(&root, &root.join("Álbuns").join("a.png")), None);
}
