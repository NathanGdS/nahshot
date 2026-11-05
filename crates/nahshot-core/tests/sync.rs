mod common;

use std::sync::mpsc;
use std::time::Duration;

use nahshot_core::library::Library;
use nahshot_core::model::Query;
use nahshot_core::sync::SyncReport;
use nahshot_core::watch::watch;

use common::{lib, png};

#[test]
fn sync_adds_png_from_explorer_and_ignores_other_files() {
    let (_d, lib) = lib();
    let m = lib.root().join("2026-10");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("2026-10-01_08-00-00.png"), png()).unwrap();
    std::fs::write(m.join("FOTO.PNG"), png()).unwrap();
    std::fs::write(m.join("notas.txt"), b"x").unwrap();
    std::fs::write(m.join("desktop.ini"), b"x").unwrap();
    std::fs::write(m.join("Thumbs.db"), b"x").unwrap();
    assert_eq!(
        lib.sync().unwrap(),
        SyncReport {
            added: 2,
            removed: 0
        }
    );
    let all = lib.query(&Query::default()).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all
        .iter()
        .any(|p| p.captured_at.to_string() == "2026-10-01 08:00:00"));
}

#[test]
fn sync_uses_mtime_when_name_has_no_timestamp() {
    let (_d, lib) = lib();
    let m = lib.root().join("2026-10");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("FOTO.PNG"), png()).unwrap();
    lib.sync().unwrap();
    let p = &lib.query(&Query::default()).unwrap()[0];
    let age = chrono::Local::now().naive_local() - p.captured_at;
    assert!(
        age.num_seconds().abs() < 60,
        "captured_at {}",
        p.captured_at
    );
}

#[test]
fn sync_picks_up_prints_inside_album_folders() {
    let (_d, lib) = lib();
    let a = lib.root().join("Álbuns").join("Memes");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::write(a.join("2026-10-01_08-00-00.png"), png()).unwrap();
    assert_eq!(lib.sync().unwrap().added, 1);
    let p = &lib.query(&Query::default()).unwrap()[0];
    assert_eq!(p.album.as_deref(), Some("Memes"));
}

#[test]
fn sync_removes_orphans_and_keeps_tags_of_survivors() {
    let (_d, lib) = lib();
    let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let a = lib.import_capture(&png(), at).unwrap();
    let b = lib.import_capture(&png(), at).unwrap();
    lib.add_tag(a.id, "keep").unwrap();
    lib.add_tag(b.id, "gone").unwrap();
    std::fs::remove_file(&b.path).unwrap();
    assert_eq!(
        lib.sync().unwrap(),
        SyncReport {
            added: 0,
            removed: 1
        }
    );
    let all = lib.query(&Query::default()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, a.id);
    assert_eq!(all[0].tags, vec!["keep".to_owned()]);
}

#[test]
fn sync_keeps_prints_outside_current_root() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.sqlite");
    let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let a = Library::open(dir.path().join("rootA"), &db).unwrap();
    let p = a.import_capture(&png(), at).unwrap();
    drop(a);
    let b = Library::open(dir.path().join("rootB"), &db).unwrap();
    assert_eq!(b.sync().unwrap(), SyncReport::default());
    assert_eq!(b.query(&Query::default()).unwrap()[0].id, p.id);
}

#[test]
fn album_folder_created_in_explorer_shows_up() {
    let (_d, lib) = lib();
    std::fs::create_dir_all(lib.root().join("Álbuns").join("Memes")).unwrap();
    assert_eq!(lib.albums().unwrap(), vec!["Memes".to_owned()]);
}

#[test]
fn sync_is_idempotent() {
    let (_d, lib) = lib();
    let m = lib.root().join("2026-10");
    std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("a.png"), png()).unwrap();
    assert_eq!(lib.sync().unwrap().added, 1);
    assert_eq!(lib.sync().unwrap(), SyncReport::default());
}

#[test]
fn sync_does_not_readd_file_whose_path_differs_only_in_case() {
    let (_d, lib) = lib();
    let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap();
    let p = lib.import_capture(&png(), at).unwrap();
    let upper = p.path.with_file_name("2026-11-04_10-00-00.PNG");
    std::fs::rename(&p.path, &upper).unwrap();
    assert_eq!(lib.sync().unwrap(), SyncReport::default());
    assert_eq!(lib.query(&Query::default()).unwrap().len(), 1);
}

#[test]
fn watcher_fires_on_new_file() {
    let (_d, lib) = lib();
    std::fs::create_dir_all(lib.root()).unwrap();
    let (tx, rx) = mpsc::channel();
    let _watcher = watch(lib.root(), move || {
        let _ = tx.send(());
    })
    .unwrap();
    std::fs::write(lib.root().join("new.png"), png()).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
}

fn t_at() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 11, 4)
        .unwrap()
        .and_hms_opt(10, 0, 0)
        .unwrap()
}

#[test]
fn sync_keeps_rows_while_root_is_unreachable() {
    let (d, lib) = lib();
    let p = lib.import_capture(&png(), t_at()).unwrap();
    lib.add_tag(p.id, "keep").unwrap();
    lib.set_favorite(p.id, true).unwrap();
    // The drive is unplugged: the whole root vanishes for a while.
    let away = d.path().join("away");
    std::fs::rename(lib.root(), &away).unwrap();
    assert_eq!(lib.sync().unwrap(), SyncReport::default());
    std::fs::rename(&away, lib.root()).unwrap();
    assert_eq!(lib.sync().unwrap(), SyncReport::default());
    let all = lib.query(&Query::default()).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, p.id);
    assert_eq!(all[0].tags, vec!["keep".to_owned()]);
    assert!(all[0].favorite);
}

#[test]
fn sync_removes_row_under_root_when_its_month_folder_was_deleted() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t_at()).unwrap();
    std::fs::remove_dir_all(p.path.parent().unwrap()).unwrap();
    assert_eq!(lib.sync().unwrap().removed, 1);
}

#[test]
fn sync_keeps_rows_outside_root_whose_folder_is_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.sqlite");
    let a = Library::open(dir.path().join("rootA"), &db).unwrap();
    let p = a.import_capture(&png(), t_at()).unwrap();
    drop(a);
    std::fs::rename(dir.path().join("rootA"), dir.path().join("away")).unwrap();
    let b = Library::open(dir.path().join("rootB"), &db).unwrap();
    std::fs::create_dir_all(b.root()).unwrap();
    assert_eq!(b.sync().unwrap(), SyncReport::default());
    assert_eq!(b.query(&Query::default()).unwrap()[0].id, p.id);
}

#[test]
fn sync_removes_rows_outside_root_when_only_the_file_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("db.sqlite");
    let a = Library::open(dir.path().join("rootA"), &db).unwrap();
    let p = a.import_capture(&png(), t_at()).unwrap();
    drop(a);
    std::fs::remove_file(&p.path).unwrap();
    let b = Library::open(dir.path().join("rootB"), &db).unwrap();
    assert_eq!(b.sync().unwrap().removed, 1);
    assert!(b.query(&Query::default()).unwrap().is_empty());
}

#[test]
fn sync_registers_many_files_in_one_go() {
    let (_d, lib) = lib();
    let m = lib.root().join("2026-10");
    std::fs::create_dir_all(&m).unwrap();
    let bytes = png();
    for i in 0..200 {
        std::fs::write(m.join(format!("p{i}.png")), &bytes).unwrap();
    }
    assert_eq!(lib.sync().unwrap().added, 200);
    assert_eq!(lib.paths().unwrap().len(), 200);
}
