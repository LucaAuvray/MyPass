//! Native Messaging Host for KeePassXC-Browser protocol.
//!
//! Communicates with browser extensions via stdin/stdout using the
//! KeePassXC-Browser protocol (NaCl box encryption).
//!
//! Message format:
//! - 4-byte length prefix (little-endian u32)
//! - JSON message body: { action, message, nonce, clientID }
//!
//! Protocol actions handled:
//! - change-public-keys: Initial NaCl key exchange
//! - associate: Link browser to vault
//! - test-associate: Verify association
//! - get-logins: Retrieve credentials for URL
//! - set-login: Create/update credential
//! - get-totp: Get TOTP code
//! - get-identities: Retrieve identities and cards for form filling
//! - generate-password: Generate random password
//! - lock-database: Lock the vault
//! - get-databasehash: Get database hash
//! - get-database-groups: Get group list
//! - create-new-group: Create new group
//! - request-autotype: Global auto-type

use crate::commands::browser;
use crate::commands::database::DbState;
use crate::security::nacl;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Loopback port the main app listens on so the standalone Native Messaging
/// Host process (spawned separately by the browser, with its own empty
/// process memory) can reach the app's actual unlocked database instead of
/// operating on a database of its own that can never be unlocked.
const BRIDGE_PORT: u16 = 25798;

/// Number of browser connections currently open on the local bridge.
/// Read by `commands::browser::get_browser_status` to report real
/// connected/disconnected state instead of a hardcoded placeholder.
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

pub fn active_connection_count() -> usize {
    ACTIVE_CONNECTIONS.load(Ordering::Relaxed)
}

/// Handle to the main app, used to bring the window forward when the browser
/// asks to unlock (`triggerUnlock`) a closed vault. Set once at startup; stays
/// unset (and focus becomes a no-op) in the `--native-messaging` proxy process.
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

