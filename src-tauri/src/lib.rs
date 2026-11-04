//! The nahshot desktop app: tray icon, settings and the capture-to-gallery flow.

pub mod capture_flow;
pub mod settings;
pub mod state;
pub mod tray;
pub mod windows;

use log::{error, info, warn};
use nahshot_capture::overlay::spawn_overlay;
use nahshot_core::library::Library;
use nahshot_core::watch::{watch, LibraryWatcher};
use nahshot_core::ThumbCache;
use settings::Settings;
use state::AppState;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Builds and runs the app until "Sair" is chosen in the tray.
///
/// # Panics
/// If the Tauri runtime cannot start or the library database cannot be opened.
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tauri::Builder::default()
        // Must come first: a second launch only opens the gallery of the running one.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_gallery(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| setup(app.handle()))
        .build(tauri::generate_context!())
        .expect("failed to start nahshot")
        .run(|_app, event| {
            // Closing the last window keeps the app in the tray; only `app.exit` quits.
            if let RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let paths = app.path();
    let settings = Settings::load(&paths.app_config_dir()?.join("settings.json"));
    let data_dir = paths.app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    apply_autostart(app, settings.autostart);

    let lib = Library::open(settings.prints_dir.clone(), &data_dir.join("nahshot.db"))?;
    let thumbs = ThumbCache::new(data_dir.join("thumbs"))?;
    let overlay = spawn_overlay(capture_flow::dispatcher(app.clone()));
    overlay.set_hotkeys(settings.hotkey_config());
    let prints_dir = settings.prints_dir.clone();
    let first_run_done = settings.first_run_done;
    app.manage(AppState {
        lib: Mutex::new(lib),
        thumbs,
        settings: Mutex::new(settings),
        overlay,
        watcher: Mutex::new(None),
    });

    let handle = app.clone();
    std::thread::Builder::new()
        .name("nahshot-sync".into())
        .spawn(move || state::resync(&handle))?;
    *app.state::<AppState>()
        .watcher
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = start_watcher(app, &prints_dir);

    tray::build(app)?;
    if !first_run_done {
        windows::open_welcome(app);
    }
    Ok(())
}

/// Watches the prints folder, creating it first. Failures are only logged: the app
/// still runs, and captures fall back to `unsaved\`.
fn start_watcher(app: &AppHandle, prints_dir: &Path) -> Option<LibraryWatcher> {
    if let Err(e) = std::fs::create_dir_all(prints_dir) {
        warn!(
            "prints folder {} unavailable, not watching: {e}",
            prints_dir.display()
        );
        return None;
    }
    let handle = app.clone();
    watch(prints_dir, move || state::resync(&handle))
        .inspect_err(|e| warn!("watching {}: {e}", prints_dir.display()))
        .ok()
}

/// Makes the "start with Windows" registration match the setting.
fn apply_autostart(app: &AppHandle, enabled: bool) {
    let autolaunch = app.autolaunch();
    let result = autolaunch
        .is_enabled()
        .and_then(|current| match (enabled, current) {
            (true, false) => autolaunch.enable(),
            (false, true) => autolaunch.disable(),
            _ => Ok(()),
        });
    match result {
        Ok(()) => info!("autostart {}", if enabled { "on" } else { "off" }),
        Err(e) => error!("autostart: {e}"),
    }
}
