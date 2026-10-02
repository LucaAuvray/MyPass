mod commands;
pub use mypass_core as kdbx;
pub mod security;
pub mod native_messaging;
pub mod ssh;

use commands::database::DbState;
use std::sync::{Arc, Mutex};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch just focuses the existing window instead of
            // starting a rival process that can't own the bridge.
            use tauri::Manager;
            if let Some(w) = app.webview_windows().values().next() {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Mutex::new(DbState::default())))
        .manage(commands::sync::SyncRuntime::default())
        .setup(|app| {
            let db_state = app.state::<Arc<Mutex<DbState>>>().inner().clone();
            native_messaging::set_app_handle(app.handle().clone());
            native_messaging::start_local_bridge(db_state.clone());
            commands::browser::ensure_native_messaging_manifest();
            crate::ssh::agent::start_if_enabled(app.handle().clone(), db_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Database
            commands::database::open_database,
            commands::database::create_database,
            commands::database::save_database,
            commands::database::lock_database,
            commands::database::get_database_info,
            commands::database::get_vault_location,
            // Entries
            commands::entries::get_entries,
            commands::entries::get_entry,
            commands::entries::create_entry,
            commands::entries::update_entry,
            commands::entries::delete_entry,
            commands::entries::search_entries,
            commands::entries::duplicate_entry,
            // Groups
            commands::groups::get_groups,
            commands::groups::create_group,
            commands::groups::update_group,
            commands::groups::delete_group,
            commands::groups::move_entry,
            // Generator
            commands::generator::generate_password,
            commands::generator::generate_passphrase,
            commands::generator::evaluate_strength,
            // TOTP
            commands::totp::generate_totp_code,
            commands::totp::generate_totp_secret,
            // Import/Export
            commands::import_export::parse_import,
            commands::import_export::import_entries,
            commands::import_export::export_entries,
            // Browser
            commands::browser::get_browser_status,
            commands::browser::is_browser_integration_enabled,
            commands::browser::toggle_browser_integration,
            // SSH
            commands::ssh::parse_ssh_key,
            commands::ssh::generate_ssh_key,
            commands::ssh::is_ssh_agent_enabled,
            commands::ssh::toggle_ssh_agent,
            commands::ssh::respond_ssh_sign,
            commands::ssh::get_ssh_agent_status,
            commands::ssh::disable_windows_ssh_agent_service,
            // Sync
            commands::sync::get_sync_config,
            commands::sync::set_sync_config,
            commands::sync::get_sync_status,
            commands::sync::sync_now,
            commands::sync::fetch_vault_from_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MyPass");
}

/// Entry point for the browser-launched `--native-messaging` process. Relays
/// stdio to the already-running main app's local bridge, since this process
/// has no access to the real unlocked database on its own (see
/// `native_messaging::run_native_messaging_proxy`).
pub fn run_native_messaging_proxy() {
    if let Err(e) = native_messaging::run_native_messaging_proxy() {
        eprintln!("Native Messaging proxy error: {}", e);
    }
}
