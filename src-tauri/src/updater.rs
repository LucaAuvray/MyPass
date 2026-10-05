//! Startup update check: asks `latest.json` on the sync server whether a newer
//! signed `.msi` exists and, if so, offers to install it through a native dialog.
//!
//! Release builds only: a `tauri dev` build must never offer the published
//! `.msi`. Nothing is exposed to the webview. On Windows, installing exits the
//! app (msiexec replaces the exe) and the MSI relaunches it afterwards.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

pub fn check_on_startup(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let update = async { app.updater()?.check().await }.await;
        match update {
            Ok(Some(update)) => offer(app, update),
            Ok(None) => tracing::info!("MyPass is up to date"),
            // Offline, server unreachable, bad manifest: stay quiet.
            Err(e) => tracing::warn!("update check failed: {e}"),
        }
    });
}

fn offer(app: AppHandle, update: Update) {
    let text = format!(
        "MyPass {} est disponible (installée : {}). L'app va se fermer pour l'installer.",
        update.version, update.current_version
    );
    let mut dialog = app
        .dialog()
        .message(text)
        .title("MyPass")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Installer".into(),
            "Plus tard".into(),
        ));
    // Modal on the window, so the vault can't be unlocked behind the question.
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.parent(&window);
    }
    dialog.show(move |install| {
        if !install {
            return;
        }
        tauri::async_runtime::spawn(async move {
            // On Windows a successful install never returns: the plugin exits
            // the process once msiexec has started.
            if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                app.dialog()
                    .message(format!("La mise à jour a échoué : {e}"))
                    .title("MyPass")
                    .kind(MessageDialogKind::Error)
                    .show(|_| {});
            }
        });
    });
}