fn focus_main_window() {
    use tauri::Manager;
    if let Some(app) = APP_HANDLE.get() {
        if let Some(w) = app.webview_windows().values().next() {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

/// Last-activity time (Unix seconds) per associated browser id, so
/// `commands::browser::get_browser_status` can show *when* a browser was
/// last seen, not just whether one is connected right now.
static LAST_SEEN: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn record_activity(id_key: &str) {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let map = LAST_SEEN.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut map) = map.lock() {
        map.insert(id_key.to_string(), now);
    }
}

/// Unix seconds this browser id was last active, as a string (empty if
/// never seen since the app started — ponytail: in-memory only, resets on
/// restart, add persistence if that turns out to matter).
pub fn last_seen(id_key: &str) -> String {
    let Some(map) = LAST_SEEN.get() else { return String::new() };
    map.lock()
        .ok()
        .and_then(|m| m.get(id_key).copied())
        .map(|secs| secs.to_string())
        .unwrap_or_default()
}

// ── Message Types ─────────────────────────────────────────────────

/// Version we advertise to the extension. It compares this against real
/// KeePassXC releases to decide which features to enable (password
/// generator ≥ 2.7.0, passkeys ≥ 2.7.7, …) — an arbitrary "2.0" would make
/// it disable everything, including the association flow.
const ADVERTISED_VERSION: &str = "2.7.10";

#[derive(Debug, Deserialize)]
struct NativeRequest {
    action: String,
    message: Option<String>,
    nonce: Option<String>,
    #[serde(rename = "clientID")]
    client_id: String,
    #[serde(rename = "publicKey", default)]
    public_key: Option<String>,
    #[serde(rename = "triggerUnlock", default)]
    trigger_unlock: Option<String>,
}

#[derive(Debug, Serialize)]
struct NativeResponse {
    action: Option<String>,
    message: Option<String>,
    nonce: Option<String>,
    #[serde(rename = "clientID")]
    client_id: Option<String>,
    error: Option<String>,
    #[serde(rename = "errorCode")]
    error_code: Option<String>,
    version: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

// ── Session State ─────────────────────────────────────────────────

struct SessionState {
    /// Browser's public key (from change-public-keys)
    browser_public_key: Option<Vec<u8>>,
    /// Our key pair (generated on change-public-keys)
    our_keypair: Option<(Vec<u8>, Vec<u8>)>,
    /// Whether the browser is associated
    associated: bool,
    /// Association id key (stored in KDBX custom data)
    id_key: Option<String>,
}

impl SessionState {
    fn new() -> Self {
        Self {
            browser_public_key: None,
            our_keypair: None,
            associated: false,
            id_key: None,
        }
    }
}

// ── Native Messaging Loop ─────────────────────────────────────────

/// Start the native messaging host directly over stdio, handling requests
/// in-process. Only useful when this process itself owns the unlocked
/// database — which the standalone `--native-messaging` process does not,
/// since the browser launches it as a fresh, separate process. Kept for
/// completeness/testing; the real entry point is `run_native_messaging_proxy`.
pub fn run_native_messaging(db_state: Arc<Mutex<DbState>>) -> Result<(), String> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    run_message_loop(&mut stdin, &mut stdout, &db_state)
}

/// Start a local TCP server the main app listens on for the lifetime of the
/// process, so the standalone Native Messaging Host process (see
/// `run_native_messaging_proxy`) can reach the app's real, already-unlocked
/// database. Runs in a dedicated background thread; errors (e.g. port already
/// in use because another MyPass instance is running) are logged, not fatal.
///
/// SECURITY: this listens on loopback with no peer authentication — any local
/// process can connect. Once the vault is unlocked, the only gate is NaCl
/// association (a browser must present an `idKey` stored in the vault). This is
/// the same trust model as KeePassXC's local socket: it assumes the local user
/// account is trusted. A malicious local process running as the same user could
/// associate and read credentials while the vault is open. Mitigations if that
/// threat matters: bind to a per-user secret path, or require an explicit
/// in-app confirmation for each new association.
pub fn start_local_bridge(db_state: Arc<Mutex<DbState>>) {
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", BRIDGE_PORT)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Native messaging bridge: failed to bind port {BRIDGE_PORT}: {e}");
                return;
            }
        };

        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let db_state = db_state.clone();
            std::thread::spawn(move || {
                let mut reader = match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let mut writer = stream;
                ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                let _ = run_message_loop(&mut reader, &mut writer, &db_state);
                ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
            });
        }
    });
}

/// Entry point for the browser-launched `mypass --native-messaging` process.
/// This process has no access to the running app's unlocked database (it's a
/// separate OS process with its own empty memory), so instead of handling
/// the KeePassXC-Browser protocol itself, it acts as a thin byte-for-byte
/// relay between the browser's stdio pipes and the main app's local bridge
/// socket (`start_local_bridge`), where the real, unlocked database lives.
pub fn run_native_messaging_proxy() -> Result<(), String> {
    let stream = TcpStream::connect(("127.0.0.1", BRIDGE_PORT))
        .map_err(|e| format!("MyPass is not running (bridge connect failed: {e})"))?;

    let mut to_app = stream.try_clone().map_err(|e| e.to_string())?;
    let from_app_thread = std::thread::spawn(move || {
        // Not io::copy: Rust's stdout is line-buffered, and these frames
        // contain no newlines — copy would sit on the response forever.
        // Flush after every chunk so the browser sees each frame immediately.
        let mut from_app = stream;
        let mut stdout = std::io::stdout();
        let mut buf = [0u8; 8192];
        loop {
            match from_app.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdout.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
            }
        }
    });

    let mut stdin = std::io::stdin();
    let _ = std::io::copy(&mut stdin, &mut to_app);
    let _ = from_app_thread.join();

    Ok(())
}

