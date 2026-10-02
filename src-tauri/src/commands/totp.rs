//! Tauri wrappers: the logic lives in mypass_core::totp.
use crate::kdbx::totp::{self, TotpCode};

#[tauri::command]
pub fn generate_totp_code(
    secret: String,
    algorithm: Option<String>,
    digits: Option<usize>,
    period: Option<u32>,
) -> Result<TotpCode, String> {
    totp::generate_totp_code(secret, algorithm, digits, period)
}

#[tauri::command]
pub fn generate_totp_secret() -> Result<String, String> {
    totp::generate_totp_secret()
}
