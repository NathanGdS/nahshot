//! Shared state managed by Tauri for the whole app lifetime.

use crate::capture_flow::LIBRARY_CHANGED;
use crate::settings::Settings;
use log::{error, info};
use nahshot_capture::overlay::OverlayHandle;
use nahshot_core::library::Library;
use nahshot_core::watch::LibraryWatcher;
use nahshot_core::ThumbCache;
use std::sync::{Mutex, PoisonError};
use tauri::{AppHandle, Emitter, Manager};

/// Everything the tray, the capture flow and (later) the windows share.
pub struct AppState {
    pub lib: Mutex<Library>,
    pub thumbs: ThumbCache,
    pub settings: Mutex<Settings>,
    pub overlay: OverlayHandle,
    /// Folder watch on the prints directory; `None` if it could not be started.
    pub watcher: Mutex<Option<LibraryWatcher>>,
}

/// Reconciles the library with its folder and tells open windows about it.
/// Does nothing if the state is not managed yet.
pub fn resync(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let report = state
        .lib
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .sync();
    match report {
        Ok(r) => {
            info!("library sync: +{} -{}", r.added, r.removed);
            if let Err(e) = app.emit(LIBRARY_CHANGED, ()) {
                error!("emitting {LIBRARY_CHANGED}: {e}");
            }
        }
        Err(e) => error!("library sync failed: {e}"),
    }
}