/// Core protocol loop shared by both the direct stdio host and the local
/// bridge: read length-prefixed frames, route each to `handle_action`, write
/// back the (possibly encrypted) response.
fn run_message_loop(
    reader: &mut impl Read,
    writer: &mut impl Write,
    db_state: &Arc<Mutex<DbState>>,
) -> Result<(), String> {
    let mut sessions: HashMap<String, SessionState> = HashMap::new();

    loop {
        // 1. Read 4-byte length prefix (little-endian u32)
        let mut len_buf = [0u8; 4];
        if reader.read_exact(&mut len_buf).is_err() {
            // Connection closed — browser disconnected
            break;
        }
        let msg_len = u32::from_le_bytes(len_buf) as usize;

        // Sanity check on message size
        if msg_len == 0 || msg_len > 16 * 1024 * 1024 {
            // Skip empty or oversized messages (>16MB)
            continue;
        }

        // 2. Read message body
        let mut msg_buf = vec![0u8; msg_len];
        if reader.read_exact(&mut msg_buf).is_err() {
            break;
        }

        // 3. Parse JSON
        let req: NativeRequest = match serde_json::from_slice(&msg_buf) {
            Ok(r) => r,
            Err(e) => {
                let error_resp = NativeResponse {
                    action: None,
                    message: None,
                    nonce: None,
                    client_id: None,
                    error: Some(format!("JSON parse error: {e}")),
                    error_code: Some("1".to_string()),
                    version: None,
                    extra: HashMap::new(),
                };
                send_response(writer, &error_resp, None);
                continue;
            }
        };

        let client_id = req.client_id.clone();
        let session = sessions.entry(client_id.clone()).or_insert_with(SessionState::new);

        // 4. Handle action
        let mut response = handle_action(&req, session, db_state);

        if let Some(id_key) = &session.id_key {
            record_activity(id_key);
        }

        // 5. Send response. The extension matches responses to pending
        // requests by nonce: every reply must carry the request nonce
        // incremented by one (libsodium semantics), both as the outer nonce
        // and inside the encrypted payload.
        let resp_nonce = req
            .nonce
            .as_deref()
            .and_then(base64_decode)
            .map(|n| nacl::increment_nonce(&n));

        // Detect errors via error_code, not error: the set-login success
        // response must carry error:"success" (KeePassXC protocol quirk —
        // the extension reads that exact string to report "saved").
        if response.error_code.is_some() {
            // Errors go out in plaintext without a nonce — that's the shape
            // the extension's error path matches on (by action).
            response.nonce = None;
            response.message = None;
            send_response(writer, &response, None);
        } else {
            response.nonce = resp_nonce.as_deref().map(base64_encode);

            // Encrypted once the key exchange is done. The change-public-keys
            // reply itself must stay plaintext: it carries our public key,
            // which the browser needs *to* decrypt anything at all.
            let encrypt_keys = if req.action == "change-public-keys" {
                None
            } else {
                match (&session.browser_public_key, &session.our_keypair, &resp_nonce) {
                    (Some(bpk), Some((_, sk)), Some(nonce)) => {
                        Some((bpk.as_slice(), sk.as_slice(), nonce.as_slice()))
                    }
                    _ => None,
                }
            };

            send_response(writer, &response, encrypt_keys);
        }
    }

    Ok(())
}

// ── Action Router ─────────────────────────────────────────────────

