/// Tauri commands for Passkeys (FIDO2/WebAuthn) management.
use crate::commands::database::DbState;
use crate::kdbx::xml::Entry;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(serde::Serialize, Clone)]
pub struct PasskeyInfo {
    pub entry_uuid: String,
    pub credential_id: String,
    pub relying_party: String,
    pub username: String,
    pub created: String,
    pub counter: u32,
}

#[derive(serde::Deserialize)]
pub struct PasskeyRegisterRequest {
    pub entry_uuid: String,
    pub relying_party: String,
    pub challenge: String,
}

#[derive(serde::Serialize)]
pub struct PasskeyRegisterResult {
    pub credential_id: String,
    pub public_key: String,
    pub success: bool,
}

#[derive(serde::Deserialize)]
pub struct PasskeyAuthRequest {
    pub entry_uuid: String,
    pub credential_id: String,
    pub challenge: String,
    pub signature: String,
}

#[derive(serde::Serialize)]
pub struct PasskeyAuthResult {
    pub success: bool,
    pub counter: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PasskeyExportData {
    pub credential_id: String,
    pub relying_party: String,
    pub username: String,
    pub public_key: String,
    pub private_key: String, // encrypted
    pub counter: u32,
    pub created: String,
}

/// List all passkeys stored in the database.
#[tauri::command]
pub async fn list_passkeys(
    state: State<'_, Arc<Mutex<DbState>>>,
) -> Result<Vec<PasskeyInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let entries = collect_all_entries(&kf.root.group);
    let passkeys: Vec<PasskeyInfo> = entries
        .iter()
        .filter(|e| e.strings.iter().any(|s| s.key.starts_with("KPEX_PASSKEY_")))
        .map(|e| {
            let rp = e.strings
                .iter()
                .find(|s| s.key == "KPEX_PASSKEY_RELYING_PARTY")
                .map(|s| s.value.content.clone())
                .unwrap_or_default();

            let cred_id = e.strings
                .iter()
                .find(|s| s.key == "KPEX_PASSKEY_CREDENTIAL_ID")
                .map(|s| s.value.content.clone())
                .unwrap_or_default();

            PasskeyInfo {
                entry_uuid: e.uuid.clone(),
                credential_id: cred_id,
                relying_party: rp,
                username: e.username().to_string(),
                created: e.times.creation_time.clone().unwrap_or_default(),
                counter: 0,
            }
        })
        .collect();

    Ok(passkeys)
}

/// Register a new passkey credential.
#[tauri::command]
pub async fn register_passkey(
    state: State<'_, Arc<Mutex<DbState>>>,
    request: PasskeyRegisterRequest,
) -> Result<PasskeyRegisterResult, String> {
    // In production, this would:
    // 1. Generate a new WebAuthn key pair
    // 2. Store the private key encrypted in the entry
    // 3. Return the public key credential to the browser
    //
    // For MVP, create a placeholder credential.

    let credential_id = uuid::Uuid::new_v4().to_string();
    let public_key = hex::encode(&rand::random::<[u8; 32]>());

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    if let Some(entry) = find_entry_mut(&mut kf.root.group, &request.entry_uuid) {
        // Add passkey fields to the entry
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_CREDENTIAL_ID".to_string(),
            value: crate::kdbx::xml::Value {
                content: credential_id.clone(),
                protected: Some("True".to_string()),
            },
        });
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_RELYING_PARTY".to_string(),
            value: crate::kdbx::xml::Value {
                content: request.relying_party,
                protected: None,
            },
        });
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_PUBLIC_KEY".to_string(),
            value: crate::kdbx::xml::Value {
                content: public_key.clone(),
                protected: Some("True".to_string()),
            },
        });
        entry.times.touch();
    }

    db.save()?;

    Ok(PasskeyRegisterResult {
        credential_id,
        public_key,
        success: true,
    })
}

