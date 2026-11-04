//! Reconciles the metadata store with what is actually on disk.

use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime};
use walkdir::WalkDir;

use crate::library::Library;
use crate::naming::parse_captured_at;
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
    /// Rows whose file still exists are never touched, even outside the current root.
    pub fn sync(&self) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        for row in self.db.all()? {
            if !row.path.exists() {
                self.db.remove(row.id)?;
                report.removed += 1;
            }
        }
        let known = self.db.all()?.len();
        for entry in WalkDir::new(&self.root).max_depth(MAX_DEPTH) {
            // Unreadable entries are skipped; the next sync retries them.
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() || !is_png(entry.path()) {
                continue;
            }
            let at = captured_at(entry.path());
            self.db.insert(entry.path(), at)?;
        }
        report.added = self.db.all()?.len() - known;
        Ok(report)
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