fn handle_action(
    req: &NativeRequest,
    session: &mut SessionState,
    db_state: &Arc<Mutex<DbState>>,
) -> NativeResponse {
    if !browser::is_enabled() {
        return error_response(req, 9, "Browser integration is disabled in MyPass");
    }

    // If the browser asks to unlock and the vault is closed, surface the app
    // window so the user can enter their master password.
    if matches!(req.trigger_unlock.as_deref(), Some("true")) {
        let closed = db_state.lock().map(|d| d.keepass_file.is_none()).unwrap_or(true);
        if closed {
            focus_main_window();
        }
    }

    match req.action.as_str() {
        "change-public-keys" => handle_change_public_keys(req, session),
        "associate" => handle_associate(req, session, db_state),
        "test-associate" => handle_test_associate(req, session, db_state),
        "get-logins" => handle_get_logins(req, session, db_state),
        "set-login" => handle_set_login(req, session, db_state),
        "get-totp" => handle_get_totp(req, session, db_state),
        "get-identities" => handle_get_identities_action(req, session, db_state),
        "generate-password" => handle_generate_password(req, session),
        "lock-database" => handle_lock_database(req, session, db_state),
        "get-databasehash" => handle_get_databasehash(req, session, db_state),
        "get-database-groups" => handle_get_database_groups(req, session, db_state),
        "create-new-group" => handle_create_new_group(req, session, db_state),
        "request-autotype" => handle_request_autotype(req, session),
        _ => error_response(req, 0, &format!("Unknown action: {}", req.action)),
    }
}

// ── Action Handlers ───────────────────────────────────────────────

fn handle_change_public_keys(req: &NativeRequest, session: &mut SessionState) -> NativeResponse {
    // Browser sends: { action: "change-public-keys", publicKey, nonce, clientID }
    // We respond: { action: "change-public-keys", publicKey, nonce, success, version, clientID }

    // The extension sends its key in the `publicKey` field (the `message`
    // fallback covers older/other clients that used that field instead).
    let browser_pk = match req.public_key.as_ref().or(req.message.as_ref()) {
        Some(msg) => match base64_decode(msg) {
            Some(key) if key.len() == 32 => key,
            _ => return error_response(req, 1, "Invalid public key format"),
        },
        None => return error_response(req, 1, "Missing publicKey"),
    };

    session.browser_public_key = Some(browser_pk);

    // Generate our key pair
    let (our_pk, our_sk) = nacl::generate_keypair();
    session.our_keypair = Some((our_pk.clone(), our_sk));

    NativeResponse {
        action: Some("change-public-keys".to_string()),
        message: Some(base64_encode(&our_pk)),
        nonce: req.nonce.clone(),
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: Some(ADVERTISED_VERSION.to_string()),
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("publicKey".to_string(), serde_json::Value::String(base64_encode(&our_pk)));
            map
        },
    }
}

fn handle_associate(req: &NativeRequest, session: &mut SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    // The encrypted payload carries `idKey`: a public key the extension
    // generates once per association and presents again in every future
    // test-associate. That's what gets persisted — not the session transport
    // key, which changes at every browser restart.
    let inner = match decrypt_message(req, session) {
        Ok(msg) => msg,
        Err(e) => return error_response(req, 1, &format!("Decrypt error: {e}")),
    };
    let Some(id_key) = inner.get("idKey").and_then(|v| v.as_str()) else {
        return error_response(req, 1, "Missing idKey in associate request");
    };

    let mut db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_mut() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    let name = format!("MyPass-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    kf.meta.custom_data.items.push(crate::kdbx::xml::CustomDataItem {
        key: format!("Browser_{name}"),
        value: id_key.to_string(),
    });
    let hash = database_hash(kf);

    // Persist so the association survives app restarts, otherwise the browser
    // would have to re-associate every launch.
    if let Err(e) = db.save() {
        return error_response(req, 4, &format!("Failed to save association: {e}"));
    }

    session.associated = true;
    session.id_key = Some(name.clone());

    NativeResponse {
        action: Some("associate".to_string()),
        message: None,
        nonce: None, // set by the message loop (request nonce + 1)
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: Some(ADVERTISED_VERSION.to_string()),
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("hash".to_string(), serde_json::Value::String(hash));
            map.insert("id".to_string(), serde_json::Value::String(name));
            map
        },
    }
}

