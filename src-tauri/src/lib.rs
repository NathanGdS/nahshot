//! The nahshot desktop app: tray icon, settings and the capture-to-gallery flow.

pub mod asset_scope;
pub mod capture_flow;
pub mod commands;
pub mod settings;
pub mod settings_commands;
pub mod state;
pub mod tray;
pub mod updater;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| setup(app.handle()))
        .invoke_handler(tauri::generate_handler![
            commands::list_prints,
            commands::ensure_thumbs,
            commands::get_sidebar,
            commands::rename_print,
            commands::set_favorite,
            commands::add_tag,
            commands::remove_tag,
            commands::create_album,
            commands::rename_album,
            commands::delete_album,
            commands::move_to_album,
            commands::remove_from_album,
            commands::delete_prints,
            commands::copy_print,
            commands::reveal_print,
            commands::open_settings,
            settings_commands::get_settings,
            settings_commands::set_autostart,
            settings_commands::set_default_print_screen,
            settings_commands::set_alt_hotkey,
            settings_commands::set_theme,
            settings_commands::pick_prints_dir,
            settings_commands::change_prints_dir,
            settings_commands::finish_welcome,
            updater::check_update,
            updater::install_update,
        ])
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
    let settings_path = paths.app_config_dir()?.join("settings.json");
    let settings = Settings::load(&settings_path);
    let data_dir = paths.app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    match sync_autostart(app, settings.autostart) {
        Ok(()) => info!(
            "autostart {}",
            if settings.autostart { "on" } else { "off" }
        ),
        Err(e) => error!("autostart: {e}"),
    }

    let lib = Library::open(settings.prints_dir.clone(), &data_dir.join("nahshot.db"))?;
    let thumbs_dir = data_dir.join("thumbs");
    let thumbs = ThumbCache::new(thumbs_dir.clone())?;
    // The prints folder can be anywhere, so the webview's asset scope is set here
    // rather than statically: thumbnails in the grid, full images in the viewer.
    asset_scope::allow_library(app, &lib, &thumbs_dir)?;
    let overlay = spawn_overlay(capture_flow::dispatcher(app.clone()));
    settings::install_hotkeys(&overlay, settings.hotkey_config());
    let prints_dir = settings.prints_dir.clone();
    let first_run_done = settings.first_run_done;
    app.manage(AppState {
        lib: Mutex::new(lib),
        thumbs,
        settings: Mutex::new(settings),
        settings_path,
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
    updater::check_on_startup(app);
    Ok(())
}

/// Watches the prints folder, creating it first. Failures are only logged: the app
/// still runs, and captures fall back to `unsaved\`.
pub(crate) fn start_watcher(app: &AppHandle, prints_dir: &Path) -> Option<LibraryWatcher> {
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

/// Makes the "start with Windows" registration match `enabled`.
pub(crate) fn sync_autostart(
    app: &AppHandle,
    enabled: bool,
) -> Result<(), tauri_plugin_autostart::Error> {
    let autolaunch = app.autolaunch();
    match (enabled, autolaunch.is_enabled()?) {
        (true, false) => autolaunch.enable(),
        (false, true) => autolaunch.disable(),
        _ => Ok(()),
    }
}
