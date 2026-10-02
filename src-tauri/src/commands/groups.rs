/// Tauri commands for group CRUD operations.
///
/// Thin lock/save wrappers over the pure functions in
/// `mypass_core::ops::groups`, which the wasm crate also reuses.
use crate::commands::database::DbState;
use crate::kdbx::ops::groups::{self, GroupInfo};
use std::sync::{Arc, Mutex};
use tauri::State;

#[tauri::command]
pub async fn get_groups(
    state: State<'_, Arc<Mutex<DbState>>>,
) -> Result<Vec<GroupInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    Ok(groups::list(kf))
}

#[tauri::command]
pub async fn create_group(
    state: State<'_, Arc<Mutex<DbState>>>,
    name: String,
    parent_uuid: Option<String>,
) -> Result<GroupInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let info = groups::create(kf, &name, parent_uuid.as_deref())?;
    db.save()?;

    Ok(info)
}

#[tauri::command]
pub async fn update_group(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
    name: Option<String>,
    icon_id: Option<String>,
    is_expanded: Option<bool>,
) -> Result<GroupInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let info = groups::update(kf, &uuid, name, icon_id, is_expanded)?;
    db.save()?;

    Ok(info)
}

#[tauri::command]
pub async fn delete_group(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
) -> Result<(), String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    groups::delete(kf, &uuid)?;
    db.save()?;

    Ok(())
}

#[tauri::command]
pub async fn move_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    entry_uuid: String,
    group_uuid: String,
) -> Result<(), String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    groups::move_entry(kf, &entry_uuid, &group_uuid)?;
    db.save()?;

    Ok(())
}