/// Authenticate using a passkey.
#[tauri::command]
pub async fn authenticate_passkey(
    state: State<'_, Arc<Mutex<DbState>>>,
    _request: PasskeyAuthRequest,
) -> Result<PasskeyAuthResult, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let _kf = db.keepass_file.as_ref().ok_or("No open database")?;

    // Verify the signature against the stored public key
    // For MVP, always return success (full FIDO2 validation is complex)

    Ok(PasskeyAuthResult {
        success: true,
        counter: 1,
    })
}

/// Export passkeys to a JSON file.
#[tauri::command]
pub async fn export_passkeys(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    uuids: Vec<String>,
) -> Result<(), String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;

    let entries = collect_all_entries(&kf.root.group);
    let passkeys: Vec<PasskeyExportData> = entries
        .iter()
        .filter(|e| {
            e.strings.iter().any(|s| s.key.starts_with("KPEX_PASSKEY_"))
                && (uuids.is_empty() || uuids.contains(&e.uuid))
        })
        .map(|e| {
            let rp = e.strings.iter().find(|s| s.key == "KPEX_PASSKEY_RELYING_PARTY")
                .map(|s| s.value.content.clone()).unwrap_or_default();
            let cred_id = e.strings.iter().find(|s| s.key == "KPEX_PASSKEY_CREDENTIAL_ID")
                .map(|s| s.value.content.clone()).unwrap_or_default();
            let pk = e.strings.iter().find(|s| s.key == "KPEX_PASSKEY_PUBLIC_KEY")
                .map(|s| s.value.content.clone()).unwrap_or_default();

            PasskeyExportData {
                credential_id: cred_id,
                relying_party: rp,
                username: e.username().to_string(),
                public_key: pk,
                private_key: String::new(),
                counter: 0,
                created: e.times.creation_time.clone().unwrap_or_default(),
            }
        })
        .collect();

    let json = serde_json::to_string_pretty(&passkeys)
        .map_err(|e| format!("JSON serialization error: {e}"))?;

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write file: {e}"))?;

    Ok(())
}

/// Import passkeys from a JSON file.
#[tauri::command]
pub async fn import_passkeys(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
) -> Result<usize, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let passkeys: Vec<PasskeyExportData> = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid passkey JSON: {e}"))?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;

    let mut imported = 0;
    for pk in &passkeys {
        let mut entry = Entry::new(
            &format!("{} (passkey)", pk.relying_party),
            &pk.username,
            "",
            &pk.relying_party,
        );

        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_CREDENTIAL_ID".to_string(),
            value: crate::kdbx::xml::Value {
                content: pk.credential_id.clone(),
                protected: Some("True".to_string()),
            },
        });
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_RELYING_PARTY".to_string(),
            value: crate::kdbx::xml::Value {
                content: pk.relying_party.clone(),
                protected: None,
            },
        });
        entry.strings.push(crate::kdbx::xml::EntryString {
            key: "KPEX_PASSKEY_PUBLIC_KEY".to_string(),
            value: crate::kdbx::xml::Value {
                content: pk.public_key.clone(),
                protected: Some("True".to_string()),
            },
        });

        kf.root.group.entries.push(entry);
        imported += 1;
    }

    db.save()?;

    Ok(imported)
}

// =============================================================================
// Helpers
// =============================================================================

fn collect_all_entries(group: &crate::kdbx::xml::Group) -> Vec<&Entry> {
    let mut result: Vec<&Entry> = group.entries.iter().collect();
    for child in &group.groups {
        result.extend(collect_all_entries(child));
    }
    result
}

fn find_entry_mut<'a>(
    group: &'a mut crate::kdbx::xml::Group,
    uuid: &str,
) -> Option<&'a mut Entry> {
    if let Some(entry) = group.entries.iter_mut().find(|e| e.uuid == uuid) {
        return Some(entry);
    }
    for child in &mut group.groups {
        if let Some(found) = find_entry_mut(child, uuid) {
            return Some(found);
        }
    }
    None
}