fn handle_test_associate(req: &NativeRequest, session: &mut SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    let inner = match decrypt_message(req, session) {
        Ok(msg) => msg,
        Err(e) => return error_response(req, 1, &format!("Decrypt error: {e}")),
    };
    let id = inner.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let key = inner.get("key").and_then(|v| v.as_str()).unwrap_or("");

    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let Some(kf) = db.keepass_file.as_ref() else {
        return error_response(req, 2, "No open database");
    };

    // Succeed only if this id was associated before and the idKey the
    // extension presents matches the one stored at associate time.
    let stored_key = format!("Browser_{id}");
    let known = kf
        .meta
        .custom_data
        .items
        .iter()
        .find(|i| i.key == stored_key)
        .is_some_and(|item| item.value == key);

    if !known {
        return error_response(req, 8, "Association not found — please re-associate");
    }

    let hash = database_hash(kf);
    drop(db);
    session.associated = true;
    session.id_key = Some(id.to_string());

    NativeResponse {
        action: Some("test-associate".to_string()),
        message: None,
        nonce: None, // set by the message loop (request nonce + 1)
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: Some(ADVERTISED_VERSION.to_string()),
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("hash".to_string(), serde_json::Value::String(hash));
            map.insert("id".to_string(), serde_json::Value::String(id.to_string()));
            map
        },
    }
}

/// SHA-256 of the root group UUID, hex-encoded — the vault's *identity*, like
/// KeePassXC (`BrowserService::getDatabaseHash`). Must be STABLE across content
/// changes: the extension keys its stored associations on this hash and drops
/// the association whenever it changes. Hashing the whole XML (as before) made
/// association impossible — writing the association itself changed the hash,
/// invalidating the association it had just created.
fn database_hash(kf: &crate::kdbx::xml::KeePassFile) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(kf.root.group.uuid.as_bytes()))
}

fn handle_get_logins(req: &NativeRequest, session: &SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    // Decrypt inner message
    let inner = match decrypt_message(req, session) {
        Ok(msg) => msg,
        Err(e) => return error_response(req, 1, &format!("Decrypt error: {e}")),
    };

    let url = inner.get("url").and_then(|v| v.as_str()).unwrap_or("");

    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_ref() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    let entries = browser::handle_get_logins(kf, url).unwrap_or_default();
    let count = entries.len();

    // message/nonce are left None here: the message loop sets nonce (request
    // nonce + 1) and encrypts uniformly. A `nonce` key in `extra` would
    // collide with the flattened struct field and emit a duplicate JSON key.
    NativeResponse {
        action: Some("get-logins".to_string()),
        message: None,
        nonce: None,
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("count".to_string(), serde_json::json!(count));
            map.insert("entries".to_string(), serde_json::json!(entries));
            map
        },
    }
}

fn handle_get_identities_action(
    req: &NativeRequest,
    session: &SessionState,
    db_state: &Arc<Mutex<DbState>>,
) -> NativeResponse {
    // Valide la session/nonce comme les autres actions chiffrées ; le
    // contenu du message est vide (pas de filtre URL : les identités ne
    // sont pas liées à un site).
    if let Err(e) = decrypt_message(req, session) {
        return error_response(req, 1, &format!("Decrypt error: {e}"));
    }

    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_ref() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    let identities = browser::handle_get_identities(kf);

    NativeResponse {
        action: Some("get-identities".to_string()),
        message: None,
        nonce: None,
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("identities".to_string(), serde_json::json!(identities));
            map
        },
    }
}

fn handle_set_login(req: &NativeRequest, session: &SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    let inner = match decrypt_message(req, session) {
        Ok(msg) => msg,
        Err(e) => return error_response(req, 1, &format!("Decrypt error: {e}")),
    };

    let url = inner.get("url").and_then(|v| v.as_str()).unwrap_or("");
    let login = inner.get("login").and_then(|v| v.as_str()).unwrap_or("");
    let password = inner.get("password").and_then(|v| v.as_str()).unwrap_or("");
    let uuid = inner.get("uuid").and_then(|v| v.as_str());

    let mut db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_mut() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    if let Err(e) = browser::handle_set_login(kf, url, login, password, uuid) {
        return error_response(req, 4, &format!("Set login error: {e}"));
    }

    // Persist so a credential saved from the browser isn't lost on close/lock.
    if let Err(e) = db.save() {
        return error_response(req, 4, &format!("Failed to save credential: {e}"));
    }

    NativeResponse {
        action: Some("set-login".to_string()),
        message: None,
        nonce: None,
        client_id: Some(req.client_id.clone()),
        // "success" is what the extension checks to report created/updated
        // (keepass.js updateCredentials: response.error === 'success').
        error: Some("success".to_string()),
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("count".to_string(), serde_json::json!(0));
            map
        },
    }
}

