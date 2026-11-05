//! Error type shared by every `nahshot-core` operation.

/// Errors produced by `nahshot-core`.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite failure.
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    /// Image encoding or decoding failure.
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    /// Filesystem watcher failure.
    #[error("watch error: {0}")]
    Watch(#[from] notify::Error),
    /// A file or album name that violates the naming rules.
    #[error("invalid name: {0}")]
    InvalidName(String),
    /// An album with that name already exists.
    #[error("album already exists: {0}")]
    AlbumExists(String),
    /// No album with that name exists.
    #[error("album not found: {0}")]
    AlbumNotFound(String),
    /// The file was saved at `path` but registering it in the store failed.
    #[error("saved to {} but not registered: {source}", path.display())]
    Unregistered {
        /// Where the file was written.
        path: std::path::PathBuf,
        /// Why registering it failed.
        source: Box<CoreError>,
    },
    /// No print with that id exists.
    #[error("print not found: {0}")]
    PrintNotFound(i64),
}

/// Convenience alias using [`CoreError`].
pub type Result<T> = std::result::Result<T, CoreError>;
