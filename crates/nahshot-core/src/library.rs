//! Gallery operations: capture import, rename, folder-backed albums, delete and query.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::db::Db;
use crate::model::{Filter, Print, PrintId, PrintRow, Query, SortOrder};
use crate::naming::{
    albums_dir, file_name_for, month_dir, unique_path, validate_album_name, validate_file_stem,
};
use crate::{CoreError, Result};

/// The screenshot library: image files under `root` plus their metadata.
pub struct Library {
    root: PathBuf,
    db: Db,
}

/// Returns the album a file lives in, i.e. `Some(name)` for `root\Álbuns\name\file`.
pub fn album_of(root: &Path, path: &Path) -> Option<String> {
    let dir = path.parent()?;
    if dir.parent()? != albums_dir(root) {
        return None;
    }
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

impl Library {
    /// Opens the library rooted at `root` with its metadata in `db_path`.
    pub fn open(root: PathBuf, db_path: &Path) -> Result<Library> {
        Ok(Library {
            root: std::path::absolute(root)?,
            db: Db::open(db_path)?,
        })
    }

    /// Root folder of the library.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Stores a captured PNG in its month folder and registers it.
    pub fn import_capture(&self, png: &[u8], at: NaiveDateTime) -> Result<Print> {
        let dir = month_dir(&self.root, at);
        fs::create_dir_all(&dir)?;
        let path = write_new(&dir, &file_name_for(at), png)?;
        let id = self.db.insert(&path, at)?;
        self.get(id)
    }

    /// Fetches one print.
    pub fn get(&self, id: PrintId) -> Result<Print> {
        Ok(self.print(self.db.get(id)?))
    }

    /// Runs a gallery query.
    pub fn query(&self, q: &Query) -> Result<Vec<Print>> {
        let needle = q
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let mut prints: Vec<Print> = self
            .db
            .all()?
            .into_iter()
            .map(|row| self.print(row))
            .filter(|p| matches_filter(p, &q.filter))
            .filter(|p| needle.as_deref().is_none_or(|n| matches_search(p, n)))
            .collect();
        if q.order == SortOrder::NewestFirst {
            prints.reverse();
        }
        Ok(prints)
    }

    /// Renames a print's file, keeping it in the same folder.
    pub fn rename(&self, id: PrintId, new_stem: &str) -> Result<Print> {
        let stem = validate_file_stem(new_stem)?;
        let row = self.db.get(id)?;
        let dir = row.path.parent().unwrap_or(&self.root);
        let new_name = format!("{stem}.png");
        let only_case_differs = row
            .path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&new_name));
        let target = if only_case_differs {
            // Renaming onto itself is not a clash, so skip the unique-name logic.
            let target = dir.join(&new_name);
            fs::rename(&row.path, &target)?;
            target
        } else {
            move_file(&row.path, dir, &new_name)?
        };
        self.db.update_path(id, &target)?;
        self.get(id)
    }

    /// Sets or clears the favorite flag.
    pub fn set_favorite(&self, id: PrintId, favorite: bool) -> Result<()> {
        self.db.set_favorite(id, favorite)
    }

    /// Attaches a tag to a print.
    pub fn add_tag(&self, id: PrintId, tag: &str) -> Result<()> {
        self.db.add_tag(id, tag)
    }

    /// Detaches a tag from a print.
    pub fn remove_tag(&self, id: PrintId, tag: &str) -> Result<()> {
        self.db.remove_tag(id, tag)
    }

    /// Lists tags in use, alphabetically.
    pub fn tags(&self) -> Result<Vec<String>> {
        self.db.tags()
    }

    /// Lists album folders alphabetically; empty when none exist yet.
    pub fn albums(&self) -> Result<Vec<String>> {
        let entries = match fs::read_dir(albums_dir(&self.root)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort_by_key(|n| n.to_lowercase());
        Ok(names)
    }

    /// Creates an album folder and returns its validated name.
    pub fn create_album(&self, name: &str) -> Result<String> {
        let name = validate_album_name(name)?;
        fs::create_dir_all(albums_dir(&self.root))?;
        match fs::create_dir(self.album_dir(&name)) {
            Ok(()) => Ok(name),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(CoreError::AlbumExists(name)),
            Err(e) => Err(e.into()),
        }
    }

    /// Renames an album folder and re-points the prints inside it.
    pub fn rename_album(&self, old: &str, new: &str) -> Result<String> {
        let old = self.resolve_album(old)?;
        let new = validate_album_name(new)?;
        let (from, to) = (self.album_dir(&old), self.album_dir(&new));
        if to.exists() && !old.eq_ignore_ascii_case(&new) {
            return Err(CoreError::AlbumExists(new));
        }
        fs::rename(&from, &to)?;
        for row in self.db.all()? {
            if album_of(&self.root, &row.path).as_deref() == Some(old.as_str()) {
                if let Some(file_name) = row.path.file_name() {
                    self.db.update_path(row.id, &to.join(file_name))?;
                }
            }
        }
        Ok(new)
    }

    /// Dissolves an album: its prints go back to their month folders.
    ///
    /// The folder itself is removed only if nothing else is left inside it.
    pub fn delete_album(&self, name: &str) -> Result<()> {
        let name = self.resolve_album(name)?;
        let dir = self.album_dir(&name);
        for row in self.db.all()? {
            if album_of(&self.root, &row.path).as_deref() == Some(name.as_str()) {
                self.move_to_month(&row)?;
            }
        }
        match fs::remove_dir(&dir) {
            Err(e) if e.kind() != io::ErrorKind::DirectoryNotEmpty => Err(e.into()),
            _ => Ok(()),
        }
    }

    /// Moves prints into an existing album folder.
    pub fn move_to_album(&self, ids: &[PrintId], album: &str) -> Result<()> {
        let album = self.resolve_album(album)?;
        let dir = self.album_dir(&album);
        for &id in ids {
            let row = self.db.get(id)?;
            if row.path.parent() != Some(dir.as_path()) {
                self.move_row(&row, &dir)?;
            }
        }
        Ok(())
    }

    /// Moves prints out of their album back into their month folders.
    pub fn remove_from_album(&self, ids: &[PrintId]) -> Result<()> {
        for &id in ids {
            let row = self.db.get(id)?;
            if album_of(&self.root, &row.path).is_some() {
                self.move_to_month(&row)?;
            }
        }
        Ok(())
    }

    /// Sends prints to the Recycle Bin and forgets them.
    pub fn delete(&self, ids: &[PrintId]) -> Result<()> {
        for &id in ids {
            let row = self.db.get(id)?;
            if row.path.exists() {
                trash::delete(&row.path).map_err(|e| io::Error::other(e.to_string()))?;
            }
            self.db.remove(id)?;
        }
        Ok(())
    }

    /// Maps a caller-supplied album name to the folder name actually on disk.
    ///
    /// Windows folders are case-insensitive, so the match ignores case.
    fn resolve_album(&self, name: &str) -> Result<String> {
        let name = validate_album_name(name)?;
        self.albums()?
            .into_iter()
            .find(|a| a.to_lowercase() == name.to_lowercase())
            .ok_or(CoreError::AlbumNotFound(name))
    }

    fn album_dir(&self, name: &str) -> PathBuf {
        albums_dir(&self.root).join(name)
    }

    fn move_to_month(&self, row: &PrintRow) -> Result<()> {
        let dir = month_dir(&self.root, row.captured_at);
        fs::create_dir_all(&dir)?;
        self.move_row(row, &dir)
    }

    fn move_row(&self, row: &PrintRow, dir: &Path) -> Result<()> {
        let file_name = row
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| file_name_for(row.captured_at));
        let target = move_file(&row.path, dir, &file_name)?;
        self.db.update_path(row.id, &target)
    }

    fn print(&self, row: PrintRow) -> Print {
        Print {
            id: row.id,
            file_name: row
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            album: album_of(&self.root, &row.path),
            path: row.path,
            captured_at: row.captured_at,
            favorite: row.favorite,
            tags: row.tags,
        }
    }
}

