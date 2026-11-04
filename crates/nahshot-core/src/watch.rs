//! Filesystem watcher that reports changes in the library folder.

use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::Result;

const DEBOUNCE: Duration = Duration::from_millis(500);

/// Keeps a folder watch alive; dropping it stops watching.
pub struct LibraryWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

/// Watches `root` recursively and calls `on_change` once per burst of changes.
pub fn watch(root: &Path, on_change: impl Fn() + Send + 'static) -> Result<LibraryWatcher> {
    let mut debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
        if result.is_ok_and(|events| !events.is_empty()) {
            on_change();
        }
    })?;
    debouncer.watcher().watch(root, RecursiveMode::Recursive)?;
    Ok(LibraryWatcher {
        _debouncer: debouncer,
    })
}
