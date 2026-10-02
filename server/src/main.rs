use mypass_server::{AppState, auth, store};
use std::path::PathBuf;
use std::sync::Arc;

// Pas de TLS : le serveur vit derrière un VPN (WireGuard/Tailscale) qui
// chiffre le transport, et le coffre est de toute façon chiffré de bout en
// bout — le serveur ne stocke que des blobs KDBX opaques.
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let data_dir =
        PathBuf::from(std::env::var("MYPASS_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let bind = std::env::var("MYPASS_BIND").unwrap_or_else(|_| "0.0.0.0:8787".into());

    std::fs::create_dir_all(&data_dir).expect("création du répertoire de données");
    let token_hash = auth::load_or_create_token_hash(&data_dir).expect("bootstrap token");
    let vault_store =
        store::VaultStore::new(data_dir.join("vaults")).expect("initialisation du store");

    let state = AppState {
        store: Arc::new(tokio::sync::Mutex::new(vault_store)),
        token_hash: Arc::new(token_hash),
    };

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|e| panic!("bind {bind}: {e}"));
    tracing::info!("mypass-server à l'écoute sur {bind} (données: {})", data_dir.display());
    let static_dir = std::env::var("MYPASS_STATIC_DIR").ok().map(PathBuf::from);
    if let Some(dir) = &static_dir {
        tracing::info!("app web servie depuis {}", dir.display());
    }
    if let Ok(dir) = std::env::var("MYPASS_DOWNLOAD_DIR") {
        tracing::info!("téléchargements servis sous /download depuis {dir}");
    }
    axum::serve(listener, mypass_server::app_with_static(state, static_dir))
        .await
        .expect("serveur HTTP");
}
