//! What happens to a finished capture: save to the gallery, then copy or "save as".

use crate::state::{emit_library_changed, AppState};
use arboard::{Clipboard, ImageData};
use chrono::NaiveDateTime;
use image::{ImageFormat, RgbaImage};
use log::{error, info, warn};
use nahshot_capture::overlay::{Action, CaptureResult};
use nahshot_core::library::{write_new, Library};
use nahshot_core::naming::file_name_for;
use nahshot_core::CoreError;
use std::borrow::Cow;
use std::io::{self, Cursor};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

/// Event telling open windows that the gallery contents changed.
pub const LIBRARY_CHANGED: &str = "library-changed";

/// A unit of work handed to a new thread.
type Job = Box<dyn FnOnce() + Send>;

/// Builds the overlay's result callback. It only spawns a worker: the overlay
/// thread also runs the keyboard hook, so blocking it would stall system input. If
/// no worker can be spawned, the capture is handled right there rather than lost.
pub fn dispatcher(app: AppHandle) -> impl Fn(CaptureResult) + Send + 'static {
    move |r| {
        let app = app.clone();
        let spawn = |job: Job| {
            std::thread::Builder::new()
                .name("nahshot-capture-result".into())
                .spawn(job)
                .map(drop)
        };
        hand_off(r, spawn, move |r| handle(&app, r));
    }
}

/// Runs `work(value)` on the thread `spawn` starts, or inline if it cannot start one.
fn hand_off<T: Send + 'static>(
    value: T,
    spawn: impl FnOnce(Job) -> io::Result<()>,
    work: impl Fn(T) + Clone + Send + 'static,
) {
    // Shared so the value can be taken back when the job never runs.
    let slot = Arc::new(Mutex::new(Some(value)));
    let (job_slot, job_work) = (Arc::clone(&slot), work.clone());
    let job: Job = Box::new(move || {
        if let Some(v) = take(&job_slot) {
            job_work(v);
        }
    });
    if let Err(e) = spawn(job) {
        error!("no worker thread ({e}); handling the capture on the overlay thread");
        if let Some(v) = take(&slot) {
            work(v);
        }
    }
}

fn take<T>(slot: &Mutex<Option<T>>) -> Option<T> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).take()
}

/// Saves a capture to the gallery (or to `unsaved\` when that fails) and runs its
/// action. A copy goes to the clipboard first, so an immediate paste gets it.
pub fn handle(app: &AppHandle, r: CaptureResult) {
    let Some(img) = r.image else { return };
    if r.action == Action::Copy {
        if let Err(e) = copy_to_clipboard(&img) {
            error!("copying the capture to the clipboard: {e}");
            notify(
                app,
                "Não foi possível copiar o print para a área de transferência.",
            );
        }
    }
    let png = encode_png(&img);
    drop(img);
    let name = file_name_for(r.captured_at);
    match import(app, &png, r.captured_at) {
        Imported::Registered => emit_library_changed(app),
        // The PNG is already in the prints folder; the watcher's sync registers it.
        Imported::OnDisk(e) => warn!("{e}; the next sync registers it"),
        Imported::Refused(e) => {
            error!("import into the gallery failed: {e}");
            save_unsaved(app, &png, &name);
        }
    }
    if r.action == Action::SaveAs {
        save_as(app, &png, &name);
    }
}

/// Encodes `img` as PNG.
pub fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut png = Cursor::new(Vec::new());
    img.write_to(&mut png, ImageFormat::Png)
        .expect("PNG encoding into memory cannot fail");
    png.into_inner()
}

/// How far a capture got into the gallery.
enum Imported {
    /// Written and registered.
    Registered,
    /// Written to the prints folder, but not registered yet.
    OnDisk(CoreError),
    /// Not written at all.
    Refused(String),
}

fn import(app: &AppHandle, png: &[u8], at: NaiveDateTime) -> Imported {
    // A capture can finish before setup has managed the state.
    let Some(state) = app.try_state::<AppState>() else {
        return Imported::Refused("the library is not open yet".into());
    };
    let lib = state.lib.lock().unwrap_or_else(PoisonError::into_inner);
    match lib.import_capture(png, at) {
        Ok(print) => {
            info!("capture saved to {}", print.path.display());
            // The folder works again: bring back captures that were kept aside.
            adopt_unsaved(app, &lib);
            Imported::Registered
        }
        Err(e @ CoreError::Unregistered { .. }) => Imported::OnDisk(e),
        Err(e) => Imported::Refused(e.to_string()),
    }
}

/// Where captures the prints folder refused are kept: `<app_local_data_dir>\unsaved\`.
pub fn unsaved_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .inspect_err(|e| error!("app data folder: {e}"))
        .ok()
        .map(|dir| dir.join("unsaved"))
}

/// Moves the captures kept in [`unsaved_dir`] into the gallery. Failures are logged
/// and the files stay where they are for the next try.
pub fn adopt_unsaved(app: &AppHandle, lib: &Library) {
    let Some(dir) = unsaved_dir(app) else { return };
    match lib.adopt(&dir) {
        Ok(0) => {}
        Ok(n) => info!("{n} print(s) from {} added to the gallery", dir.display()),
        Err(e) => warn!("adopting prints from {}: {e}", dir.display()),
    }
}

/// Keeps a capture the gallery folder refused in [`unsaved_dir`], never overwriting.
fn save_unsaved(app: &AppHandle, png: &[u8], name: &str) {
    let saved = unsaved_dir(app)
        .ok_or_else(|| "no app data folder".to_owned())
        .and_then(|dir| {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            write_new(&dir, name, png).map_err(|e| e.to_string())
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

/// Puts `img` on the clipboard as a bitmap.
pub fn copy_to_clipboard(img: &RgbaImage) -> Result<(), arboard::Error> {
    let data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    };
    Clipboard::new().and_then(|mut c| c.set_image(data))
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

    #[test]
    fn hand_off_runs_the_work_on_the_spawned_job() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        hand_off(
            7,
            |job| {
                job();
                Ok(())
            },
            move |v| sink.lock().unwrap().push(v),
        );
        assert_eq!(*seen.lock().unwrap(), vec![7]);
    }

    #[test]
    fn hand_off_runs_inline_when_no_thread_can_start() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        hand_off(
            7,
            |_job| Err(io::Error::other("no threads left")),
            move |v| sink.lock().unwrap().push(v),
        );
        assert_eq!(*seen.lock().unwrap(), vec![7]);
    }
}
