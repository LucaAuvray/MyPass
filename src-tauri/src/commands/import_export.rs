/// Tauri commands for importing and exporting passwords. The formats live in
/// `kdbx::ops::transfer`, shared with the PWA's wasm.
use crate::commands::database::DbState;
use crate::kdbx::ops::transfer::{self, ExportFormat, ImportResult, ImportedEntry, ResolvedImport};
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

/// Read an import file's content (MyPass JSON or CSV).
#[tauri::command]
pub async fn parse_import(content: String) -> Result<Vec<ImportedEntry>, String> {
    transfer::parse_import(&content)
}

/// Write deduplicated entries into the open vault, then save it.
#[tauri::command]
pub async fn import_entries(
    state: State<'_, Arc<Mutex<DbState>>>,
    entries: Vec<ResolvedImport>,
) -> Result<ImportResult, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    let result = transfer::apply_import(kf, entries)?;
    db.save()?;
    Ok(result)
}

#[derive(serde::Serialize)]
pub struct ExportOutcome {
    saved: bool,
}

/// Export to a file the user picks in a native "Save as" dialog. The path never
/// comes from the front end, and the vault lock is released before the dialog.
#[tauri::command]
pub async fn export_entries(
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<DbState>>>,
    format: String,
    uuids: Vec<String>,
    file_name: String,
) -> Result<ExportOutcome, String> {
    let format = ExportFormat::parse(&format)?;
    let content = {
        let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
        let kf = db.keepass_file.as_ref().ok_or("No open database")?;
        transfer::export(kf, format, &uuids)?
    };
    let (filter, ext) = match format {
        ExportFormat::Json => ("JSON", "json"),
        ExportFormat::Csv => ("CSV", "csv"),
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter(filter, &[ext]).set_file_name(&file_name).blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(path) = picked else {
        return Ok(ExportOutcome { saved: false });
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(ExportOutcome { saved: true })
}
