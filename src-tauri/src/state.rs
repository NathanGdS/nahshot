//! Shared state managed by Tauri for the whole app lifetime.

use crate::capture_flow::LIBRARY_CHANGED;
use crate::settings::Settings;
use log::{error, info};
use nahshot_capture::overlay::OverlayHandle;
use nahshot_core::library::Library;
use nahshot_core::watch::LibraryWatcher;
use nahshot_core::ThumbCache;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use tauri::{AppHandle, Emitter, Manager};

/// Event carrying the new [`Settings`] after every change, so open windows follow.
pub const SETTINGS_CHANGED: &str = "settings-changed";

/// Everything the tray, the capture flow and (later) the windows share.
pub struct AppState {
    pub lib: Mutex<Library>,
    pub thumbs: ThumbCache,
    pub settings: Mutex<Settings>,
    /// Where `settings` is persisted.
    pub settings_path: PathBuf,
    pub overlay: OverlayHandle,
    /// Folder watch on the prints directory; `None` if it could not be started.
    pub watcher: Mutex<Option<LibraryWatcher>>,
}

impl AppState {
    /// Applies `change` to the settings and, if it succeeds, saves them right away
    /// and tells every open window.
    pub fn update_settings<T>(
        &self,
        app: &AppHandle,
        change: impl FnOnce(&mut Settings) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut settings = self.settings.lock().unwrap_or_else(PoisonError::into_inner);
        let out = change(&mut settings)?;
        settings
            .save(&self.settings_path)
            .map_err(|e| format!("saving settings: {e}"))?;
        if let Err(e) = app.emit(SETTINGS_CHANGED, &*settings) {
            error!("emitting {SETTINGS_CHANGED}: {e}");
        }
        Ok(out)
    }
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
            emit_library_changed(app);
        }
        Err(e) => error!("library sync failed: {e}"),
    }
}

/// Tells every open window that the gallery contents changed.
pub fn emit_library_changed(app: &AppHandle) {
    if let Err(e) = app.emit(LIBRARY_CHANGED, ()) {
        error!("emitting {LIBRARY_CHANGED}: {e}");
    }
}
