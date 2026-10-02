//! Tauri wrappers: the logic lives in mypass_core::generator.
use crate::kdbx::generator::{self, PassphraseConfig, PasswordConfig, StrengthResult};

#[tauri::command]
pub fn generate_password(config: PasswordConfig) -> Result<String, String> {
    generator::generate_password(config)
}

#[tauri::command]
pub fn generate_passphrase(config: PassphraseConfig) -> Result<String, String> {
    generator::generate_passphrase(config)
}

#[tauri::command]
pub fn evaluate_strength(password: String) -> Result<StrengthResult, String> {
    generator::evaluate_strength(password)
}