fn matches_filter(p: &Print, filter: &Filter) -> bool {
    match filter {
        Filter::All => true,
        Filter::Favorites => p.favorite,
        Filter::Album(name) => p.album.as_deref() == Some(name.as_str()),
        Filter::Tag(tag) => p.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
    }
}

/// `needle` must already be lowercase.
fn matches_search(p: &Print, needle: &str) -> bool {
    p.file_name.to_lowercase().contains(needle)
        || p.tags.iter().any(|t| t.to_lowercase().contains(needle))
        || p.album
            .as_deref()
            .is_some_and(|a| a.to_lowercase().contains(needle))
}

/// Atomically claims a free path in `dir` by creating an empty file there.
///
/// Retries with the next suffix if another writer takes the name first.
fn reserve(dir: &Path, file_name: &str) -> Result<(PathBuf, fs::File)> {
    loop {
        let path = unique_path(dir, file_name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

/// Writes `bytes` to a new, never-overwritten file in `dir`.
fn write_new(dir: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let (path, mut file) = reserve(dir, file_name)?;
    if let Err(e) = file.write_all(bytes) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(e.into());
    }
    Ok(path)
}

/// Moves `from` into `dir` under a free name; falls back to copy + delete across devices.
fn move_file(from: &Path, dir: &Path, file_name: &str) -> Result<PathBuf> {
    // The reserved placeholder is replaced by the rename, so the name stays ours.
    let (to, placeholder) = reserve(dir, file_name)?;
    drop(placeholder);
    let moved = match fs::rename(from, &to) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            fs::copy(from, &to).and_then(|_| fs::remove_file(from))
        }
        other => other,
    };
    if let Err(e) = moved {
        let _ = fs::remove_file(&to);
        return Err(e.into());
    }
    Ok(to)
}
