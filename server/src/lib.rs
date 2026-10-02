use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::get;
use std::sync::Arc;

pub mod api;
pub mod auth;
pub mod store;

/// Le coffre contient des scans de documents — laisser de la marge.
pub const MAX_BODY: usize = 100 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<tokio::sync::Mutex<store::VaultStore>>,
    pub token_hash: Arc<String>,
}

pub fn app(state: AppState) -> Router {
    app_with_static(state, None)
}

/// Router complet ; si `static_dir` est fourni, sert l'app web (build Vite)
/// en fallback : les routes /api gardent priorité, toute autre URL retombe
/// sur index.html (routing SPA côté client).
pub fn app_with_static(state: AppState, static_dir: Option<std::path::PathBuf>) -> Router {
    let vault = Router::new()
        .route("/api/vault", get(api::get_vault).put(api::put_vault))
        .route("/api/vault/versions", get(api::list_versions))
        .route("/api/vault/versions/{n}", get(api::get_version))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ));
    let router = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .merge(vault)
        // Les /api inconnus doivent rester des 404 même quand les statiques
        // sont servies : sans cet attrape-tout, fallback_service (qui REMPLACE
        // le .fallback ci-dessous) les transformerait en index.html 200.
        // {*rest} (matchit) exige un segment non vide, et axum 0.8 ne
        // redirige plus le slash final → routes dédiées pour /api et /api/.
        .route(
            "/api",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .route(
            "/api/",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .route(
            "/api/{*rest}",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
        // ponytail: sans ce fallback explicite, merge() hérite du fallback
        // 404 par défaut du sous-routeur `vault`, déjà enveloppé par le
        // middleware d'auth par son .layer() — un chemin inconnu répondrait
        // 401 au lieu de 404. On force un vrai 404 pour les routes non /api.
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND });

    // Hébergement d'installeurs : tout fichier déposé dans MYPASS_DOWNLOAD_DIR
    // est servi sous /download/<nom> (ex: le MSI desktop), pour qu'un PC neuf
    // récupère l'app depuis un lien. Dossier distinct du build web → un
    // redéploiement de la webapp ne l'efface pas. Public (pas d'auth), ce qui
    // convient derrière le VPN. La route nommée /download prime sur le
    // fallback SPA.
    let router = match std::env::var("MYPASS_DOWNLOAD_DIR") {
        Ok(dir) => router.nest_service("/download", tower_http::services::ServeDir::new(dir)),
        Err(_) => router,
    };

    match static_dir {
        Some(dir) => router
            .fallback_service(
                tower_http::services::ServeDir::new(&dir)
                    .fallback(tower_http::services::ServeFile::new(dir.join("index.html"))),
            )
            .layer(middleware::from_fn(static_cache_policy)),
        None => router,
    }
}

/// Politique de cache des statiques : l'index.html DOIT être revalidé à
/// chaque visite (no-cache) — sinon, après un redéploiement, un index mis en
/// cache heuristique référence des assets aux hashes disparus et la page se
/// charge sans CSS/JS. Les assets Vite sont hashés → cache long immuable.
/// Les réponses /api (JSON/octets, jamais text/html ni /assets/) ne matchent
/// aucune des deux branches et restent sans directive de cache.
async fn static_cache_policy(
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    use axum::http::{HeaderValue, header};
    let is_asset = req.uri().path().starts_with("/assets/");
    let mut res = next.run(req).await;
    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"text/html"));
    // Un asset au hash disparu retombe sur le fallback SPA (index.html) :
    // il ne doit surtout pas être caché « immutable » — le test is_html prime.
    if is_asset && !is_html {
        res.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    } else if is_html {
        res.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    res
}
