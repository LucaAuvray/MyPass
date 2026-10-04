/// Synchronisation avec mypass-server : GET → merge → PUT, boucle 409.
/// Contrat serveur (voir plan serveur) : ETag ré-envoyé VERBATIM dans
/// If-Match ; 5xx au PUT = état inconnu → re-GET avant retry ; serveur
/// vide (404) → premier PUT sans If-Match.
use crate::commands::database::{
    default_vault_path, install_remote_vault, load_into_state, DatabaseInfo, DbState,
};
use crate::kdbx::{self, merge};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SyncConfig {
    pub server_url: String,
    // ponytail: token en clair dans %APPDATA% (comme l'association
    // keepassxc-browser) — passer par DPAPI/keychain si ça devient un vrai risque.
    pub token: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_etag: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub state: String, // not_configured | idle | syncing | synced | offline | error
    pub detail: Option<String>,
    pub last_sync: Option<String>,
    pub server_version: Option<String>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self { state: "idle".into(), detail: None, last_sync: None, server_version: None }
    }
}

pub type SyncRuntime = Arc<Mutex<SyncStatus>>;

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("MyPass").join("sync.json"))
}

fn load_config_at(path: &Path) -> SyncConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn store_config_at(path: &Path, cfg: &SyncConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn load_config() -> SyncConfig {
    config_path().as_deref().map(load_config_at).unwrap_or_default()
}

fn store_config(cfg: &SyncConfig) -> Result<(), String> {
    store_config_at(&config_path().ok_or("APPDATA introuvable")?, cfg)
}

fn set_status(runtime: &SyncRuntime, state: &str, detail: Option<String>) {
    let mut s = runtime.lock().unwrap();
    s.state = state.to_string();
    s.detail = detail;
}

#[tauri::command]
pub async fn get_sync_config() -> Result<serde_json::Value, String> {
    let cfg = load_config();
    Ok(serde_json::json!({
        "serverUrl": cfg.server_url,
        "enabled": cfg.enabled,
        "hasToken": !cfg.token.is_empty(),
    }))
}

#[tauri::command]
pub async fn set_sync_config(
    server_url: String,
    token: Option<String>,
    enabled: bool,
) -> Result<(), String> {
    let mut cfg = load_config();
    // URL changée → l'etag mémorisé ne veut plus rien dire
    let normalized = normalize_server_url(&server_url);
    if cfg.server_url != normalized {
        cfg.last_etag = None;
    }
    cfg.server_url = normalized;
    if let Some(t) = token {
        if !t.trim().is_empty() {
            cfg.token = t.trim().to_string();
        }
    }
    cfg.enabled = enabled;
    store_config(&cfg)
}

fn normalize_server_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Config written after a successful fetch: this PC now syncs with that server.
fn fetched_sync_config(server_url: &str, token: &str, etag: Option<String>) -> SyncConfig {
    SyncConfig {
        server_url: normalize_server_url(server_url),
        token: token.trim().to_string(),
        enabled: true,
        last_etag: etag,
    }
}

/// Fresh PC: download the vault from the sync server, keep it as this PC's
/// vault (decrypted before writing, never overwriting), enable sync, open it.
#[tauri::command]
pub async fn fetch_vault_from_server(
    db: State<'_, Arc<Mutex<DbState>>>,
    server_url: String,
    token: String,
    password: String,
) -> Result<DatabaseInfo, String> {
    let path = default_vault_path()?;
    if path.exists() {
        return Err("VAULT_EXISTS".into());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(format!("{}/api/vault", normalize_server_url(&server_url)))
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|_| "SERVER_UNREACHABLE".to_string())?;
    match resp.status().as_u16() {
        200 => {}
        401 => return Err("TOKEN_REFUSED".into()),
        404 => return Err("NO_REMOTE_VAULT".into()),
        _ => return Err("SERVER_ERROR".into()),
    }
    let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(String::from);
    let bytes = resp.bytes().await.map_err(|_| "SERVER_UNREACHABLE".to_string())?;

    let result = install_remote_vault(&bytes, &password, &path)?;
    store_config(&fetched_sync_config(&server_url, &token, etag))?;

    let mut db = db.lock().map_err(|e| format!("Lock error: {e}"))?;
    Ok(load_into_state(&mut db, path, result, &password, None))
}

#[tauri::command]
pub async fn get_sync_status(runtime: State<'_, SyncRuntime>) -> Result<SyncStatus, String> {
    Ok(runtime.lock().unwrap().clone())
}

#[tauri::command]
pub async fn sync_now(
    db: State<'_, Arc<Mutex<DbState>>>,
    runtime: State<'_, SyncRuntime>,
) -> Result<SyncStatus, String> {
    let result = run_sync(db.inner().clone(), runtime.inner().clone()).await;
    let status = runtime.lock().unwrap().clone();
    result.map(|_| status)
}

async fn run_sync(db: Arc<Mutex<DbState>>, runtime: SyncRuntime) -> Result<(), String> {
    let result = run_sync_inner(db, runtime.clone()).await;
    if let Err(e) = &result {
        let mut s = runtime.lock().unwrap();
        // Ne pas écraser un statut déjà posé (offline, not_configured, error) —
        // seul un état resté bloqué sur "syncing" est corrigé ici.
        if s.state == "syncing" {
            s.state = "error".into();
            s.detail = Some(e.clone());
        }
    }
    result
}

async fn run_sync_inner(db: Arc<Mutex<DbState>>, runtime: SyncRuntime) -> Result<(), String> {
    let mut cfg = load_config();
    if !cfg.enabled || cfg.server_url.is_empty() || cfg.token.is_empty() {
        set_status(&runtime, "not_configured", None);
        return Err("sync non configurée".into());
    }
    if !db.lock().unwrap().is_open {
        return Err("aucun coffre ouvert".into());
    }
    set_status(&runtime, "syncing", None);
    let client = reqwest::Client::new();
    let url = format!("{}/api/vault", cfg.server_url);

    for _attempt in 0..3 {
        // ---- GET (jamais de If-Match : on veut l'état courant) ----
        let resp = match client.get(&url).bearer_auth(&cfg.token).send().await {
            Ok(r) => r,
            Err(e) => {
                set_status(&runtime, "offline", Some(e.to_string()));
                return Err(format!("serveur injoignable: {e}"));
            }
        };
        let (remote_etag, remote_bytes): (Option<String>, Option<Vec<u8>>) =
            match resp.status().as_u16() {
                200 => {
                    let etag = resp
                        .headers()
                        .get("etag")
                        .and_then(|v| v.to_str().ok())
                        .map(String::from);
                    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
                    (etag, Some(bytes))
                }
                404 => (None, None),
                401 => {
                    set_status(&runtime, "error", Some("token refusé (401)".into()));
                    return Err("token refusé".into());
                }
                s => {
                    set_status(&runtime, "error", Some(format!("GET {s}")));
                    return Err(format!("GET {s}"));
                }
            };

        // ---- Merge + décision de push, sous le lock, sans await ----
        let push_bytes: Option<Vec<u8>> = {
            let mut db = db.lock().unwrap();
            if !db.is_open || db.keepass_file.is_none() {
                return Err("coffre fermé pendant la sync".into());
            }
            let password = db.master_password.clone().ok_or("mot de passe non disponible")?;
            let keyfile = db.keyfile_data.clone();
            let keyfile = keyfile.as_deref().map(Vec::as_slice);

            match &remote_bytes {
                Some(bytes) => {
                    let remote =
                        kdbx::reader::read_database_bytes(bytes, &password, keyfile)
                            .map_err(|e| {
                                set_status(
                                    &runtime,
                                    "error",
                                    Some("coffre distant illisible (mot de passe différent ?)".into()),
                                );
                                format!("déchiffrement distant: {e}")
                            })?;
                    let kf = db.keepass_file.as_mut().unwrap();
                    let pulled = merge::merge(kf, &remote.keepass_file);
                    if pulled.changed() {
                        db.save()?;
                    }
                    // Delta tombstones-seulement : changed() l'ignore → propagation en un
                    // aller-retour de plus via le prochain push réel (tracé en revue, accepté).
                    // Le serveur a-t-il besoin de nos changements ?
                    let mut remote_view = remote.keepass_file.clone();
                    let to_push =
                        merge::merge(&mut remote_view, db.keepass_file.as_ref().unwrap());
                    if to_push.changed() {
                        Some(kdbx::writer::write_database_bytes(
                            db.keepass_file.as_ref().unwrap(),
                            &password,
                            keyfile,
                            db.cipher,
                            &db.kdf,
                        )?)
                    } else {
                        None
                    }
                }
                None => Some(kdbx::writer::write_database_bytes(
                    db.keepass_file.as_ref().unwrap(),
                    &password,
                    keyfile,
                    db.cipher,
                    &db.kdf,
                )?),
            }
        };

        // ---- PUT si nécessaire ----
        let Some(body) = push_bytes else {
            finish_synced(&runtime, &mut cfg, remote_etag)?;
            return Ok(());
        };
        let mut req = client.put(&url).bearer_auth(&cfg.token).body(body);
        if let Some(etag) = &remote_etag {
            req = req.header("If-Match", etag); // écho VERBATIM
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                set_status(&runtime, "offline", Some(e.to_string()));
                return Err(format!("PUT: {e}"));
            }
        };
        match resp.status().as_u16() {
            200 => {
                let new_etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);
                finish_synced(&runtime, &mut cfg, new_etag)?;
                return Ok(());
            }
            // 409 : version bougée entre GET et PUT → re-GET + re-merge.
            // 5xx : état serveur inconnu → même traitement (re-GET d'abord).
            409 | 500..=599 => continue,
            401 => {
                set_status(&runtime, "error", Some("token refusé (401)".into()));
                return Err("token refusé".into());
            }
            s => {
                set_status(&runtime, "error", Some(format!("PUT {s}")));
                return Err(format!("PUT {s}"));
            }
        }
    }
    set_status(&runtime, "error", Some("conflit persistant (3 tentatives)".into()));
    Err("conflit persistant".into())
}

