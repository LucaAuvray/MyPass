/// Commandes Tauri pour les clés SSH : import (parsing) et génération.
/// La création de l'entrée KDBX passe par le `create_entry` existant côté
/// frontend (customFields SSH_*), il n'y a donc aucune écriture ici.
use crate::commands::database::DbState;
use crate::ssh::agent;
use crate::ssh::keys::{self, ParsedSshKey};
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyFields {
    pub private_key: String,
    pub public_key: String,
    pub fingerprint: String,
    pub algorithm: String,
    pub comment: String,
}

impl From<ParsedSshKey> for SshKeyFields {
    fn from(k: ParsedSshKey) -> Self {
        Self {
            private_key: k.private_key_openssh,
            public_key: k.public_key_openssh,
            fingerprint: k.fingerprint,
            algorithm: k.algorithm,
            comment: k.comment,
        }
    }
}

/// Parse le contenu d'un fichier de clé privée OpenSSH. Erreurs sentinelles
/// "PASSPHRASE_REQUIRED" / "PASSPHRASE_INVALID" interprétées par le frontend.
#[tauri::command]
pub async fn parse_ssh_key(
    content: String,
    passphrase: Option<String>,
) -> Result<SshKeyFields, String> {
    keys::parse_private_key(&content, passphrase.as_deref()).map(Into::into)
}

#[tauri::command]
pub async fn generate_ssh_key(comment: String) -> Result<SshKeyFields, String> {
    keys::generate_ed25519(&comment).map(Into::into)
}

#[tauri::command]
pub async fn is_ssh_agent_enabled() -> Result<bool, String> {
    Ok(agent::is_enabled())
}

/// Réponse de l'UI (popup de confirmation) à une demande de signature en attente.
#[tauri::command]
pub async fn respond_ssh_sign(request_id: String, approved: bool, remember: bool) -> Result<(), String> {
    agent::respond(&request_id, approved, remember);
    Ok(())
}

#[tauri::command]
pub async fn toggle_ssh_agent(
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<DbState>>>,
    enabled: bool,
) -> Result<bool, String> {
    agent::set_enabled(enabled)?;
    if enabled {
        agent::start(app, state.inner().clone());
    } else {
        agent::stop();
    }
    Ok(enabled)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshAgentStatus {
    pub enabled: bool,
    pub listening: bool,
    pub service_running: bool,
}

/// Le service Windows "OpenSSH Authentication Agent" (nom interne ssh-agent)
/// occupe le pipe s'il tourne — même conflit que 1Password.
fn windows_ssh_agent_service_running() -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("sc.exe")
        .args(["query", "ssh-agent"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
        .unwrap_or(false)
}

#[tauri::command]
pub async fn get_ssh_agent_status() -> Result<SshAgentStatus, String> {
    let service_running = tauri::async_runtime::spawn_blocking(windows_ssh_agent_service_running)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SshAgentStatus {
        enabled: agent::is_enabled(),
        listening: agent::is_listening(),
        service_running,
    })
}

/// Stoppe et désactive le service Windows via une invite UAC, puis l'UI
/// relance le toggle pour reprendre le pipe.
#[tauri::command]
pub async fn disable_windows_ssh_agent_service() -> Result<(), String> {
    let output = tauri::async_runtime::spawn_blocking(|| {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile -Command Stop-Service ssh-agent -ErrorAction SilentlyContinue; Set-Service ssh-agent -StartupType Disabled'",
            ])
            .creation_flags(0x0800_0000)
            .output()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Ne détecte que le refus UAC (Start-Process sans -PassThru jette dans ce
    // cas et powershell sort non-zéro) : un échec interne de Set-Service dans
    // le shell élevé n'est pas propagé ici — la bannière de conflit le
    // re-signalera au prochain refresh du statut.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() { "Service disable failed".to_string() } else { stderr });
    }
    Ok(())
}
