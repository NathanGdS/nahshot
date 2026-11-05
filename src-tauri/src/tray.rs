//! The system tray icon and its menu.

use crate::state::AppState;
use crate::windows;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

const CAPTURE: &str = "capture";
const GALLERY: &str = "gallery";
const SETTINGS: &str = "settings";
const QUIT: &str = "quit";

/// Installs the tray icon: Capturar, Abrir galeria, Configurações, Sair.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    let menu = Menu::with_items(
        app,
        &[
            &item(CAPTURE, "Capturar")?,
            &item(GALLERY, "Abrir galeria")?,
            &item(SETTINGS, "Configurações")?,
            &PredefinedMenuItem::separator(app)?,
            &item(QUIT, "Sair")?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("nahshot")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu)
        .on_tray_icon_event(on_icon);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
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
