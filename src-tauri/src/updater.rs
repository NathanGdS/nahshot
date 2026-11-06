//! Self-update from the GitHub releases of NathanGdS/nahshot.
//!
//! The updater plugin reads `latest.json` from the latest release (endpoint and
//! public key in `tauri.conf.json`), checks the installer's minisign signature, runs
//! it in passive mode and exits; the installer reopens nahshot when it finishes.

use crate::commands::CmdResult;
use log::{error, info, warn};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Set while an update is downloading, so a second request is refused.
static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Event with the download progress, 0–100 (`null` when the size is unknown).
pub const PROGRESS_EVENT: &str = "update-progress";

/// What `check_update` found.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// Running version.
    pub current: String,
    /// Newer version on GitHub, `None` when this one is the latest.
    pub latest: Option<String>,
    /// A debug build: it can check, but never installs.
    pub dev_build: bool,
}

const DEV_BUILD: bool = cfg!(debug_assertions);

async fn find(app: &AppHandle) -> CmdResult<Option<Update>> {
    app.updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Não foi possível procurar atualizações: {e}"))
}

/// Checks in the background at startup and, if there is a newer version, asks
/// whether to install it now. Debug builds skip this.
pub fn check_on_startup(app: &AppHandle) {
    if DEV_BUILD {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match find(&app).await {
            Ok(Some(update)) => offer(&app, update),
            Ok(None) => info!("nahshot is up to date"),
            Err(e) => warn!("update check: {e}"),
        }
    });
}

fn offer(app: &AppHandle, update: Update) {
    info!("update available: {}", update.version);
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "A versão {} do nahshot está disponível (você tem a {}).\n\n\
             O nahshot fecha, atualiza e abre de novo sozinho.",
            update.version, update.current_version
        ))
        .title("Atualização do nahshot")
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Atualizar agora".into(),
            "Depois".into(),
        ))
        .show(move |accepted| {
            if accepted {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = install(&handle, update).await {
                        error!("update: {e}");
                        handle
                            .dialog()
                            .message(e)
                            .title("Atualização do nahshot")
                            .kind(MessageDialogKind::Error)
                            .show(|_| {});
                    }
                });
            }
        });
}

/// Downloads, verifies and runs the installer. On success the process exits.
async fn install(app: &AppHandle, update: Update) -> CmdResult<()> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("A atualização já está em andamento.".into());
    }
    info!("installing {}", update.version);
    let mut downloaded = 0u64;
    let mut last = None;
    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let pct = total
                    .filter(|t| *t > 0)
                    .map(|t| (downloaded * 100 / t).min(100));
                if pct != last {
                    last = pct;
                    let _ = app.emit(PROGRESS_EVENT, pct);
                }
            },
            || info!("update downloaded, starting the installer"),
        )
        .await;
    // Only reached if something failed: a successful install exits the process.
    INSTALLING.store(false, Ordering::SeqCst);
    result.map_err(|e| format!("Não foi possível atualizar: {e}"))
}

/// Compares this version with the latest release.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> CmdResult<UpdateInfo> {
    let latest = find(&app).await?.map(|u| u.version);
    Ok(UpdateInfo {
        current: app.package_info().version.to_string(),
        latest,
        dev_build: DEV_BUILD,
    })
}

/// Installs the latest release; nahshot closes and the installer reopens it.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    if DEV_BUILD {
        return Err("Build de desenvolvimento: não se atualiza.".into());
    }
    match find(&app).await? {
        Some(update) => install(&app, update).await,
        None => Err("Você já está na versão mais recente.".into()),
    }
}
