/// Agent SSH intégré : sert les clés du coffre sur le named pipe OpenSSH de
/// Windows. Même modèle que le bridge navigateur : tout vit dans le process
/// principal (tauri-plugin-single-instance), la clé privée ne sort jamais.
use crate::commands::database::DbState;
use ssh_agent_lib::agent::{listen, NamedPipeListener, Session};
use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Identity, SignRequest};
use ssh_key::{PrivateKey, PublicKey, Signature};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tauri::Emitter;
use tokio::sync::oneshot;

/// Pipe par défaut du client OpenSSH Windows (ssh.exe, VS Code Remote-SSH).
pub const PIPE_NAME: &str = r"\\.\pipe\openssh-ssh-agent";

static LISTENING: AtomicBool = AtomicBool::new(false);
static AGENT_TASK: LazyLock<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    LazyLock::new(|| Mutex::new(None));

/// (fingerprint de la clé demandée, canal de réponse) pour une request_id en attente.
type PendingEntry = (String, oneshot::Sender<bool>);

/// Clé = request_id. Le fingerprint mémorisé ici, et non celui fourni par
/// l'appelant de `respond`, est ce qui va dans SESSION_APPROVED : un appel
/// erroné ou tardif ne peut donc pas pré-approuver une clé que l'utilisateur
/// n'a pas vue.
static PENDING: LazyLock<Mutex<HashMap<String, PendingEntry>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// Fingerprints approuvés « pour cette session de déverrouillage ».
static SESSION_APPROVED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Réponse de l'UI à une demande de signature. Un request_id inconnu (expiré
/// par le timeout de 30 s, ou jamais émis) est un no-op silencieux.
pub fn respond(request_id: &str, approved: bool, remember: bool) {
    let Some((fingerprint, tx)) = PENDING.lock().unwrap().remove(request_id) else {
        return;
    };
    // Fingerprint vide = entrée SSH créée à la main sans SSH_Fingerprint :
    // ne jamais l'ajouter à SESSION_APPROVED, ça pré-approuverait toute
    // autre clé sans fingerprint.
    if approved && remember && !fingerprint.is_empty() {
        SESSION_APPROVED.lock().unwrap().insert(fingerprint);
    }
    let _ = tx.send(approved);
}

/// À appeler au verrouillage du coffre : les approbations « session » tombent.
pub fn clear_session_approvals() {
    SESSION_APPROVED.lock().unwrap().clear();
}

/// Demande l'accord de l'utilisateur via l'UI. Timeout 30 s = refus.
async fn confirm(app: &tauri::AppHandle, key: &VaultKey) -> bool {
    if !key.fingerprint.is_empty() && SESSION_APPROVED.lock().unwrap().contains(&key.fingerprint) {
        return true;
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    PENDING.lock().unwrap().insert(request_id.clone(), (key.fingerprint.clone(), tx));

    let _ = app.emit(
        "ssh-sign-request",
        serde_json::json!({
            "requestId": request_id,
            "title": key.title,
            "fingerprint": key.fingerprint,
        }),
    );

    let approved = matches!(
        tokio::time::timeout(std::time::Duration::from_secs(30), rx).await,
        Ok(Ok(true))
    );
    PENDING.lock().unwrap().remove(&request_id);
    // Ferme la popup si elle est encore affichée (timeout / refus côté agent).
    let _ = app.emit("ssh-sign-request-closed", serde_json::json!({ "requestId": request_id }));
    approved
}

/// Une clé SSH du coffre, prête à servir.
pub struct VaultKey {
    pub title: String,
    pub public: PublicKey,
    pub private_pem: String,
    pub fingerprint: String,
}

/// Toutes les clés SSH du coffre. Vide si le coffre est verrouillé ou si une
/// entrée est malformée (clé publique illisible => entrée ignorée).
pub fn vault_keys(db: &Arc<Mutex<DbState>>) -> Vec<VaultKey> {
    let Ok(guard) = db.lock() else { return Vec::new() };
    let Some(kf) = guard.keepass_file.as_ref() else { return Vec::new() };
    crate::commands::browser::collect_all_entries(&kf.root.group)
        .iter()
        .filter(|e| e.get_field("MyPass_Type") == Some("ssh_key"))
        .filter_map(|e| {
            let public = PublicKey::from_openssh(e.get_field("SSH_PublicKey")?).ok()?;
            Some(VaultKey {
                title: e.title().to_string(),
                fingerprint: e.get_field("SSH_Fingerprint").unwrap_or_default().to_string(),
                private_pem: e.get_field("SSH_PrivateKey")?.to_string(),
                public,
            })
        })
        .collect()
}

#[derive(Clone)]
pub struct MyPassAgent {
    pub db: Arc<Mutex<DbState>>,
    pub app: tauri::AppHandle,
}

fn refused(msg: &str) -> AgentError {
    AgentError::other(std::io::Error::new(std::io::ErrorKind::PermissionDenied, msg.to_string()))
}

#[ssh_agent_lib::async_trait]
impl Session for MyPassAgent {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        Ok(vault_keys(&self.db)
            .into_iter()
            .map(|k| Identity { pubkey: k.public.key_data().clone(), comment: k.title })
            .collect())
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        let key = vault_keys(&self.db)
            .into_iter()
            .find(|k| k.public.key_data() == &request.pubkey)
            .ok_or_else(|| refused("unknown key"))?;

        if !confirm(&self.app, &key).await {
            return Err(refused("signature refused by user"));
        }

        let private = PrivateKey::from_openssh(&key.private_pem).map_err(AgentError::other)?;
        use signature::Signer;
        // ponytail: RSA toujours signé rsa-sha2-512 sans regarder request.flags ;
        // OK pour tout serveur OpenSSH moderne. Gérer les flags si un serveur
        // rsa-sha2-256-only se présente un jour.
        private.try_sign(&request.data).map_err(AgentError::other)
    }
}