fn handle_get_totp(req: &NativeRequest, session: &SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    let inner = match decrypt_message(req, session) {
        Ok(msg) => msg,
        Err(e) => return error_response(req, 1, &format!("Decrypt error: {e}")),
    };

    let uuid = inner.get("uuid").and_then(|v| v.as_str()).unwrap_or("");

    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let kf = match db.keepass_file.as_ref() {
        Some(kf) => kf,
        None => return error_response(req, 2, "No open database"),
    };

    // "" when the entry has no usable 2FA: the extension then reports no TOTP found.
    let totp = browser::handle_get_totp(kf, uuid);

    NativeResponse {
        action: Some("get-totp".to_string()),
        message: None,
        nonce: None,
        client_id: Some(req.client_id.clone()),
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("totp".to_string(), serde_json::Value::String(totp));
            map
        },
    }
}

fn handle_generate_password(_req: &NativeRequest, _session: &SessionState) -> NativeResponse {
    // Generate a random 20-char password
    let charset = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*";
    let mut rng = rand::thread_rng();
    let password: String = (0..20)
        .map(|_| {
            let idx = rng.gen_range(0..charset.len());
            charset[idx] as char
        })
        .collect();

    NativeResponse {
        action: Some("generate-password".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("password".to_string(), serde_json::Value::String(password));
            map
        },
    }
}

fn handle_lock_database(req: &NativeRequest, _session: &SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    let mut db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    // Same lock as the app's own button: wipes the master password and keyfile too.
    crate::ssh::agent::clear_session_approvals();
    db.lock_vault();

    NativeResponse {
        action: Some("lock-database".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map
        },
    }
}

fn handle_get_databasehash(req: &NativeRequest, _session: &SessionState, db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    let db = match db_state.lock() {
        Ok(db) => db,
        Err(_) => return error_response(req, 3, "Database lock error"),
    };

    let Some(kf) = db.keepass_file.as_ref() else {
        return error_response(req, 2, "No open database");
    };

    NativeResponse {
        action: Some("get-databasehash".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("hash".to_string(), serde_json::Value::String(database_hash(kf)));
            map
        },
    }
}

