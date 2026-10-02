/// Tauri commands for database operations: open, create, save, lock.
use crate::kdbx::{self, crypto::Cipher, keys::KdfParams, xml::KeePassFile};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

/// In-memory database state. Stored behind a Mutex for thread safety.
pub struct DbState {
    pub is_open: bool,
    pub file_path: Option<PathBuf>,
    pub keepass_file: Option<KeePassFile>,
    pub password_hash: Option<Vec<u8>>, // Cached for save operations
    pub keyfile_data: Option<Vec<u8>>,
    pub cipher: Cipher,
    pub kdf: KdfParams,
    pub salt: Vec<u8>,
}

impl Default for DbState {
    fn default() -> Self {
        Self {
            is_open: false,
            file_path: None,
            keepass_file: None,
            password_hash: None,
            keyfile_data: None,
            cipher: Cipher::Aes256,
            kdf: KdfParams::default(),
            salt: Vec::new(),
        }
    }
}

impl DbState {
    /// Persist the in-memory database to its file. Shared by the `save_database`
    /// command and the browser bridge (associate/set-login) so writes from the
    /// extension aren't lost on close/lock. Caller already holds the lock.
    pub fn save(&self) -> Result<(), String> {
        if !self.is_open || self.keepass_file.is_none() || self.file_path.is_none() {
            return Err("No open database".to_string());
        }
        let password = String::from_utf8(
            self.password_hash.as_ref().ok_or("No password cached")?.clone(),
        )
        .map_err(|_| "Password encoding error".to_string())?;

        kdbx::writer::write_database(
            self.file_path.as_ref().unwrap(),
            self.keepass_file.as_ref().unwrap(),
            &password,
            self.keyfile_data.as_deref(),
            self.cipher,
            &self.kdf,
        )
    }
}

#[derive(serde::Serialize)]
pub struct DatabaseInfo {
    pub file_path: String,
    pub name: String,
    pub description: String,
    pub encryption: String,
    pub kdf: String,
    pub groups: usize,
    pub entries: usize,
    pub created: String,
    pub modified: String,
}

#[tauri::command]
pub async fn open_database(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    password: String,
    keyfile_path: Option<String>,
) -> Result<DatabaseInfo, String> {
    let path = PathBuf::from(&path);

    // Read keyfile if provided
    let keyfile_data = if let Some(kf_path) = &keyfile_path {
        Some(std::fs::read(kf_path).map_err(|e| format!("Failed to read keyfile: {e}"))?)
    } else {
        None
    };

    // Read and decrypt the database
    let result = kdbx::reader::read_database(
        &path,
        &password,
        keyfile_data.as_deref(),
    )?;

    let kf = &result.keepass_file;

    // Count entries recursively
    let entry_count = count_entries(&kf.root.group);
    let group_count = count_groups(&kf.root.group);

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;

    let info = DatabaseInfo {
        file_path: path.to_string_lossy().to_string(),
        name: kf.meta.database_name.clone(),
        description: kf.meta.database_description.clone(),
        encryption: format!("{:?}", result.cipher),
        kdf: "Argon2id".to_string(),
        groups: group_count,
        entries: entry_count,
        created: kf.meta.database_name_changed.clone(),
        modified: kf.meta.settings_changed.clone(),
    };

    db.is_open = true;
    db.file_path = Some(path);
    db.keepass_file = Some(kf.clone());
    db.password_hash = Some(password.as_bytes().to_vec()); // Simplified — use hash in production
    db.keyfile_data = keyfile_data;
    db.cipher = result.cipher;
    db.kdf = result.kdf;
    db.salt = result.salt;

    Ok(info)
}

