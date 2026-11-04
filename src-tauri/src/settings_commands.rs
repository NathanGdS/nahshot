//! Tauri commands behind the settings and welcome windows. Every setter saves the
//! settings immediately and broadcasts them as `settings-changed`.

use crate::commands::{blocking, CmdResult};
use crate::settings::{apply_print_screen_default, Settings, Theme};
use crate::state::{self, AppState};
use crate::{start_watcher, sync_autostart};
use log::{error, warn};
use nahshot_capture::hotkey::parse_accelerator;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::PoisonError;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Outcome of moving the prints folder, as the settings window reports it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDirReport {
    /// Prints moved into the new folder.
    pub moved: usize,
    /// Prints that could not be moved and stay in the old folder.
    pub failed: usize,
}

/// The current settings.
#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Settings {
    app.state::<AppState>()
        .settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Registers or unregisters nahshot to start with Windows.
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        sync_autostart(app, enabled).map_err(|e| e.to_string())?;
        state.update_settings(app, |s| {
            s.autostart = enabled;
            Ok(())
        })
    })
    .await
}

/// Takes Print Screen from Windows (or gives it back), restoring the previous value.
#[tauri::command]
pub async fn set_default_print_screen(app: AppHandle, enabled: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        state.update_settings(app, |s| {
            apply_print_screen_default(enabled, s, &state.overlay).map_err(|e| e.to_string())
        })
    })
    .await
}

/// Validates and installs a new alternative capture hotkey; returns it as stored.
#[tauri::command]
pub async fn set_alt_hotkey(app: AppHandle, accelerator: String) -> CmdResult<String> {
    blocking(app, move |app, state| {
        let accelerator = accelerator.trim().to_owned();
        parse_accelerator(&accelerator).map_err(|e| e.to_string())?;
        state.update_settings(app, |s| {
            s.alt_hotkey = accelerator.clone();
            state.overlay.set_hotkeys(s.hotkey_config());
            Ok(accelerator)
        })
    })
    .await
}

/// Changes the colour scheme of every window.
#[tauri::command]
pub async fn set_theme(app: AppHandle, theme: Theme) -> CmdResult<()> {
    blocking(app, move |app, state| {
        state.update_settings(app, |s| {
            s.theme = theme;
            Ok(())
        })
    })
    .await
}

/// Asks for a new prints folder; `None` when the dialog is cancelled.
#[tauri::command]
pub async fn pick_prints_dir(app: AppHandle, window: WebviewWindow) -> CmdResult<Option<PathBuf>> {
    blocking(app, move |app, state| {
        let current = state
            .settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .prints_dir
            .clone();
        app.dialog()
            .file()
            .set_title("Escolha a pasta dos prints")
            .set_directory(current)
            .set_parent(&window)
            .blocking_pick_folder()
            .map(|picked| picked.into_path().map_err(|e| e.to_string()))
            .transpose()
    })
    .await
}

/// Makes `new` the prints folder, moving the existing prints there if asked.
///
/// Prints that stay behind (not moved, or failed to move) remain in the gallery.
#[tauri::command]
pub async fn change_prints_dir(
    app: AppHandle,
    new: PathBuf,
    move_existing: bool,
) -> CmdResult<ChangeDirReport> {
    blocking(app, move |app, state| {
        std::fs::create_dir_all(&new).map_err(|e| e.to_string())?;
        // Stop watching the old folder so the move itself does not trigger resyncs.
        drop(
            state
                .watcher
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        let (root, relocated) = {
            let mut lib = state.lib.lock().unwrap_or_else(PoisonError::into_inner);
            // Register files added since the last sync, so they are moved too.
            if let Err(e) = lib.sync() {
                warn!("sync before relocating: {e}");
            }
            let relocated = lib.relocate(new, move_existing);
            (lib.root().to_path_buf(), relocated)
        };
        // The old folder stays in scope: prints left there are still shown.
        if let Err(e) = app.asset_protocol_scope().allow_directory(&root, true) {
            error!("asset scope for {}: {e}", root.display());
        }
        *state.watcher.lock().unwrap_or_else(PoisonError::into_inner) = start_watcher(app, &root);
        let report = relocated.map_err(|e| e.to_string())?;
        for path in &report.failed {
            warn!("could not move {}, left in place", path.display());
        }
        state.update_settings(app, |s| {
            s.prints_dir = root;
            Ok(())
        })?;
        state::resync(app);
        Ok(ChangeDirReport {
            moved: report.moved,
            failed: report.failed.len(),
        })
    })
    .await
}

/// Ends the first run: marks the welcome as seen (whatever the answer), takes Print
/// Screen if `enable_default`, then closes the welcome window.
#[tauri::command]
pub async fn finish_welcome(app: AppHandle, enable_default: bool) -> CmdResult<()> {
    blocking(app, move |app, state| {
        let applied = state.update_settings(app, |s| {
            s.first_run_done = true;
            Ok(if enable_default {
                apply_print_screen_default(true, s, &state.overlay).map_err(|e| e.to_string())
            } else {
                Ok(())
            })
        })?;
        applied?;
        if let Some(window) = app.get_webview_window("welcome") {
            window.close().map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
}
