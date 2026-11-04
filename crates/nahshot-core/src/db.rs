//! SQLite-backed metadata store.

use std::path::Path;

use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{PrintId, PrintRow};
use crate::{CoreError, Result};

const TS_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS prints (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    captured_at TEXT NOT NULL,
    favorite INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS print_tags (
    print_id INTEGER NOT NULL REFERENCES prints(id) ON DELETE CASCADE,
    tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (print_id, tag_id)
);
PRAGMA user_version = 1;
";

/// Metadata store for prints and their tags.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (creating if needed) the database file at `path`.
    pub fn open(path: &Path) -> Result<Db> {
        Self::init(Connection::open(path)?)
    }

    /// Opens a throwaway in-memory database.
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Db { conn })
    }

    /// Registers a print, returning the existing id when the path is already known.
    pub fn insert(&self, path: &Path, captured_at: NaiveDateTime) -> Result<PrintId> {
        let path = path.to_string_lossy();
        self.conn.execute(
            "INSERT OR IGNORE INTO prints (path, captured_at) VALUES (?1, ?2)",
            params![path, captured_at.format(TS_FORMAT).to_string()],
        )?;
        let id = self
            .conn
            .query_row("SELECT id FROM prints WHERE path = ?1", [path], |r| {
                r.get(0)
            })?;
        Ok(PrintId(id))
    }

    /// Fetches one print with its tags.
    pub fn get(&self, id: PrintId) -> Result<PrintRow> {
        self.conn
            .query_row(
                "SELECT id, path, captured_at, favorite FROM prints WHERE id = ?1",
                [id.0],
                |r| self.row(r),
            )
            .optional()?
            .ok_or(CoreError::PrintNotFound(id.0))
    }

    /// Looks a print up by its (case-insensitive) path.
    pub fn find_by_path(&self, path: &Path) -> Result<Option<PrintId>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM prints WHERE path = ?1",
                [path.to_string_lossy()],
                |r| r.get(0),
            )
            .optional()?
            .map(PrintId))
    }

    /// Lists every print, oldest first.
    pub fn all(&self) -> Result<Vec<PrintRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, captured_at, favorite FROM prints ORDER BY captured_at, id",
        )?;
        let rows = stmt.query_map([], |r| self.row(r))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Points a print at a new file path.
    pub fn update_path(&self, id: PrintId, path: &Path) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), id.0],
        )?;
        Self::expect_one(n, id)
    }

    /// Deletes a print and its tag links.
    pub fn remove(&self, id: PrintId) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM prints WHERE id = ?1", [id.0])?;
        Self::expect_one(n, id)
    }

    /// Sets or clears the favorite flag.
    pub fn set_favorite(&self, id: PrintId, favorite: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET favorite = ?1 WHERE id = ?2",
            params![favorite, id.0],
        )?;
        Self::expect_one(n, id)
    }

    /// Attaches a tag (case-insensitive, idempotent) to a print.
    pub fn add_tag(&self, id: PrintId, tag: &str) -> Result<()> {
        self.get(id)?;
        self.conn
            .execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [tag])?;
        self.conn.execute(
            "INSERT OR IGNORE INTO print_tags (print_id, tag_id)
             SELECT ?1, id FROM tags WHERE name = ?2",
            params![id.0, tag],
        )?;
        Ok(())
    }

    /// Detaches a tag from a print.
    pub fn remove_tag(&self, id: PrintId, tag: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM print_tags WHERE print_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![id.0, tag],
        )?;
        Ok(())
    }

    /// Lists tags in use by at least one print, alphabetically.
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM tags
             WHERE id IN (SELECT tag_id FROM print_tags) ORDER BY name",
        )?;
        let names = stmt.query_map([], |r| r.get(0))?;
        Ok(names.collect::<rusqlite::Result<_>>()?)
    }

    fn expect_one(changed: usize, id: PrintId) -> Result<()> {
        if changed == 0 {
            Err(CoreError::PrintNotFound(id.0))
        } else {
            Ok(())
        }
    }

    fn row(&self, r: &rusqlite::Row<'_>) -> rusqlite::Result<PrintRow> {
        let id: i64 = r.get(0)?;
        let path: String = r.get(1)?;
        let captured: String = r.get(2)?;
        let captured_at = NaiveDateTime::parse_from_str(&captured, TS_FORMAT).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?;
        let mut stmt = self.conn.prepare_cached(
            "SELECT t.name FROM tags t JOIN print_tags pt ON pt.tag_id = t.id
             WHERE pt.print_id = ?1 ORDER BY t.name",
        )?;
        let tags = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(PrintRow {
            id: PrintId(id),
            path: path.into(),
            captured_at,
            favorite: r.get(3)?,
            tags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn t0() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 2)
            .unwrap()
            .and_hms_opt(3, 4, 5)
            .unwrap()
    }

    #[test]
    fn insert_is_idempotent_by_path_case_insensitive() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert(Path::new(r"C:\x\A.png"), t0()).unwrap();
        let b = db.insert(Path::new(r"c:\x\a.png"), t0()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn tags_and_favorite() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert(Path::new("p.png"), t0()).unwrap();
        db.add_tag(id, "bug").unwrap();
        db.add_tag(id, "BUG").unwrap();
        db.add_tag(id, "figma").unwrap();
        db.set_favorite(id, true).unwrap();
        let r = db.get(id).unwrap();
        assert!(r.favorite);
        assert_eq!(r.tags, vec!["bug", "figma"]);
        db.remove_tag(id, "bug").unwrap();
        assert_eq!(db.tags().unwrap(), vec!["figma"]);
    }

    #[test]
    fn remove_cascades_tags() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert(Path::new("p.png"), t0()).unwrap();
        db.add_tag(id, "x").unwrap();
        db.remove(id).unwrap();
        assert!(db.tags().unwrap().is_empty());
        assert!(matches!(db.get(id), Err(CoreError::PrintNotFound(_))));
    }

    #[test]
    fn reopen_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");
        {
            let db = Db::open(&file).unwrap();
            db.insert(Path::new("p.png"), t0()).unwrap();
        }
        let db = Db::open(&file).unwrap();
        let all = db.all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].captured_at, t0());
    }

    #[test]
    fn update_path_and_find_by_path() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert(Path::new("a.png"), t0()).unwrap();
        db.update_path(id, Path::new("b.png")).unwrap();
        assert_eq!(db.find_by_path(Path::new("B.PNG")).unwrap(), Some(id));
        assert_eq!(db.find_by_path(Path::new("a.png")).unwrap(), None);
    }
}
