//! Tauri wrapper: the logic lives in mypass_core (totp + ops::entries).
use crate::commands::database::DbState;
use crate::kdbx::ops::entries;
use crate::kdbx::totp::TotpCode;
use std::sync::{Arc, Mutex};
use tauri::State;

/// Current 2FA code of an entry: `{code, period, secondsRemaining}`.
#[tauri::command]
pub async fn get_totp_code(
    state: State<'_, Arc<Mutex<DbState>>>,
    uuid: String,
) -> Result<TotpCode, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;
    entries::get_totp_code(kf, &uuid, crate::kdbx::time::unix_now())
}
