//! Reconciles the metadata store with what is actually on disk.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use walkdir::WalkDir;

use crate::library::{move_file, Library};
use crate::naming::{month_dir, parse_captured_at};
use crate::Result;

/// Deepest level scanned: `root/month/file` and `root/Álbuns/name/file`.
const MAX_DEPTH: usize = 3;

/// What a [`Library::sync`] run changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Files found on disk and newly registered.
    pub added: usize,
    /// Rows dropped because their file is gone.
    pub removed: usize,
}

impl Library {
    /// Brings the store in line with the folder: forgets vanished files, adopts new PNGs.
    ///
    /// A row is only forgotten when its folder is reachable but the file is gone: while
    /// the root (or, outside it, the print's own folder) is missing, e.g. on an unplugged
    /// drive, its rows and their tags are kept. Runs as one transaction.
    pub fn sync(&self) -> Result<SyncReport> {
        let root_reachable = self.root.is_dir();
        self.db.transaction(|db| {
            let mut report = SyncReport::default();
            for (id, path) in db.all_paths()? {
                if !path.exists() && self.gone_for_good(&path, root_reachable) {
                    db.remove(id)?;
                    report.removed += 1;
                }
            }
            let known = db.count()?;
            for entry in WalkDir::new(&self.root).max_depth(MAX_DEPTH) {
                // Unreadable entries are skipped; the next sync retries them.
                let Ok(entry) = entry else { continue };
                if entry.file_type().is_file() && is_png(entry.path()) {
                    db.insert(entry.path(), captured_at(entry.path()))?;
                }
            }
            report.added = db.count()? - known;
            Ok(report)
        })
    }

    /// Moves every PNG directly inside `dir` into its month folder (never overwriting)
    /// and registers it; returns how many were adopted. A missing `dir` adopts nothing.
    ///
    /// Used for captures that were kept aside when the prints folder refused them.
    pub fn adopt(&self, dir: &Path) -> Result<usize> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let mut adopted = 0;
        for entry in entries {
            let path = entry?.path();
            if !path.is_file() || !is_png(&path) {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let at = captured_at(&path);
            let month = month_dir(&self.root, at);
            fs::create_dir_all(&month)?;
            let target = move_file(&path, &month, &name)?;
            self.db.insert(&target, at)?;
            adopted += 1;
        }
        Ok(adopted)
    }

    /// Whether a missing file at `path` was really removed, rather than living on a
    /// folder or drive that is unreachable right now.
    fn gone_for_good(&self, path: &Path, root_reachable: bool) -> bool {
        if path.starts_with(&self.root) {
            root_reachable
        } else {
            path.parent().is_some_and(Path::is_dir)
        }
    }
}

fn is_png(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
}

/// Capture time from the file name, falling back to the modification time.
fn captured_at(path: &Path) -> NaiveDateTime {
    path.file_name()
        .and_then(|n| parse_captured_at(&n.to_string_lossy()))
        .or_else(|| {
            let modified = path.metadata().and_then(|m| m.modified()).ok()?;
            Some(local(modified))
        })
        .unwrap_or_else(|| Local::now().naive_local())
}

fn local(t: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(t).naive_local()
}
