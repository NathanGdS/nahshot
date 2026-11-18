//! Moving the album folder from its old Portuguese name to the fixed English one.
//!
//! Up to v0.1 albums lived in `root\Álbuns`; they now live in `root\Albums`, whatever
//! the UI language (the UI shows the translated name). Libraries are migrated when
//! opened, without ever overwriting or deleting a file.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use log::{info, warn};

use crate::library::Library;
use crate::naming::{albums_dir, legacy_albums_dir};
use crate::Result;

/// What [`Library::migrate_albums_dir`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum AlbumsMigration {
    /// There was no `Álbuns` folder.
    NotNeeded,
    /// `Álbuns` was renamed to `Albums`.
    Renamed,
    /// Both folders existed: the albums were merged into `Albums`. Files whose name was
    /// taken there (`left`) stay in `Álbuns`.
    Merged { left: Vec<PathBuf> },
    /// `Álbuns` could not be renamed (e.g. open in Explorer): it stays in use until the
    /// next try.
    Deferred,
}

impl Library {
    /// Moves `root\Álbuns` to `root\Albums` and re-points the prints inside it, keeping
    /// their tags and favorites. Sets the albums folder the library uses from then on.
    ///
    /// When `Albums` does not exist yet, the whole folder is renamed and the database
    /// updated in one transaction; if the database refuses, the rename is undone. When
    /// both exist, each album (or, for an album on both sides, each file) is moved
    /// across unless its name is taken; the old folder goes away only once empty.
    pub fn migrate_albums_dir(&mut self) -> Result<AlbumsMigration> {
        let legacy = legacy_albums_dir(&self.root);
        let target = albums_dir(&self.root);
        self.albums = target.clone();
        if !legacy.is_dir() {
            return Ok(AlbumsMigration::NotNeeded);
        }
        if !target.exists() {
            if let Err(e) = fs::rename(&legacy, &target) {
                warn!(
                    "renaming {} to Albums: {e}; trying again next time",
                    legacy.display()
                );
                self.albums = legacy;
                return Ok(AlbumsMigration::Deferred);
            }
            if let Err(e) = self.repoint(&legacy, &target) {
                if fs::rename(&target, &legacy).is_ok() {
                    self.albums = legacy;
                }
                return Err(e);
            }
            info!("album folder renamed to {}", target.display());
            return Ok(AlbumsMigration::Renamed);
        }
        let left = self.merge(&legacy, &target)?;
        if left.is_empty() {
            info!("album folder merged into {}", target.display());
        } else {
            warn!(
                "{} file(s) left in {}: their names are taken in Albums",
                left.len(),
                legacy.display()
            );
        }
        Ok(AlbumsMigration::Merged { left })
    }

    /// Merges every album of `legacy` into `target`; returns the files left behind.
    fn merge(&self, legacy: &Path, target: &Path) -> Result<Vec<PathBuf>> {
        let mut left = Vec::new();
        for entry in fs::read_dir(legacy)? {
            let entry = entry?;
            let from = entry.path();
            let to = target.join(entry.file_name());
            if !entry.file_type()?.is_dir() {
                // A stray file next to the albums: not an album, so it stays.
                left.push(from);
                continue;
            }
            if !to.exists() {
                match fs::rename(&from, &to) {
                    Ok(()) => self.repoint(&from, &to)?,
                    Err(e) => {
                        warn!("moving album {}: {e}", from.display());
                        left.push(from);
                    }
                }
                continue;
            }
            for file in fs::read_dir(&from)? {
                let file = file?.path();
                let dest = to.join(file.file_name().unwrap_or_default());
                if dest.exists() {
                    left.push(file);
                    continue;
                }
                match fs::rename(&file, &dest) {
                    Ok(()) => self.repoint(&file, &dest)?,
                    Err(e) => {
                        warn!("moving {}: {e}", file.display());
                        left.push(file);
                    }
                }
            }
            remove_if_empty(&from)?;
        }
        remove_if_empty(legacy)?;
        Ok(left)
    }

    /// Re-points every row at or under `from` (a file or folder) to the same place
    /// under `to`, in one transaction. Rows marked missing are re-pointed too, so a
    /// file that comes back later still finds its tags.
    fn repoint(&self, from: &Path, to: &Path) -> Result<()> {
        self.db.transaction(|db| {
            for row in db.tracked()? {
                if let Ok(rest) = row.path.strip_prefix(from) {
                    let new = if rest.as_os_str().is_empty() {
                        to.to_path_buf()
                    } else {
                        to.join(rest)
                    };
                    db.update_path(row.id, &new)?;
                }
            }
            Ok(())
        })
    }
}

/// Removes the folder `dir` if nothing is left inside it.
fn remove_if_empty(dir: &Path) -> Result<()> {
    match fs::remove_dir(dir) {
        Err(e) if e.kind() != io::ErrorKind::DirectoryNotEmpty => Err(e.into()),
        _ => Ok(()),
    }
}
