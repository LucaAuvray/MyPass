//! Le serveur sert l'app web (build Vite) en fallback des routes /api.
mod common;

use http_body_util::BodyExt;
use mypass_server::app_with_static;
use tower::ServiceExt;

fn request(uri: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder().uri(uri).body(axum::body::Body::empty()).unwrap()
}

#[tokio::test]
async fn serves_index_and_spa_fallback_and_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>mypass-web</html>").unwrap();
    std::fs::write(web.join("assets/app.js"), "console.log(1)").unwrap();

    let app = app_with_static(common::test_state(tmp.path()), Some(web));

    // Racine → index.html
    let res = app.clone().oneshot(request("/")).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("mypass-web"));

    // Asset réel
    let res = app.clone().oneshot(request("/assets/app.js")).await.unwrap();
    assert_eq!(res.status(), 200);

    // Route SPA inconnue → index.html (react-router gère côté client)
    let res = app.clone().oneshot(request("/sync")).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("mypass-web"));

    // /api/* garde la priorité et son auth (401 sans token)
    let res = app.clone().oneshot(request("/api/vault")).await.unwrap();
    assert_eq!(res.status(), 401);
    let res = app.oneshot(request("/api/health")).await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn cache_headers_no_cache_html_immutable_assets() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>mypass-web</html>").unwrap();
    std::fs::write(web.join("assets/app.js"), "console.log(1)").unwrap();

    let app = app_with_static(common::test_state(tmp.path()), Some(web));

    // index.html (direct et fallback SPA) : no-cache — sans ça le navigateur
    // met l'index en cache heuristique et référence des assets aux hashes
    // disparus après un redéploiement (page sans CSS / erreur MIME).
    for uri in ["/", "/sync"] {
        let res = app.clone().oneshot(request(uri)).await.unwrap();
        assert_eq!(
            res.headers().get("cache-control").map(|v| v.to_str().unwrap()),
            Some("no-cache"),
            "cache-control manquant pour {uri}"
        );
    }

    // Assets hashés par Vite : cache long immuable.
    let res = app.clone().oneshot(request("/assets/app.js")).await.unwrap();
    assert_eq!(
        res.headers().get("cache-control").map(|v| v.to_str().unwrap()),
        Some("public, max-age=31536000, immutable")
    );

    // Un asset au hash disparu retombe sur index.html (fallback SPA) :
    // no-cache, jamais immutable (sinon le HTML empoisonne le cache 1 an).
    let res = app.clone().oneshot(request("/assets/gone-Abc123.js")).await.unwrap();
    assert_eq!(
        res.headers().get("cache-control").map(|v| v.to_str().unwrap()),
        Some("no-cache")
    );

    // /api n'est pas concerné par la politique de cache statique.
    let res = app.oneshot(request("/api/health")).await.unwrap();
    assert!(res.headers().get("cache-control").is_none());
}

#[tokio::test]
async fn without_static_dir_root_stays_404() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_static(common::test_state(tmp.path()), None);
    let res = app.oneshot(request("/")).await.unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn unknown_api_route_is_404_not_spa_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "<html>mypass-web</html>").unwrap();

    let app = app_with_static(common::test_state(tmp.path()), Some(web));

    let res = app.clone().oneshot(request("/api/nope")).await.unwrap();
    assert_eq!(res.status(), 404);
    let res = app.clone().oneshot(request("/api/vault/nope/deep")).await.unwrap();
    assert_eq!(res.status(), 404);
    // {*rest} (matchit) exige un segment non vide : /api et /api/ ont besoin
    // d'une route dédiée pour ne pas retomber sur le fallback SPA.
    let res = app.clone().oneshot(request("/api")).await.unwrap();
    assert_eq!(res.status(), 404);
    let res = app.oneshot(request("/api/")).await.unwrap();
    assert_eq!(res.status(), 404);
}
