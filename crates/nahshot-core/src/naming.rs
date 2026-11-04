//! File and album naming rules.

use crate::{CoreError, Result};
use chrono::NaiveDateTime;
use std::path::{Path, PathBuf};

const TIMESTAMP_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";
const MAX_NAME_CHARS: usize = 100;
const FORBIDDEN_CHARS: &str = "<>:\"/\\|?*";
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Builds the capture file name for a timestamp, e.g. `2026-11-04_14-32-10.png`.
pub fn file_name_for(t: NaiveDateTime) -> String {
    format!("{}.png", t.format(TIMESTAMP_FORMAT))
}

/// Parses the capture time from `YYYY-MM-DD_HH-MM-SS.png` or `..._N.png` (any extension case).
pub fn parse_captured_at(file_name: &str) -> Option<NaiveDateTime> {
    let path = Path::new(file_name);
    if !path.extension()?.eq_ignore_ascii_case("png") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    if let Ok(t) = NaiveDateTime::parse_from_str(stem, TIMESTAMP_FORMAT) {
        return Some(t);
    }
    let (base, suffix) = stem.rsplit_once('_')?;
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    NaiveDateTime::parse_from_str(base, TIMESTAMP_FORMAT).ok()
}

/// Monthly folder for a capture: `root\YYYY-MM`.
pub fn month_dir(root: &Path, t: NaiveDateTime) -> PathBuf {
    root.join(t.format("%Y-%m").to_string())
}

/// Folder that holds the album directories: `root\Álbuns`.
pub fn albums_dir(root: &Path) -> PathBuf {
    root.join("Álbuns")
}

/// Returns `dir/file_name`, or `stem_2.ext`, `stem_3.ext`, ... if the name is taken.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let ext = path.extension().and_then(|e| e.to_str());
    (2..)
        .map(|n| match ext {
            Some(ext) => dir.join(format!("{stem}_{n}.{ext}")),
            None => dir.join(format!("{stem}_{n}")),
        })
        .find(|p| !p.exists())
        .expect("an unbounded range always yields a free name")
}

/// Trims and validates an album name against Windows file name rules.
pub fn validate_album_name(name: &str) -> Result<String> {
    validate_name(name)
}

/// Trims and validates a file stem used when renaming a print.
pub fn validate_file_stem(stem: &str) -> Result<String> {
    validate_name(stem)
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    let invalid = |reason: &str| Err(CoreError::InvalidName(format!("{name:?}: {reason}")));
    if name.is_empty() {
        return invalid("name is empty");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return invalid("name is longer than 100 characters");
    }
    if name
        .chars()
        .any(|c| c.is_control() || FORBIDDEN_CHARS.contains(c))
    {
        return invalid("contains a forbidden character");
    }
    if name.ends_with('.') {
        return invalid("ends with a dot");
    }
    let base = name.split('.').next().unwrap_or(name);
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(base)) {
        return invalid("reserved Windows name");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn file_name_format() {
        let t = NaiveDate::from_ymd_opt(2026, 11, 4)
            .unwrap()
            .and_hms_opt(14, 32, 10)
            .unwrap();
        assert_eq!(file_name_for(t), "2026-11-04_14-32-10.png");
    }

    #[test]
    fn parse_roundtrip_and_suffix() {
        let t = parse_captured_at("2026-11-04_14-32-10_2.PNG").unwrap();
        assert_eq!(t.to_string(), "2026-11-04 14:32:10");
        assert!(parse_captured_at("foto.png").is_none());
    }

    #[test]
    fn unique_path_adds_suffix() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), b"").unwrap();
        std::fs::write(d.path().join("a_2.png"), b"").unwrap();
        assert_eq!(unique_path(d.path(), "a.png"), d.path().join("a_3.png"));
    }

    #[test]
    fn album_name_rules() {
        for bad in [
            "", "   ", "a/b", "a\\b", "x:y", "q?", "con", "CON", "lpt1", "con.txt", "nome.", "a*b",
            "a<b", "a|b", "a\"b",
        ] {
            assert!(validate_album_name(bad).is_err(), "{bad:?} deveria falhar");
        }
        assert_eq!(validate_album_name("  Trabalho ").unwrap(), "Trabalho");
        assert!(validate_album_name(&"x".repeat(101)).is_err());
        assert_eq!(validate_album_name("Ação & Bugs").unwrap(), "Ação & Bugs");
    }

    #[test]
    fn month_and_album_dirs() {
        let t = NaiveDate::from_ymd_opt(2026, 1, 9)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(month_dir(Path::new("R"), t), Path::new("R").join("2026-01"));
        assert_eq!(albums_dir(Path::new("R")), Path::new("R").join("Álbuns"));
    }
}