fn handle_get_database_groups(_req: &NativeRequest, _session: &SessionState, _db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    NativeResponse {
        action: Some("get-database-groups".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: None,
        error_code: None,
        version: None,
        extra: {
            let mut map = HashMap::new();
            map.insert("success".to_string(), serde_json::Value::String("true".to_string()));
            map.insert("groups".to_string(), serde_json::json!({
                "groups": [{
                    "name": "Root",
                    "uuid": "root",
                    "children": []
                }]
            }));
            map
        },
    }
}

fn handle_create_new_group(_req: &NativeRequest, _session: &SessionState, _db_state: &Arc<Mutex<DbState>>) -> NativeResponse {
    NativeResponse {
        action: Some("create-new-group".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: Some("Create group via browser not yet implemented".to_string()),
        error_code: Some("8".to_string()),
        version: None,
        extra: HashMap::new(),
    }
}

fn handle_request_autotype(_req: &NativeRequest, _session: &SessionState) -> NativeResponse {
    NativeResponse {
        action: Some("request-autotype".to_string()),
        message: None,
        nonce: None,
        client_id: None,
        error: Some("Auto-type not supported".to_string()),
        error_code: Some("8".to_string()),
        version: None,
        extra: HashMap::new(),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn error_response(req: &NativeRequest, code: u8, msg: &str) -> NativeResponse {
    NativeResponse {
        action: req.action.clone().into(),
        message: req.message.clone(),
        nonce: req.nonce.clone(),
        client_id: req.client_id.clone().into(),
        error: Some(msg.to_string()),
        error_code: Some(code.to_string()),
        version: None,
        extra: HashMap::new(),
    }
}

fn decrypt_message(
    req: &NativeRequest,
    session: &SessionState,
) -> Result<HashMap<String, serde_json::Value>, String> {
    let ciphertext = req.message.as_deref().unwrap_or("");
    let nonce = req.nonce.as_deref().unwrap_or("");

    if ciphertext.is_empty() || nonce.is_empty() {
        return Err("Empty message or nonce".to_string());
    }

    let ct_bytes = base64_decode(ciphertext).ok_or("Invalid base64 in message")?;
    let nonce_bytes = base64_decode(nonce).ok_or("Invalid base64 in nonce")?;

    let browser_pk = session.browser_public_key.as_deref()
        .ok_or("No browser public key")?;
    let our_sk = session.our_keypair.as_ref()
        .map(|(_, sk)| sk.as_slice())
        .ok_or("No keypair")?;

    let plaintext = nacl::decrypt(&ct_bytes, &nonce_bytes, browser_pk, our_sk)?;
    let json: HashMap<String, serde_json::Value> =
        serde_json::from_slice(&plaintext).map_err(|e| format!("JSON parse: {e}"))?;

    Ok(json)
}

fn send_response(
    stdout: &mut dyn Write,
    response: &NativeResponse,
    encrypt_key: Option<(&[u8], &[u8], &[u8])>,
) {
    let resp_json = serde_json::to_string(response).unwrap_or_else(|_| "{}".to_string());

    // Encrypt if keys are available. The nonce is imposed by the protocol
    // (request nonce + 1) — it is both the box nonce and the wrapper nonce,
    // which is how the extension pairs this response with its request.
    let final_json = if let Some((browser_pk, our_sk, nonce)) = encrypt_key {
        match nacl::encrypt_with_nonce(resp_json.as_bytes(), nonce, browser_pk, our_sk) {
            Ok(ciphertext) => {
                let wrapper = NativeResponse {
                    action: response.action.clone(),
                    message: Some(base64_encode(&ciphertext)),
                    nonce: Some(base64_encode(nonce)),
                    client_id: response.client_id.clone(),
                    error: None,
                    error_code: None,
                    version: None,
                    extra: HashMap::new(),
                };
                serde_json::to_string(&wrapper).unwrap_or_else(|_| "{}".to_string())
            }
            Err(_) => resp_json,
        }
    } else {
        resp_json
    };

    let len = (final_json.len() as u32).to_le_bytes();
    let _ = stdout.write_all(&len);
    let _ = stdout.write_all(final_json.as_bytes());
    let _ = stdout.flush();
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdbx::xml::KeePassFile;
    use zeroize::Zeroizing;

    #[test]
    fn extension_lock_wipes_password_and_keyfile() {
        let db = Arc::new(Mutex::new(DbState {
            is_open: true,
            keepass_file: Some(KeePassFile::new("t")),
            master_password: Some(Zeroizing::new("pw".to_string())),
            keyfile_data: Some(Zeroizing::new(vec![1, 2, 3])),
            ..Default::default()
        }));
        let req = NativeRequest {
            action: "lock-database".to_string(),
            message: None,
            nonce: None,
            client_id: "c".to_string(),
            public_key: None,
            trigger_unlock: None,
        };
        handle_lock_database(&req, &SessionState::new(), &db);
        let db = db.lock().unwrap();
        assert!(!db.is_open);
        assert!(db.keepass_file.is_none());
        assert!(db.master_password.is_none());
        assert!(db.keyfile_data.is_none());
    }
}