fn finish_synced(
    runtime: &SyncRuntime,
    cfg: &mut SyncConfig,
    etag: Option<String>,
) -> Result<(), String> {
    cfg.last_etag = etag.clone();
    store_config(cfg)?;
    let mut s = runtime.lock().unwrap();
    s.state = "synced".into();
    s.detail = None;
    s.last_sync = Some(now_iso());
    s.server_version = etag.map(|e| e.trim_matches('"').to_string());
    Ok(())
}

fn now_iso() -> String {
    // Affichage uniquement (statut UI) — pas utilisé par la fusion.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_and_token_preservation() {
        let dir = std::env::temp_dir().join(format!("mypass-sync-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sync.json");

        let cfg = SyncConfig {
            server_url: "http://example:8787".into(),
            token: "secret".into(),
            enabled: true,
            last_etag: Some("\"3\"".into()),
        };
        store_config_at(&path, &cfg).unwrap();
        let loaded = load_config_at(&path);
        assert_eq!(loaded.server_url, "http://example:8787");
        assert_eq!(loaded.token, "secret");
        assert_eq!(loaded.last_etag.as_deref(), Some("\"3\""));

        // fichier absent → défaut silencieux
        let missing = load_config_at(&dir.join("nope.json"));
        assert!(!missing.enabled);
        assert!(missing.token.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn default_status_is_idle() {
        assert_eq!(SyncStatus::default().state, "idle");
    }

    #[test]
    fn fetched_sync_config_normalizes_inputs() {
        let cfg = fetched_sync_config(" https://mypass.example.ts.net/ ", "  tok \n", Some("\"7\"".into()));
        assert_eq!(cfg.server_url, "https://mypass.example.ts.net");
        assert_eq!(cfg.token, "tok");
        assert!(cfg.enabled);
        assert_eq!(cfg.last_etag.as_deref(), Some("\"7\""));
    }
}
