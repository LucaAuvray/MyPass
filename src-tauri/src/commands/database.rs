/// Tauri commands for database operations: open, create, save, lock.
use crate::kdbx::{self, crypto::Cipher, keys::KdfParams, xml::KeePassFile};
use std::path::{Path, PathBuf};
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

/// KDBX file signature (little-endian 0x9AA2D903, 0xB54BFB67).
const KDBX_SIGNATURE: [u8; 8] = [0x9A, 0xA2, 0xD9, 0x03, 0xB5, 0x4B, 0xFB, 0x67];

/// The one desktop vault location: one vault per PC, next to sync.json.
pub fn default_vault_path() -> Result<PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA introuvable".to_string())?;
    Ok(PathBuf::from(appdata).join("MyPass").join("mypass-vault.kdbx"))
}

/// Decrypt vault bytes, mapping failures to the stable codes the unlock
/// screen translates (`NOT_A_VAULT`, `WRONG_PASSWORD`).
pub fn read_vault_bytes(
    bytes: &[u8],
    password: &str,
    keyfile: Option<&[u8]>,
) -> Result<kdbx::reader::DatabaseReadResult, String> {
    if !bytes.starts_with(&KDBX_SIGNATURE) {
        return Err("NOT_A_VAULT".to_string());
    }
    // ponytail: a corrupted KDBX is indistinguishable from a wrong password here
    // (both fail AEAD decryption) — split them if mypass-core ever exposes typed errors.
    kdbx::reader::read_database_bytes(bytes, password, keyfile).map_err(|_| "WRONG_PASSWORD".to_string())
}

/// Install a decrypted vault as the open database. Shared by every way of
/// opening a vault (local file, fetch from server).
pub fn load_into_state(
    db: &mut DbState,
    path: PathBuf,
    result: kdbx::reader::DatabaseReadResult,
    password: &str,
    keyfile: Option<Vec<u8>>,
) -> DatabaseInfo {
    let kf = &result.keepass_file;
    let info = DatabaseInfo {
        file_path: path.to_string_lossy().to_string(),
        name: kf.meta.database_name.clone(),
        description: kf.meta.database_description.clone(),
        encryption: format!("{:?}", result.cipher),
        kdf: "Argon2id".to_string(),
        groups: count_groups(&kf.root.group),
        entries: count_entries(&kf.root.group),
        created: kf.meta.database_name_changed.clone(),
        modified: kf.meta.settings_changed.clone(),
    };

    db.is_open = true;
    db.file_path = Some(path);
    db.keepass_file = Some(result.keepass_file);
    db.password_hash = Some(password.as_bytes().to_vec()); // Simplified — use hash in production
    db.keyfile_data = keyfile;
    db.cipher = result.cipher;
    db.kdf = result.kdf;
    db.salt = result.salt;

    info
}

/// Write a brand-new vault, refusing to replace an existing file.
fn create_vault_file(
    path: &Path,
    keepass_file: &KeePassFile,
    password: &str,
    keyfile: Option<&[u8]>,
    cipher: Cipher,
    kdf: &KdfParams,
) -> Result<(), String> {
    if path.exists() {
        return Err("VAULT_EXISTS".to_string());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create vault folder: {e}"))?;
    }
    kdbx::writer::write_database(path, keepass_file, password, keyfile, cipher, kdf)
}

/// Save a vault downloaded from the sync server as this PC's vault. The bytes
/// must decrypt with `password` before anything touches the disk, and are
/// written verbatim; `create_new` makes the no-overwrite check atomic.
pub fn install_remote_vault(
    bytes: &[u8],
    password: &str,
    path: &Path,
) -> Result<kdbx::reader::DatabaseReadResult, String> {
    let result = read_vault_bytes(bytes, password, None)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create vault folder: {e}"))?;
    }
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| {
        match e.kind() {
            std::io::ErrorKind::AlreadyExists => "VAULT_EXISTS".to_string(),
            _ => format!("Failed to write vault: {e}"),
        }
    })?;
    std::io::Write::write_all(&mut file, bytes).map_err(|e| format!("Failed to write vault: {e}"))?;
    Ok(result)
}

fn read_keyfile(keyfile_path: &Option<String>) -> Result<Option<Vec<u8>>, String> {
    keyfile_path
        .as_ref()
        .map(|p| std::fs::read(p).map_err(|e| format!("Failed to read keyfile: {e}")))
        .transpose()
}

#[derive(serde::Serialize)]
pub struct VaultLocation {
    pub path: String,
    pub exists: bool,
}

