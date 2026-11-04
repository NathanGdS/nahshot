//! The webview windows. Each exists only while open: closing destroys it.

use crate::settings::Theme;
use crate::state::AppState;
use log::error;
use std::sync::PoisonError;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Opens (or focuses) the gallery.
pub fn open_gallery(app: &AppHandle) {
    open(app, "gallery", "gallery.html", "nahshot", (1100.0, 720.0));
}

/// Opens (or focuses) the settings window.
pub fn open_settings(app: &AppHandle) {
    open(
        app,
        "settings",
        "settings.html",
        "nahshot · Configurações",
        (560.0, 520.0),
    );
}

/// Opens (or focuses) the first-run welcome window.
pub fn open_welcome(app: &AppHandle) {
    open(
        app,
        "welcome",
        "welcome.html",
        "Bem-vindo ao nahshot",
        (520.0, 420.0),
    );
}

fn open(app: &AppHandle, label: &str, page: &str, title: &str, (width, height): (f64, f64)) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        if let Err(e) = window.set_focus() {
            error!("focusing window {label}: {e}");
        }
        return;
    }
    let url = format!("{page}?theme={}", current_theme(app).as_str());
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(width, height)
        .center()
        // Tauri's file-drop handler would swallow the gallery's HTML5 drag and drop.
        .disable_drag_drop_handler()
        .build();
    if let Err(e) = built {
        error!("opening window {label}: {e}");
    }
}

/// The theme from the settings, or the system one before the state exists.
fn current_theme(app: &AppHandle) -> Theme {
    app.try_state::<AppState>().map_or(Theme::System, |s| {
        s.settings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .theme
    })
}
