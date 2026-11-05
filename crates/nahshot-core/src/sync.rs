//! Reconciles the metadata store with what is actually on disk.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local, NaiveDateTime, TimeDelta};
use walkdir::WalkDir;

use crate::db::Registration;
use crate::library::{move_file, Library};
use crate::naming::{month_dir, parse_captured_at};
use crate::Result;

/// Deepest level scanned: `root/month/file` and `root/Álbuns/name/file`.
const MAX_DEPTH: usize = 3;

/// How long a print whose file is gone keeps its row (and tags) before sync forgets it.
pub const MISSING_RETENTION_DAYS: i64 = 30;

/// What a [`Library::sync`] run changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Files found on disk and newly registered.
    pub added: usize,
    /// Rows newly marked missing (hidden, tags kept) because their file is gone.
    pub removed: usize,
    /// Rows marked missing that found their file again, at the same path or moved.
    pub restored: usize,
}

impl Library {
    /// Brings the store in line with the folder: hides vanished files, adopts new PNGs.
    ///
    /// A print whose file is gone is not deleted but marked missing, so its tags and
    /// favorite flag come back with the file: at the same path, or anywhere under the
    /// root with the same name and capture time (see [`crate::db::Db::register`]).
    /// Rows missing for more than [`MISSING_RETENTION_DAYS`] are forgotten for good.
    ///
    /// A row is only marked when its folder is reachable but the file is gone: while
    /// the root (or, outside it, the print's own folder) is missing, e.g. on an unplugged
    /// drive, its rows stay visible and the retention clock does not start. Runs as one
    /// transaction.
    pub fn sync(&self) -> Result<SyncReport> {
        self.sync_at(Local::now().naive_local())
    }

    /// [`Library::sync`] with an explicit clock: `now` stamps newly missing rows and
    /// decides which ones are past retention.
    pub fn sync_at(&self, now: NaiveDateTime) -> Result<SyncReport> {
        let root_reachable = self.root.is_dir();
        self.db.transaction(|db| {
            let mut report = SyncReport::default();
            // Marked in this run; one found again below was only moved.
            let mut marked = HashSet::new();
            for row in db.tracked()? {
                match (row.missing, row.path.exists()) {
                    (true, true) => {
                        db.mark_present(row.id)?;
                        report.restored += 1;
                    }
                    (false, false) if self.gone_for_good(&row.path, root_reachable) => {
                        db.mark_missing(row.id, now)?;
                        marked.insert(row.id);
                    }
                    _ => {}
                }
            }
            for entry in WalkDir::new(&self.root).max_depth(MAX_DEPTH) {
                // Unreadable entries are skipped; the next sync retries them.
                let Ok(entry) = entry else { continue };
                if !entry.file_type().is_file() || !is_png(entry.path()) {
                    continue;
                }
                match db.register(entry.path(), captured_at(entry.path()))? {
                    (_, Registration::Known) => {}
                    (_, Registration::Added) => report.added += 1,
                    (id, Registration::Restored) => {
                        marked.remove(&id);
                        report.restored += 1;
                    }
                }
            }
            report.removed = marked.len();
            if let Some(cutoff) = now.checked_sub_signed(TimeDelta::days(MISSING_RETENTION_DAYS)) {
                db.purge_missing_before(cutoff)?;
            }
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
    /// folder or drive that is unreachable right now. Keeps an unplugged drive from
    /// hiding its prints and running down their retention period.
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