// --- Réglage on/off (même pattern fichier que browser.rs, pour survivre au
// redémarrage sans nouvel état en base) ---

fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(appdata).join("MyPass").join("ssh_agent_settings.json"))
}

/// Opt-in : désactivé par défaut, comme l'intégration navigateur.
pub fn is_enabled() -> bool {
    let Some(path) = config_path() else { return false };
    let Ok(content) = std::fs::read_to_string(path) else { return false };
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let path = config_path().ok_or("Could not resolve config directory (%APPDATA%)")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::json!({ "enabled": enabled }).to_string())
        .map_err(|e| e.to_string())
}

// --- Cycle de vie du listener ---

pub fn start_if_enabled(app: tauri::AppHandle, db: Arc<Mutex<DbState>>) {
    if is_enabled() {
        start(app, db);
    }
}

pub fn start(app: tauri::AppHandle, db: Arc<Mutex<DbState>>) {
    let mut task = AGENT_TASK.lock().unwrap();
    // Ne pas doubler un listener vivant ; un task mort (bind raté) est remplacé.
    if task.is_some() && is_listening() {
        return;
    }
    if let Some(old) = task.take() {
        old.abort();
    }
    let agent = MyPassAgent { db, app };
    *task = Some(tauri::async_runtime::spawn(async move {
        match NamedPipeListener::bind(PIPE_NAME) {
            Ok(listener) => {
                LISTENING.store(true, Ordering::SeqCst);
                if let Err(e) = listen(listener, agent).await {
                    tracing::error!("SSH agent stopped: {e}");
                }
            }
            // Typiquement : service Windows "OpenSSH Authentication Agent"
            // déjà propriétaire du pipe. Le statut UI le signale (Task 5).
            Err(e) => tracing::warn!("SSH agent pipe unavailable: {e}"),
        }
        LISTENING.store(false, Ordering::SeqCst);
    }));
}

pub fn stop() {
    if let Some(task) = AGENT_TASK.lock().unwrap().take() {
        task.abort();
    }
    LISTENING.store(false, Ordering::SeqCst);
}