#[tauri::command]
pub async fn get_vault_location() -> Result<VaultLocation, String> {
    let path = default_vault_path()?;
    Ok(VaultLocation { exists: path.exists(), path: path.to_string_lossy().to_string() })
}

#[tauri::command]
pub async fn open_database(
    state: State<'_, Arc<Mutex<DbState>>>,
    password: String,
    keyfile_path: Option<String>,
) -> Result<DatabaseInfo, String> {
    let path = default_vault_path()?;
    let keyfile_data = read_keyfile(&keyfile_path)?;

    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "NO_VAULT".to_string(),
        _ => format!("Failed to read vault: {e}"),
    })?;
    let result = read_vault_bytes(&bytes, &password, keyfile_data.as_deref())?;

    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    Ok(load_into_state(&mut db, path, result, &password, keyfile_data))
}

#[tauri::command]
pub async fn create_database(
    state: State<'_, Arc<Mutex<DbState>>>,
    password: String,
    name: String,
    encryption: Option<String>,
    keyfile_path: Option<String>,
) -> Result<DatabaseInfo, String> {
    let path = default_vault_path()?;

    let cipher = match encryption.as_deref() {
        Some("chacha20") => Cipher::ChaCha20,
        _ => Cipher::Aes256,
    };

    let kdf = KdfParams::default();
    let keyfile_data = read_keyfile(&keyfile_path)?;

    let keepass_file = KeePassFile::new(&name);
    create_vault_file(&path, &keepass_file, &password, keyfile_data.as_deref(), cipher, &kdf)?;

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

    fn sample_vault(password: &str) -> Vec<u8> {
        kdbx::writer::write_database_bytes(&KeePassFile::new("T"), password, None, Cipher::Aes256, &KdfParams::default())
            .unwrap()
    }

    fn temp_dir_unique(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mypass-{tag}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn read_vault_bytes_rejects_non_kdbx() {
        assert_eq!(read_vault_bytes(b"<!doctype html><html></html>", "pw", None).unwrap_err(), "NOT_A_VAULT");
        assert_eq!(read_vault_bytes(b"", "pw", None).unwrap_err(), "NOT_A_VAULT");
    }

    #[test]
    fn read_vault_bytes_maps_wrong_password() {
        let bytes = sample_vault("right");
        assert_eq!(read_vault_bytes(&bytes, "wrong", None).unwrap_err(), "WRONG_PASSWORD");
        assert_eq!(read_vault_bytes(&bytes, "right", None).unwrap().keepass_file.meta.database_name, "T");
    }

    #[test]
    fn create_vault_file_refuses_existing() {
        let dir = temp_dir_unique("create-existing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mypass-vault.kdbx");
        std::fs::write(&path, b"keep me").unwrap();

        let result = create_vault_file(&path, &KeePassFile::new("T"), "pw", None, Cipher::Aes256, &KdfParams::default());

        assert_eq!(result, Err("VAULT_EXISTS".to_string()));
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_vault_file_creates_missing_parent() {
        let dir = temp_dir_unique("create-parent");
        let path = dir.join("MyPass").join("mypass-vault.kdbx");

        create_vault_file(&path, &KeePassFile::new("T"), "pw", None, Cipher::Aes256, &KdfParams::default()).unwrap();

        assert!(kdbx::reader::read_database(&path, "pw", None).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_remote_vault_wrong_password_writes_nothing() {
        let dir = temp_dir_unique("install-wrong-pw");
        let path = dir.join("mypass-vault.kdbx");

        let err = install_remote_vault(&sample_vault("right"), "wrong", &path).unwrap_err();

        assert_eq!(err, "WRONG_PASSWORD");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_remote_vault_rejects_html() {
        let dir = temp_dir_unique("install-html");
        let path = dir.join("mypass-vault.kdbx");

        let err = install_remote_vault(b"<!doctype html>", "pw", &path).unwrap_err();

        assert_eq!(err, "NOT_A_VAULT");
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_remote_vault_refuses_existing_file() {
        let dir = temp_dir_unique("install-existing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mypass-vault.kdbx");
        std::fs::write(&path, b"keep me").unwrap();

        let err = install_remote_vault(&sample_vault("pw"), "pw", &path).unwrap_err();

        assert_eq!(err, "VAULT_EXISTS");
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_remote_vault_writes_bytes_verbatim_and_creates_parent() {
        let dir = temp_dir_unique("install-ok");
        let path = dir.join("MyPass").join("mypass-vault.kdbx");
        let bytes = sample_vault("pw");

        install_remote_vault(&bytes, "pw", &path).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(kdbx::reader::read_database(&path, "pw", None).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
