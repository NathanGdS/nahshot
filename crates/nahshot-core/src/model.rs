//! Domain types shared across the core crate.

use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Identifier of a print in the metadata store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PrintId(pub i64);

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
