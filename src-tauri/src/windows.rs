//! The webview windows. Each exists only while open: closing destroys it.

use crate::settings::Theme;
use crate::state::{self, AppState};
use log::error;
use nahshot_i18n::{lang, t};
use std::sync::PoisonError;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// One kind of webview window.
struct Page {
    label: &'static str,
    file: &'static str,
    /// The window title in the current language.
    title: fn() -> String,
    size: (f64, f64),
}

const GALLERY: Page = Page {
    label: "gallery",
    file: "gallery.html",
    title: || t("window.gallery"),
    size: (1100.0, 720.0),
};
const SETTINGS: Page = Page {
    label: "settings",
    file: "settings.html",
    title: || t("window.settings"),
    size: (560.0, 600.0),
};
const WELCOME: Page = Page {
    label: "welcome",
    file: "welcome.html",
    title: || t("window.welcome"),
    size: (520.0, 420.0),
};
const PAGES: [Page; 3] = [GALLERY, SETTINGS, WELCOME];

/// Opens (or focuses) the gallery. A newly opened gallery triggers a folder rescan.
pub fn open_gallery(app: &AppHandle) {
    if open(app, &GALLERY) {
        state::gallery_opened(app);
    }
}

/// Opens (or focuses) the settings window.
pub fn open_settings(app: &AppHandle) {
    open(app, &SETTINGS);
}

/// Opens (or focuses) the first-run welcome window.
pub fn open_welcome(app: &AppHandle) {
    open(app, &WELCOME);
}

/// Reloads every open window in the current language, with its title.
pub fn relocalize(app: &AppHandle) {
    for page in &PAGES {
        let Some(window) = app.get_webview_window(page.label) else {
            continue;
        };
        if let Err(e) = window.set_title(&(page.title)()) {
            error!("retitling window {}: {e}", page.label);
        }
        let reloaded = window.url().and_then(|mut url| {
            url.set_query(Some(&query(app)));
            window.navigate(url)
        });
        if let Err(e) = reloaded {
            error!("reloading window {}: {e}", page.label);
        }
    }
}

/// The query every page starts from: `theme=…&lang=…`.
fn query(app: &AppHandle) -> String {
    format!(
        "theme={}&lang={}",
        current_theme(app).as_str(),
        lang().code()
    )
}

/// Focuses the window of `page`, or builds it; `true` only when a new window was built.
fn open(app: &AppHandle, page: &Page) -> bool {
    let label = page.label;
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        if let Err(e) = window.set_focus() {
            error!("focusing window {label}: {e}");
        }
        return false;
    }
    let (width, height) = page.size;
    let url = format!("{}?{}", page.file, query(app));
    let built = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title((page.title)())
        .inner_size(width, height)
        .center()
        // Tauri's file-drop handler would swallow the gallery's HTML5 drag and drop.
        .disable_drag_drop_handler()
        .build();
    built
        .inspect_err(|e| error!("opening window {label}: {e}"))
        .is_ok()
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
