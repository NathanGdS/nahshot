//! Domain types shared across the core crate.

use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Identifier of a print in the metadata store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrintId(pub i64);

/// A screenshot as shown in the gallery: row data plus derived file name and album.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Print {
    /// Store identifier.
    pub id: PrintId,
    /// Location of the image file.
    pub path: PathBuf,
    /// Name of the image file, extension included.
    pub file_name: String,
    /// When the screenshot was taken.
    pub captured_at: NaiveDateTime,
    /// Whether the user marked it as favorite.
    pub favorite: bool,
    /// Album (folder) the print lives in, if any.
    pub album: Option<String>,
    /// Tags attached to the print, alphabetically.
    pub tags: Vec<String>,
}

/// Which prints a gallery query selects.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum Filter {
    /// Every print.
    All,
    /// Only favorites.
    Favorites,
    /// Only prints in the named album.
    Album(String),
    /// Only prints carrying the tag.
    Tag(String),
}

/// Ordering of query results by capture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    /// Most recent capture first.
    NewestFirst,
    /// Oldest capture first.
    OldestFirst,
}

/// A gallery query: filter, free-text search and ordering.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    /// Which prints to select.
    pub filter: Filter,
    /// Case-insensitive text matched against file name, tags and album.
    pub search: Option<String>,
    /// Result ordering.
    pub order: SortOrder,
}

impl Default for Query {
    /// Every print, no search, newest first.
    fn default() -> Self {
        Query {
            filter: Filter::All,
            search: None,
            order: SortOrder::NewestFirst,
        }
    }
}

/// A screenshot together with its metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintRow {
    /// Store identifier.
    pub id: PrintId,
    /// Location of the image file.
    pub path: PathBuf,
    /// When the screenshot was taken.
    pub captured_at: NaiveDateTime,
    /// Whether the user marked it as favorite.
    pub favorite: bool,
    /// Tags attached to the print, alphabetically.
    pub tags: Vec<String>,
}
