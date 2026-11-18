//! The system tray icon and its menu.

use crate::state::AppState;
use crate::windows;
use nahshot_i18n::t;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const CAPTURE: &str = "capture";
const GALLERY: &str = "gallery";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";
const TRAY_ID: &str = "main";

/// The tray menu in the current language: Capture, Open gallery, Settings, Quit.
fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: String| MenuItem::with_id(app, id, text, true, None::<&str>);
    Menu::with_items(
        app,
        &[
            &item(CAPTURE, t("tray.capture"))?,
            &item(GALLERY, t("tray.gallery"))?,
            &item(SETTINGS, t("tray.settings"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(QUIT, t("tray.quit"))?,
        ],
    )
}

/// Installs the tray icon and its menu.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("nahshot")
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_icon);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Rebuilds the menu after a language change.
pub fn refresh(app: &AppHandle) -> tauri::Result<()> {
    match app.tray_by_id(TRAY_ID) {
        Some(tray) => tray.set_menu(Some(menu(app)?)),
        None => Ok(()),
    }
}

fn on_menu(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        CAPTURE => app.state::<AppState>().overlay.trigger(),
        GALLERY => windows::open_gallery(app),
        SETTINGS => windows::open_settings(app),
        QUIT => app.exit(0),
        _ => {}
    }
}

fn on_icon(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        windows::open_gallery(tray.app_handle());
    }
}
