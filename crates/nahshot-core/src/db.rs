//! SQLite-backed metadata store.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use rusqlite::{params, Connection, OptionalExtension};

use crate::model::{PrintId, PrintRow};
use crate::{CoreError, Result};

const TS_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

/// Schema version this build reads and writes.
const SCHEMA_VERSION: i64 = 2;

/// Schema of a fresh database. `missing_since` is NULL while the file is present and
/// holds when it was first found gone otherwise; the row and its tags are kept so a
/// file that comes back finds them again.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS prints (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    captured_at TEXT NOT NULL,
    favorite INTEGER NOT NULL DEFAULT 0,
    missing_since TEXT
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
PRAGMA user_version = 2;
";

/// v1 to v2: prints whose file is gone are marked instead of deleted.
const MIGRATE_V1_TO_V2: &str = "
BEGIN;
ALTER TABLE prints ADD COLUMN missing_since TEXT;
PRAGMA user_version = 2;
COMMIT;
";

/// How [`Db::register`] accounted for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// The path was already registered and present.
    Known,
    /// A new row was created.
    Added,
    /// A row marked missing was brought back, with its tags and favorite flag.
    Restored,
}

/// A registered print as sync sees it: present or marked missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tracked {
    /// Store identifier.
    pub id: PrintId,
    /// Last known location of the file.
    pub path: PathBuf,
    /// Whether the row is marked missing.
    pub missing: bool,
}

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
        migrate(&conn)?;
        Ok(Db { conn })
    }

    /// Registers a print, returning the existing id when the path is already known.
    ///
    /// See [`Db::register`] for how rows marked missing are brought back.
    pub fn insert(&self, path: &Path, captured_at: NaiveDateTime) -> Result<PrintId> {
        Ok(self.register(path, captured_at)?.0)
    }

    /// Registers the file at `path`, reusing a row marked missing when it is the
    /// same print: first one with this very path, then one whose file had the same
    /// name (ignoring case) and capture time, the most recently missed first.
    pub fn register(
        &self,
        path: &Path,
        captured_at: NaiveDateTime,
    ) -> Result<(PrintId, Registration)> {
        let text = path.to_string_lossy();
        let same_path: Option<(i64, bool)> = self
            .conn
            .query_row(
                "SELECT id, missing_since IS NOT NULL FROM prints WHERE path = ?1",
                [&text],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match same_path {
            Some((id, false)) => return Ok((PrintId(id), Registration::Known)),
            Some((id, true)) => {
                self.mark_present(PrintId(id))?;
                return Ok((PrintId(id), Registration::Restored));
            }
            None => {}
        }
        let at = captured_at.format(TS_FORMAT).to_string();
        if let Some(id) = self.missing_twin(path, &at)? {
            self.conn.execute(
                "UPDATE prints SET path = ?1, missing_since = NULL WHERE id = ?2",
                params![text, id.0],
            )?;
            return Ok((id, Registration::Restored));
        }
        self.conn.execute(
            "INSERT INTO prints (path, captured_at) VALUES (?1, ?2)",
            params![text, at],
        )?;
        Ok((PrintId(self.conn.last_insert_rowid()), Registration::Added))
    }

    /// The most recently missed row whose file had `path`'s name and was captured at `at`.
    fn missing_twin(&self, path: &Path, at: &str) -> Result<Option<PrintId>> {
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
            return Ok(None);
        };
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, path FROM prints
             WHERE missing_since IS NOT NULL AND captured_at = ?1
             ORDER BY missing_since DESC, id DESC",
        )?;
        let mut rows = stmt.query([at])?;
        while let Some(r) = rows.next()? {
            let old = PathBuf::from(r.get::<_, String>(1)?);
            if old
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_lowercase() == name)
            {
                return Ok(Some(PrintId(r.get(0)?)));
            }
        }
        Ok(None)
    }

    /// Every row with its path and whether it is marked missing, by id.
    pub fn tracked(&self) -> Result<Vec<Tracked>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, missing_since IS NOT NULL FROM prints ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Tracked {
                id: PrintId(r.get(0)?),
                path: PathBuf::from(r.get::<_, String>(1)?),
                missing: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Hides a print whose file is gone, keeping its tags; `now` starts the retention clock.
    pub fn mark_missing(&self, id: PrintId, now: NaiveDateTime) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET missing_since = ?1 WHERE id = ?2",
            params![now.format(TS_FORMAT).to_string(), id.0],
        )?;
        Self::expect_one(n, id)
    }

    /// Shows a print marked missing again.
    pub fn mark_present(&self, id: PrintId) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET missing_since = NULL WHERE id = ?1",
            [id.0],
        )?;
        Self::expect_one(n, id)
    }

    /// Deletes, for good, rows marked missing before `cutoff`; returns how many went.
    pub fn purge_missing_before(&self, cutoff: NaiveDateTime) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM prints WHERE missing_since < ?1",
            [cutoff.format(TS_FORMAT).to_string()],
        )?)
    }

    /// Fetches one present print with its tags.
    pub fn get(&self, id: PrintId) -> Result<PrintRow> {
        self.conn
            .query_row(
                "SELECT id, path, captured_at, favorite FROM prints
                 WHERE id = ?1 AND missing_since IS NULL",
                [id.0],
                |r| self.row(r),
            )
            .optional()?
            .ok_or(CoreError::PrintNotFound(id.0))
    }

    /// Looks a present print up by its (case-insensitive) path.
    pub fn find_by_path(&self, path: &Path) -> Result<Option<PrintId>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM prints WHERE path = ?1 AND missing_since IS NULL",
                [path.to_string_lossy()],
                |r| r.get(0),
            )
            .optional()?
            .map(PrintId))
    }

    /// Lists every present print, oldest first.
    pub fn all(&self) -> Result<Vec<PrintRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, captured_at, favorite FROM prints
             WHERE missing_since IS NULL ORDER BY captured_at, id",
        )?;
        let rows = stmt.query_map([], |r| self.row(r))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Lists every present print's id and path, without tags.
    pub fn all_paths(&self) -> Result<Vec<(PrintId, PathBuf)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path FROM prints WHERE missing_since IS NULL ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok((PrintId(r.get(0)?), PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Number of present prints.
    pub fn count(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM prints WHERE missing_since IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Runs `f` inside one transaction: committed if it succeeds, rolled back otherwise.
    pub fn transaction<T>(&self, f: impl FnOnce(&Db) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("BEGIN")?;
        match f(self) {
            Ok(out) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(out)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Points a print at a new file path.
    pub fn update_path(&self, id: PrintId, path: &Path) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET path = ?1 WHERE id = ?2",
            params![path.to_string_lossy(), id.0],
        )?;
        Self::expect_one(n, id)
    }

    /// Deletes a print and its tag links for good.
    pub fn remove(&self, id: PrintId) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM prints WHERE id = ?1", [id.0])?;
        Self::expect_one(n, id)
    }

    /// Sets or clears the favorite flag of a present print.
    pub fn set_favorite(&self, id: PrintId, favorite: bool) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE prints SET favorite = ?1 WHERE id = ?2 AND missing_since IS NULL",
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

    /// Lists tags in use by at least one present print, alphabetically.
    pub fn tags(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM tags
             WHERE id IN (SELECT pt.tag_id FROM print_tags pt
                          JOIN prints p ON p.id = pt.print_id
                          WHERE p.missing_since IS NULL)
             ORDER BY name",
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

/// Brings the schema to [`SCHEMA_VERSION`]; refuses a database from a newer build.
fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    match version {
        0 => conn.execute_batch(SCHEMA)?,
        1 => {
            if let Err(e) = conn.execute_batch(MIGRATE_V1_TO_V2) {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e.into());
            }
        }
        SCHEMA_VERSION => {}
        newer => return Err(CoreError::UnsupportedSchema(newer)),
    }
    Ok(())
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
    fn all_paths_and_count() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert(Path::new("a.png"), t0()).unwrap();
        let b = db.insert(Path::new("b.png"), t0()).unwrap();
        db.add_tag(a, "x").unwrap();
        assert_eq!(db.count().unwrap(), 2);
        assert_eq!(
            db.all_paths().unwrap(),
            vec![(a, PathBuf::from("a.png")), (b, PathBuf::from("b.png"))]
        );
    }

    #[test]
    fn transaction_commits_or_rolls_back() {
        let db = Db::open_in_memory().unwrap();
        db.transaction(|db| db.insert(Path::new("kept.png"), t0()))
            .unwrap();
        let failed: Result<()> = db.transaction(|db| {
            db.insert(Path::new("dropped.png"), t0())?;
            Err(CoreError::InvalidName("boom".into()))
        });
        assert!(failed.is_err());
        assert_eq!(db.count().unwrap(), 1);
        assert!(db.find_by_path(Path::new("kept.png")).unwrap().is_some());
    }

    #[test]
    fn update_path_and_find_by_path() {
        let db = Db::open_in_memory().unwrap();
        let id = db.insert(Path::new("a.png"), t0()).unwrap();
        db.update_path(id, Path::new("b.png")).unwrap();
        assert_eq!(db.find_by_path(Path::new("B.PNG")).unwrap(), Some(id));
        assert_eq!(db.find_by_path(Path::new("a.png")).unwrap(), None);
    }

    /// The schema as shipped in v1, before prints could be marked missing.
    const SCHEMA_V1: &str = "
        CREATE TABLE prints (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE COLLATE NOCASE,
            captured_at TEXT NOT NULL,
            favorite INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE COLLATE NOCASE
        );
        CREATE TABLE print_tags (
            print_id INTEGER NOT NULL REFERENCES prints(id) ON DELETE CASCADE,
            tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (print_id, tag_id)
        );
        PRAGMA user_version = 1;
        INSERT INTO prints (path, captured_at, favorite) VALUES ('p.png', '2026-01-02T03:04:05', 1);
        INSERT INTO tags (name) VALUES ('bug');
        INSERT INTO print_tags (print_id, tag_id) VALUES (1, 1);
    ";

    fn user_version(file: &Path) -> i64 {
        Connection::open(file)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migrates_a_v1_database_keeping_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");
        Connection::open(&file)
            .unwrap()
            .execute_batch(SCHEMA_V1)
            .unwrap();
        let db = Db::open(&file).unwrap();
        let all = db.all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].captured_at, t0());
        assert!(all[0].favorite);
        assert_eq!(all[0].tags, vec!["bug"]);
        let present: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM prints WHERE missing_since IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(present, 1);
        drop(db);
        assert_eq!(user_version(&file), 2);
        // Opening again is a no-op.
        assert_eq!(Db::open(&file).unwrap().count().unwrap(), 1);
    }

    #[test]
    fn fresh_database_starts_at_v2() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");
        drop(Db::open(&file).unwrap());
        assert_eq!(user_version(&file), 2);
    }

    #[test]
    fn refuses_a_database_from_a_newer_version() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("meta.db");
        Connection::open(&file)
            .unwrap()
            .execute_batch(&format!("{SCHEMA_V1} PRAGMA user_version = 3;"))
            .unwrap();
        assert!(Db::open(&file).is_err());
        assert_eq!(user_version(&file), 3);
    }
}
