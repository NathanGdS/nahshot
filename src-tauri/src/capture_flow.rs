//! What happens to a finished capture: save to the gallery, then copy or "save as".

use crate::state::AppState;
use arboard::{Clipboard, ImageData};
use chrono::NaiveDateTime;
use image::{ImageFormat, RgbaImage};
use log::{error, info};
use nahshot_capture::overlay::{Action, CaptureResult};
use nahshot_core::naming::{file_name_for, unique_path};
use std::borrow::Cow;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::PoisonError;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

/// Event telling open windows that the gallery contents changed.
pub const LIBRARY_CHANGED: &str = "library-changed";

/// Builds the overlay's result callback. It only spawns a worker: the overlay
/// thread also runs the keyboard hook, so blocking it would stall system input.
pub fn dispatcher(app: AppHandle) -> impl Fn(CaptureResult) + Send + 'static {
    move |r| {
        let app = app.clone();
        let spawned = std::thread::Builder::new()
            .name("nahshot-capture-result".into())
            .spawn(move || handle(&app, r));
        if let Err(e) = spawned {
            error!("capture result dropped, no worker thread: {e}");
        }
    }
}

/// Saves a capture to the gallery (or to `unsaved\` when that fails), then runs its action.
pub fn handle(app: &AppHandle, r: CaptureResult) {
    let Some(img) = r.image else { return };
    let png = encode_png(&img);
    let name = file_name_for(r.captured_at);
    match import(app, &png, r.captured_at) {
        Ok(()) => {
            if let Err(e) = app.emit(LIBRARY_CHANGED, ()) {
                error!("emitting {LIBRARY_CHANGED}: {e}");
            }
        }
        Err(e) => {
            error!("import into the gallery failed: {e}");
            save_unsaved(app, &png, &name);
        }
    }
    match r.action {
        Action::Copy => copy_to_clipboard(&img),
        Action::SaveAs => save_as(app, &png, &name),
        Action::Close => {}
    }
}

/// Encodes `img` as PNG.
pub fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    img.write_to(&mut png, ImageFormat::Png)
        .expect("PNG encoding into memory cannot fail");
    png.into_inner()
}

fn import(app: &AppHandle, png: &[u8], at: NaiveDateTime) -> nahshot_core::Result<()> {
    let state = app.state::<AppState>();
    let lib = state.lib.lock().unwrap_or_else(PoisonError::into_inner);
    let print = lib.import_capture(png, at)?;
    info!("capture saved to {}", print.path.display());
    Ok(())
}

/// Keeps a capture the gallery folder refused in `<app_local_data_dir>\unsaved\`.
fn save_unsaved(app: &AppHandle, png: &[u8], name: &str) {
    let saved = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())
        .and_then(|dir| {
            let dir = dir.join("unsaved");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = unique_path(&dir, name);
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
            Ok::<PathBuf, String>(path)
        });
    let body = match saved {
        Ok(path) => format!(
            "Não foi possível salvar na pasta dos prints; o print foi guardado em {}",
            path.display()
        ),
        Err(e) => {
            error!("fallback save failed: {e}");
            format!("Não foi possível salvar o print: {e}")
        }
    };
    notify(app, &body);
}

fn copy_to_clipboard(img: &RgbaImage) {
    let data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    };
    if let Err(e) = Clipboard::new().and_then(|mut c| c.set_image(data)) {
        error!("copying the capture to the clipboard: {e}");
    }
}

/// Asks where to put an extra copy of the PNG. Blocks this worker until answered.
fn save_as(app: &AppHandle, png: &[u8], name: &str) {
    let Some(target) = app
        .dialog()
        .file()
        .set_file_name(name)
        .add_filter("PNG", &["png"])
        .blocking_save_file()
    else {
        return;
    };
    let result = target
        .into_path()
        .map_err(|e| e.to_string())
        .and_then(|path| std::fs::write(&path, png).map_err(|e| e.to_string()));
    if let Err(e) = result {
        error!("save as failed: {e}");
        notify(app, &format!("Não foi possível salvar a cópia: {e}"));
    }
}

fn notify(app: &AppHandle, body: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("nahshot")
        .body(body)
        .show()
    {
        error!("notification failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_png_roundtrip() {
        let mut img = RgbaImage::new(7, 3);
        img.put_pixel(6, 2, image::Rgba([10, 20, 30, 255]));
        let png = encode_png(&img);
        assert!(png.starts_with(b"\x89PNG"));
        let back = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(back.dimensions(), (7, 3));
        assert_eq!(back, img);
    }
}
