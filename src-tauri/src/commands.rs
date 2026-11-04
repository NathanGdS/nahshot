//! Tauri commands behind the gallery window. Every one returns the error as a
//! message string, and every mutation emits `library-changed`.

use crate::capture_flow::copy_to_clipboard;
use crate::state::{emit_library_changed, AppState};
use crate::windows;
use log::warn;
use nahshot_core::library::Library;
use nahshot_core::model::{Filter, Print, PrintId, Query, SortOrder};
use nahshot_core::ThumbCache;
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::PoisonError;
use tauri::{AppHandle, Manager};

/// A print as the gallery sees it: the print plus the path of its thumbnail.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintDto {
    #[serde(flatten)]
    pub print: Print,
    /// Thumbnail file (the full image if no thumbnail could be made).
    pub thumb: String,
}

/// Contents of the gallery's sidebar.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidebar {
    pub albums: Vec<String>,
    pub tags: Vec<String>,
    /// Number of prints in the library.
    pub total: usize,
    /// Number of favorite prints.
    pub favorites: usize,
}

type CmdResult<T> = Result<T, String>;

/// Runs `f` on a blocking worker, so slow disk work never stalls IPC or the main thread.
async fn blocking<T, F>(app: AppHandle, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &AppState) -> CmdResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&app, &app.state::<AppState>()))
        .await
        .map_err(|e| e.to_string())?
}

/// Runs `f` with the library locked.
fn with_lib<T>(
    state: &AppState,
    f: impl FnOnce(&Library) -> nahshot_core::Result<T>,
) -> CmdResult<T> {
    let lib = state.lib.lock().unwrap_or_else(PoisonError::into_inner);
    f(&lib).map_err(|e| e.to_string())
}

/// Runs a library mutation on a worker, then tells every window about it.
async fn mutate<T, F>(app: AppHandle, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Library) -> nahshot_core::Result<T> + Send + 'static,
{
    blocking(app, |app, state| {
        let out = with_lib(state, f);
        emit_library_changed(app);
        out
    })
    .await
}

/// Pairs a print with its thumbnail, falling back to the full image on failure.
fn to_dto(thumbs: &ThumbCache, print: Print) -> PrintDto {
    let thumb = thumbs.get_or_create(&print.path).unwrap_or_else(|e| {
        warn!("thumbnail for {}: {e}", print.path.display());
        print.path.clone()
    });
    PrintDto {
        thumb: thumb.to_string_lossy().into_owned(),
        print,
    }
}

/// Builds thumbnails for `prints` on every core, keeping their order.
fn to_dtos(thumbs: &ThumbCache, prints: Vec<Print>) -> Vec<PrintDto> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = prints.len().div_ceil(workers).max(1);
    let mut prints = prints.into_iter();
    let chunks = std::iter::from_fn(|| {
        let c: Vec<Print> = prints.by_ref().take(chunk).collect();
        (!c.is_empty()).then_some(c)
    });
    std::thread::scope(|s| {
        let handles: Vec<_> = chunks
            .map(|c| s.spawn(move || c.into_iter().map(|p| to_dto(thumbs, p)).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("thumbnail worker panicked"))
            .collect()
    })
}

/// Runs a gallery query. The library lock is released before thumbnails are made.
#[tauri::command]
pub async fn list_prints(app: AppHandle, query: Query) -> CmdResult<Vec<PrintDto>> {
    blocking(app, move |_, state| {
        let prints = with_lib(state, |lib| lib.query(&query))?;
        Ok(to_dtos(&state.thumbs, prints))
    })
    .await
}

/// Albums, tags and counters for the sidebar.
#[tauri::command]
pub async fn get_sidebar(app: AppHandle) -> CmdResult<Sidebar> {
    blocking(app, |_, state| {
        with_lib(state, |lib| {
            let count = |filter| {
                lib.query(&Query {
                    filter,
                    search: None,
                    order: SortOrder::OldestFirst,
                })
                .map(|v| v.len())
            };
            Ok(Sidebar {
                albums: lib.albums()?,
                tags: lib.tags()?,
                total: count(Filter::All)?,
                favorites: count(Filter::Favorites)?,
            })
        })
    })
    .await
}

/// Renames a print's file to `<stem>.png`.
#[tauri::command]
pub async fn rename_print(app: AppHandle, id: PrintId, stem: String) -> CmdResult<PrintDto> {
    blocking(app, move |app, state| {
        let print = with_lib(state, |lib| lib.rename(id, &stem));
        emit_library_changed(app);
        Ok(to_dto(&state.thumbs, print?))
    })
    .await
}

