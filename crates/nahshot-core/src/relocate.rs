//! Moving the library to a new root folder.

use std::fs;
use std::path::{Path, PathBuf};

use crate::library::Library;
use crate::model::PrintRow;
use crate::Result;

/// What a [`Library::relocate`] run did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RelocateReport {
    /// Prints moved under the new root.
    pub moved: usize,
    /// Prints that could not be moved; they stay where they were, still registered.
    pub failed: Vec<PathBuf>,
}

impl Library {
    /// Switches the library to `new_root`.
    ///
    /// With `move_existing`, every print under the old root is moved to the same
    /// relative place under the new one (`AAAA-MM\`, `Álbuns\<nome>\`). A print that
    /// cannot be moved is counted and left in place; nothing is ever overwritten or
    /// deleted. Prints outside the old root are not touched either way.
    pub fn relocate(&mut self, new_root: PathBuf, move_existing: bool) -> Result<RelocateReport> {
        let new_root = std::path::absolute(new_root)?;
        let mut report = RelocateReport::default();
        if move_existing && !same_folder(&self.root, &new_root) {
            for row in self.db.all()? {
                let Some(dir) = target_dir(&self.root, &new_root, &row.path) else {
                    continue;
                };
                match self.move_into(&row, &dir) {
                    Ok(()) => report.moved += 1,
                    Err(_) => report.failed.push(row.path),
                }
            }
        }
        self.root = new_root;
        Ok(report)
    }

    fn move_into(&self, row: &PrintRow, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir)?;
        self.move_row(row, dir)
    }
}

/// The folder under `new_root` matching the one `path` has under `old_root`, or
/// `None` when `path` is not inside `old_root` (or is already inside `new_root`).
fn target_dir(old_root: &Path, new_root: &Path, path: &Path) -> Option<PathBuf> {
    if path.starts_with(new_root) {
        return None;
    }
    let relative_dir = path.strip_prefix(old_root).ok()?.parent()?;
    Some(new_root.join(relative_dir))
}

/// Whether two paths name the same folder, ignoring case and spelling differences
/// when both exist.
fn same_folder(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
