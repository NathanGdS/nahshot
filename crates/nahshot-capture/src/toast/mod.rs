//! The "screenshot saved" toast: a small native card shown after a capture is saved.

mod model;
mod paint;
mod window;

pub use model::ToastKind;
pub use window::{dismiss, show};