pub fn is_listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::database::DbState;
    use crate::kdbx::ops::entries::apply_custom_fields;
    use crate::kdbx::xml::{Entry, KeePassFile};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn db_with_key() -> Arc<Mutex<DbState>> {
        let generated = crate::ssh::keys::generate_ed25519("test@mypass").unwrap();
        let mut kf = KeePassFile::new("test");
        kf.root.group.entries.push(Entry::new("Login normal", "u", "p", "https://x.io"));
        let mut e = Entry::new("Clé prod", "", "", "");
        let mut f = HashMap::new();
        f.insert("MyPass_Type".to_string(), "ssh_key".to_string());
        f.insert("SSH_PrivateKey".to_string(), generated.private_key_openssh.clone());
        f.insert("SSH_PublicKey".to_string(), generated.public_key_openssh.clone());
        f.insert("SSH_Fingerprint".to_string(), generated.fingerprint.clone());
        apply_custom_fields(&mut e, &f);
        kf.root.group.entries.push(e);

        let db = DbState { is_open: true, keepass_file: Some(kf), ..Default::default() };
        Arc::new(Mutex::new(db))
    }

    #[test]
    fn vault_keys_returns_only_ssh_entries_with_parsed_public_key() {
        let db = db_with_key();
        let keys = vault_keys(&db);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].title, "Clé prod");
        assert!(keys[0].fingerprint.starts_with("SHA256:"));
        assert_eq!(keys[0].public.key_data().algorithm().to_string(), "ssh-ed25519");
    }

    #[test]
    fn vault_keys_is_empty_when_locked() {
        let db = Arc::new(Mutex::new(DbState::default()));
        assert!(vault_keys(&db).is_empty());
    }

    #[tokio::test]
    async fn respond_approves_pending_request_and_remembers() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        PENDING.lock().unwrap().insert("req-1".to_string(), ("SHA256:abc".to_string(), tx));

        respond("req-1", true, true);

        assert!(rx.await.unwrap());
        assert!(SESSION_APPROVED.lock().unwrap().contains("SHA256:abc"));

        clear_session_approvals();
        assert!(!SESSION_APPROVED.lock().unwrap().contains("SHA256:abc"));
    }

    #[tokio::test]
    async fn respond_deny_does_not_remember() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        PENDING.lock().unwrap().insert("req-2".to_string(), ("SHA256:def".to_string(), tx));

        respond("req-2", false, true);

        assert!(!rx.await.unwrap());
        assert!(!SESSION_APPROVED.lock().unwrap().contains("SHA256:def"));
    }

    #[tokio::test]
    async fn respond_remember_with_empty_fingerprint_does_not_pollute_session_approved() {
        let (tx, rx) = tokio::sync::oneshot::channel();
        PENDING.lock().unwrap().insert("req-empty-fp".to_string(), ("".to_string(), tx));

        respond("req-empty-fp", true, true);

        assert!(rx.await.unwrap());
        assert!(!SESSION_APPROVED.lock().unwrap().contains(""));
    }

    #[tokio::test]
    async fn respond_unknown_request_is_a_noop() {
        // Statics sont process-wide (partagés entre tests qui tournent en
        // parallèle) : on n'observe que des noms uniques à ce test, jamais
        // l'état global, pour rester indépendant des autres tests du module.
        let (tx, _rx) = tokio::sync::oneshot::channel();
        PENDING
            .lock()
            .unwrap()
            .insert("req-real-noop".to_string(), ("SHA256:realfp-noop".to_string(), tx));

        respond("req-unknown-does-not-exist-noop", true, true);

        assert!(!SESSION_APPROVED.lock().unwrap().contains("SHA256:realfp-noop"));
        assert!(PENDING.lock().unwrap().contains_key("req-real-noop"), "unrelated pending request must survive");
    }

    #[test]
    fn vault_keys_skips_malformed_public_keys() {
        let db = db_with_key();
        {
            let mut guard = db.lock().unwrap();
            let kf = guard.keepass_file.as_mut().unwrap();
            let mut bad = Entry::new("Clé cassée", "", "", "");
            let mut f = HashMap::new();
            f.insert("MyPass_Type".to_string(), "ssh_key".to_string());
            f.insert("SSH_PublicKey".to_string(), "pas une clé publique".to_string());
            f.insert("SSH_PrivateKey".to_string(), "x".to_string());
            apply_custom_fields(&mut bad, &f);
            kf.root.group.entries.push(bad);
        }
        assert_eq!(vault_keys(&db).len(), 1);
    }
}
