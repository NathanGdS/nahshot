//! Screen capture overlay for nahshot: selection geometry, annotation model and rendering.

pub mod annotate;
pub mod font;
pub mod geom;
pub mod grab;
pub mod overlay;
pub mod render;

/// Errors raised while capturing the screen or reacting to hotkeys.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// A monitor could not be enumerated or captured.
    #[error("screen grab failed: {0}")]
    Grab(String),
    /// A file-system or font-loading failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A global hotkey could not be registered.
    #[error("hotkey error: {0}")]
    Hotkey(String),
}
