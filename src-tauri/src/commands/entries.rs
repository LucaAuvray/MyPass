/// Tauri commands for entry CRUD operations.
///
/// Thin lock/save wrappers over the pure functions in
/// `mypass_core::ops::entries`, which the wasm crate also reuses.
use crate::commands::database::DbState;
use crate::kdbx::ops::entries::{self, EntryInfo, NewEntry, UpdateEntry};
use std::sync::{Arc, Mutex};
use tauri::State;

#[tauri::command]
pub async fn get_entries(
    state: State<'_, Arc<Mutex<DbState>>>,
    group_uuid: Option<String>,
) -> Result<Vec<EntryInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;
    entries::list(kf, group_uuid.as_deref())
}

#[tauri::command]
pub async fn get_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
) -> Result<EntryInfo, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;
    entries::get(kf, &uuid)
}

#[tauri::command]
pub async fn create_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    entry: NewEntry,
) -> Result<EntryInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    let info = entries::create(kf, entry)?;

    // Persistance immédiate (même pattern que le pont navigateur) — sans ça, les
    // mutations ne vivent qu'en mémoire et lock/close les perd.
    db.save()?;

    Ok(info)
}

#[tauri::command]
pub async fn update_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
    update: UpdateEntry,
) -> Result<EntryInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    let info = entries::update(kf, &uuid, update)?;
    db.save()?;

    Ok(info)
}

#[tauri::command]
pub async fn delete_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
) -> Result<(), String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    entries::delete(kf, &uuid)?;
    db.save()?;

    Ok(())
}

#[tauri::command]
pub async fn search_entries(
    state: State<'_, Arc<Mutex<DbState>>>,
    query: String,
) -> Result<Vec<EntryInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let query_lower = query.to_lowercase();
    let all = entries::list(kf, None)?;
    let filtered: Vec<_> = all
        .into_iter()
        .filter(|e| {
            e.title.to_lowercase().contains(&query_lower)
                || e.username.to_lowercase().contains(&query_lower)
                || e.url.to_lowercase().contains(&query_lower)
        })
        .collect();

    Ok(filtered)
}

#[tauri::command]
pub async fn duplicate_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
) -> Result<EntryInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    let info = entries::duplicate(kf, &uuid)?;
    db.save()?;

    Ok(info)
}