#[tauri::command]
pub async fn create_database(
    state: State<'_, Arc<Mutex<DbState>>>,
    path: String,
    password: String,
    name: String,
    encryption: Option<String>,
    keyfile_path: Option<String>,
) -> Result<DatabaseInfo, String> {
    let path = PathBuf::from(&path);

    let cipher = match encryption.as_deref() {
        Some("chacha20") => Cipher::ChaCha20,
        _ => Cipher::Aes256,
    };

    let kdf = KdfParams::default();
    let keyfile_data = if let Some(kf_path) = &keyfile_path {
        Some(std::fs::read(kf_path).map_err(|e| format!("Failed to read keyfile: {e}"))?)
    } else {
        None
    };

    let keepass_file = KeePassFile::new(&name);

    // Write to disk
    kdbx::writer::write_database(
        &path,
        &keepass_file,
        &password,
        keyfile_data.as_deref(),
        cipher,
        &kdf,
    )?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;

    db.is_open = true;
    db.file_path = Some(path.clone());
    db.keepass_file = Some(keepass_file);
    db.password_hash = Some(password.as_bytes().to_vec());
    db.keyfile_data = keyfile_data;
    db.cipher = cipher;
    db.kdf = kdf;
    db.salt = kdbx::keys::generate_salt(32);

    Ok(DatabaseInfo {
        file_path: path.to_string_lossy().to_string(),
        name,
        description: String::new(),
        encryption: format!("{:?}", cipher),
        kdf: "Argon2id".to_string(),
        groups: 1,
        entries: 0,
        created: String::new(),
        modified: String::new(),
    })
}

#[tauri::command]
pub async fn save_database(state: State<'_, Arc<Mutex<DbState>>>) -> Result<(), String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    db.save()
}

#[tauri::command]
pub async fn lock_database(state: State<'_, Arc<Mutex<DbState>>>) -> Result<(), String> {
    crate::ssh::agent::clear_session_approvals();
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;

    // Zero out sensitive data
    db.password_hash = None;
    db.keepass_file = None;
    db.is_open = false;

    Ok(())
}

#[tauri::command]
pub async fn get_database_info(state: State<'_, Arc<Mutex<DbState>>>) -> Result<Option<DatabaseInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;

    if !db.is_open || db.keepass_file.is_none() {
        return Ok(None);
    }

    let kf = db.keepass_file.as_ref().unwrap();

    Ok(Some(DatabaseInfo {
        file_path: db.file_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        name: kf.meta.database_name.clone(),
        description: kf.meta.database_description.clone(),
        encryption: format!("{:?}", db.cipher),
        kdf: "Argon2id".to_string(),
        groups: count_groups(&kf.root.group),
        entries: count_entries(&kf.root.group),
        created: kf.meta.database_name_changed.clone(),
        modified: kf.meta.settings_changed.clone(),
    }))
}

// =============================================================================
// Helpers
// =============================================================================

fn count_entries(group: &kdbx::xml::Group) -> usize {
    group.entries.len() + group.groups.iter().map(|g| count_entries(g)).sum::<usize>()
}

fn count_groups(group: &kdbx::xml::Group) -> usize {
    1 + group.groups.iter().map(|g| count_groups(g)).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdbx::xml::{CustomDataItem, KeePassFile};

    /// Proves the browser-bridge fix: mutating the in-memory vault and calling
    /// save() actually persists to disk (associations/credentials survive
    /// restart), by writing, mutating+saving, then reopening from disk.
    #[test]
    fn test_save_persists_custom_data() {
        let path = std::env::temp_dir().join(format!("mypass-save-test-{}.kdbx", uuid::Uuid::new_v4()));
        let password = "correct horse";
        let cipher = Cipher::Aes256;
        let kdf = KdfParams::default();

        // Write an initial vault, then load it into an "open" DbState.
        let initial = KeePassFile::new("Test Vault");
        kdbx::writer::write_database(&path, &initial, password, None, cipher, &kdf).unwrap();
        let loaded = kdbx::reader::read_database(&path, password, None).unwrap();

        let mut db = DbState {
            is_open: true,
            file_path: Some(path.clone()),
            keepass_file: Some(loaded.keepass_file),
            password_hash: Some(password.as_bytes().to_vec()),
            keyfile_data: None,
            cipher: loaded.cipher,
            kdf: loaded.kdf,
            salt: loaded.salt,
        };

        // Mutate like the browser bridge does, then persist.
        db.keepass_file.as_mut().unwrap().meta.custom_data.items.push(CustomDataItem {
            key: "Browser_MyPass-test".to_string(),
            value: "an-id-key".to_string(),
        });
        db.save().unwrap();

        // Reopen from disk — the association must be there.
        let reopened = kdbx::reader::read_database(&path, password, None).unwrap();
        let found = reopened
            .keepass_file
            .meta
            .custom_data
            .items
            .iter()
            .any(|i| i.key == "Browser_MyPass-test" && i.value == "an-id-key");

        let _ = std::fs::remove_file(&path);
        assert!(found, "custom data written before save() was lost on reload");
    }
}
