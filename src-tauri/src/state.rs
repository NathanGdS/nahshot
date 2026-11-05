//! Shared state managed by Tauri for the whole app lifetime.

use crate::capture_flow::{adopt_unsaved, LIBRARY_CHANGED};
use crate::settings::{commit, revert_side_effects, Settings};
use crate::start_watcher;
use log::{error, info, warn};
use nahshot_capture::overlay::OverlayHandle;
use nahshot_core::library::Library;
use nahshot_core::watch::LibraryWatcher;
use nahshot_core::ThumbCache;
use std::collections::HashSet;
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
    /// Applies `change` to a copy of the settings, saves it and only then makes it
    /// live, telling every open window. If saving fails the live settings stay as
    /// they were, and the registry and hotkeys `change` touched are reverted.
    pub fn update_settings<T>(
        &self,
        app: &AppHandle,
        change: impl FnOnce(&mut Settings) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut settings = self.settings.lock().unwrap_or_else(PoisonError::into_inner);
        let out = commit(
            &mut settings,
            &self.settings_path,
            change,
            |before, after| revert_side_effects(before, after, &self.overlay),
        )?;
        if let Err(e) = app.emit(SETTINGS_CHANGED, &*settings) {
            error!("emitting {SETTINGS_CHANGED}: {e}");
        }
        Ok(out)
    }
}

/// Reconciles the library with its folder (first bringing back captures kept in
/// `unsaved\`), drops thumbnails nobody uses and tells open windows about it.
/// Does nothing if the state is not managed yet.
pub fn resync(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let synced = {
        let lib = state.lib.lock().unwrap_or_else(PoisonError::into_inner);
        adopt_unsaved(app, &lib);
        lib.sync().and_then(|r| Ok((r, lib.paths()?)))
    };
    match synced {
        Ok((r, paths)) => {
            info!(
                "library sync: +{} -{} restored {}",
                r.added, r.removed, r.restored
            );
            prune_thumbs(&state.thumbs, &paths);
            emit_library_changed(app);
        }
        Err(e) => error!("library sync failed: {e}"),
    }
}

/// Deletes cached thumbnails that belong to no present print (`prints` comes from
/// [`Library::paths`], which leaves out prints marked missing). Thumbnails of prints
/// that are unreachable or missing right now go too; they are rebuilt when needed.
fn prune_thumbs(thumbs: &ThumbCache, prints: &[PathBuf]) {
    let keep: HashSet<PathBuf> = prints
        .iter()
        .filter_map(|p| thumbs.path_for(p).ok())
        .collect();
    match thumbs.prune(&keep) {
        Ok(0) => {}
        Ok(n) => info!("{n} stale thumbnail(s) removed"),
        Err(e) => warn!("pruning thumbnails: {e}"),
    }
}

/// Spec 6.3: opening the gallery rescans the folder, restarting the folder watch
/// first if it is not running. Works on a short-lived thread.
pub fn gallery_opened(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("nahshot-gallery-scan".into())
        .spawn(move || {
            ensure_watcher(&app);
            resync(&app);
        });
    if let Err(e) = spawned {
        error!("gallery scan not started: {e}");
    }
}

/// Starts watching the prints folder if no watch is running.
fn ensure_watcher(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let mut watcher = state.watcher.lock().unwrap_or_else(PoisonError::into_inner);
    if watcher.is_none() {
        let root = state
            .lib
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .root()
            .to_path_buf();
        *watcher = start_watcher(app, &root);
        if watcher.is_some() {
            info!("watching {} again", root.display());
        }
    }
}

/// Tells every open window that the gallery contents changed.
pub fn emit_library_changed(app: &AppHandle) {
    if let Err(e) = app.emit(LIBRARY_CHANGED, ()) {
        error!("emitting {LIBRARY_CHANGED}: {e}");
    }
}