/// Marks or unmarks a print as favorite.
#[tauri::command]
pub async fn set_favorite(app: AppHandle, id: PrintId, value: bool) -> CmdResult<()> {
    mutate(app, move |lib| lib.set_favorite(id, value)).await
}

/// Attaches a tag to a print.
#[tauri::command]
pub async fn add_tag(app: AppHandle, id: PrintId, tag: String) -> CmdResult<()> {
    mutate(app, move |lib| lib.add_tag(id, &tag)).await
}

/// Detaches a tag from a print.
#[tauri::command]
pub async fn remove_tag(app: AppHandle, id: PrintId, tag: String) -> CmdResult<()> {
    mutate(app, move |lib| lib.remove_tag(id, &tag)).await
}

/// Creates an album folder; returns its final name.
#[tauri::command]
pub async fn create_album(app: AppHandle, name: String) -> CmdResult<String> {
    mutate(app, move |lib| lib.create_album(&name)).await
}

/// Renames an album folder; returns its final name.
#[tauri::command]
pub async fn rename_album(app: AppHandle, old: String, new: String) -> CmdResult<String> {
    mutate(app, move |lib| lib.rename_album(&old, &new)).await
}

/// Dissolves an album, sending its prints back to their month folders.
#[tauri::command]
pub async fn delete_album(app: AppHandle, name: String) -> CmdResult<()> {
    mutate(app, move |lib| lib.delete_album(&name)).await
}

/// Moves prints into an album folder.
#[tauri::command]
pub async fn move_to_album(app: AppHandle, ids: Vec<PrintId>, album: String) -> CmdResult<()> {
    mutate(app, move |lib| lib.move_to_album(&ids, &album)).await
}

/// Takes prints out of their album.
#[tauri::command]
pub async fn remove_from_album(app: AppHandle, ids: Vec<PrintId>) -> CmdResult<()> {
    mutate(app, move |lib| lib.remove_from_album(&ids)).await
}

/// Sends prints to the Recycle Bin.
#[tauri::command]
pub async fn delete_prints(app: AppHandle, ids: Vec<PrintId>) -> CmdResult<()> {
    mutate(app, move |lib| lib.delete(&ids)).await
}

/// Copies a print's image to the clipboard.
#[tauri::command]
pub async fn copy_print(app: AppHandle, id: PrintId) -> CmdResult<()> {
    blocking(app, move |_, state| {
        let path = with_lib(state, |lib| lib.get(id))?.path;
        let img = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
        copy_to_clipboard(&img).map_err(|e| e.to_string())
    })
    .await
}

/// Shows a print selected in a new Explorer window.
#[tauri::command]
pub async fn reveal_print(app: AppHandle, id: PrintId) -> CmdResult<()> {
    blocking(app, move |_, state| {
        let path = with_lib(state, |lib| lib.get(id))?.path;
        // Explorer wants `/select,"<path>"` verbatim, which std's quoting would mangle.
        // Its exit code is meaningless (1 even on success), so only spawning is checked.
        Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string())
    })
    .await
}

/// Opens (or focuses) the settings window. Async because creating a window from a
/// synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    windows::open_settings(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    #[test]
    fn query_deserializes_from_ui_shape() {
        let q: Query = serde_json::from_str(
            r#"{"filter":{"kind":"album","value":"Bugs"},"search":null,"order":"newestFirst"}"#,
        )
        .unwrap();
        assert_eq!(q.filter, Filter::Album("Bugs".into()));
        assert_eq!(q.order, SortOrder::NewestFirst);
        assert_eq!(q.search, None);
    }

    #[test]
    fn unit_filters_deserialize_without_value() {
        let q: Query = serde_json::from_str(
            r#"{"filter":{"kind":"favorites"},"search":"bug","order":"oldestFirst"}"#,
        )
        .unwrap();
        assert_eq!(q.filter, Filter::Favorites);
        assert_eq!(q.search.as_deref(), Some("bug"));
        assert_eq!(q.order, SortOrder::OldestFirst);
    }

    #[test]
    fn print_dto_flattens_print_with_thumb() {
        let print = Print {
            id: PrintId(7),
            path: PathBuf::from(r"C:\p\a.png"),
            file_name: "a.png".into(),
            captured_at: NaiveDate::from_ymd_opt(2026, 11, 4)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap(),
            favorite: true,
            album: None,
            tags: vec!["x".into()],
        };
        let dto = PrintDto {
            print,
            thumb: r"C:\t\1.jpg".into(),
        };
        let v = serde_json::to_value(&dto).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["capturedAt"], "2026-11-04T09:00:00");
        assert_eq!(v["fileName"], "a.png");
        assert_eq!(v["thumb"], r"C:\t\1.jpg");
    }
}
