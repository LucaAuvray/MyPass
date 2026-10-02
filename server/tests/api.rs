mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Un blob avec la signature KDBX4 + un octet discriminant.
fn kdbx_bytes(tag: u8) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0x03D9A29Au32.to_le_bytes());
    v.extend_from_slice(&0x67FB4BB5u32.to_le_bytes());
    v.push(tag);
    v
}

fn etag_of(resp: &axum::response::Response) -> String {
    resp.headers()
        .get(axum::http::header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn health_is_ok_without_auth() {
    let dir = tempfile::tempdir().unwrap();
    let app = mypass_server::app(common::test_state(dir.path()));
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn get_vault_empty_is_404() {
    let dir = tempfile::tempdir().unwrap();
    let app = mypass_server::app(common::test_state(dir.path()));
    let resp = app
        .oneshot(common::req(Method::GET, "/api/vault").body(common::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn first_put_then_get_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());

    let resp = mypass_server::app(state.clone())
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .body(Body::from(kdbx_bytes(1)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(etag_of(&resp), "\"1\"");

    let resp = mypass_server::app(state)
        .oneshot(common::req(Method::GET, "/api/vault").body(common::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(etag_of(&resp), "\"1\"");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), kdbx_bytes(1).as_slice());
}

#[tokio::test]
async fn put_with_matching_if_match_creates_next_version() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());
    mypass_server::app(state.clone())
        .oneshot(common::req(Method::PUT, "/api/vault").body(Body::from(kdbx_bytes(1))).unwrap())
        .await
        .unwrap();

    let resp = mypass_server::app(state)
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .header("if-match", "\"1\"")
                .body(Body::from(kdbx_bytes(2)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(etag_of(&resp), "\"2\"");
}

#[tokio::test]
async fn put_with_stale_or_missing_if_match_is_409_with_current_etag() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());
    for tag in [1u8, 2] {
        let if_match = if tag == 1 { None } else { Some("\"1\"") };
        let mut builder = common::req(Method::PUT, "/api/vault");
        if let Some(im) = if_match {
            builder = builder.header("if-match", im);
        }
        mypass_server::app(state.clone())
            .oneshot(builder.body(Body::from(kdbx_bytes(tag))).unwrap())
            .await
            .unwrap();
    }
    // version courante = 2 ; If-Match périmé
    let resp = mypass_server::app(state.clone())
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .header("if-match", "\"1\"")
                .body(Body::from(kdbx_bytes(9)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert_eq!(etag_of(&resp), "\"2\"");
    // If-Match absent alors que le coffre existe → 409 aussi
    let resp = mypass_server::app(state)
        .oneshot(common::req(Method::PUT, "/api/vault").body(Body::from(kdbx_bytes(9))).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn put_rejects_non_kdbx_body() {
    let dir = tempfile::tempdir().unwrap();
    let app = mypass_server::app(common::test_state(dir.path()));
    let resp = app
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .body(Body::from(b"not a vault".to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_versions_returns_json_history() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());
    mypass_server::app(state.clone())
        .oneshot(common::req(Method::PUT, "/api/vault").body(Body::from(kdbx_bytes(1))).unwrap())
        .await
        .unwrap();
    mypass_server::app(state.clone())
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .header("if-match", "\"1\"")
                .body(Body::from(kdbx_bytes(2)))
                .unwrap(),
        )
        .await
        .unwrap();

    let resp = mypass_server::app(state)
        .oneshot(common::req(Method::GET, "/api/vault/versions").body(common::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let expected_size = kdbx_bytes(1).len() as u64;
    assert_eq!(
        json,
        serde_json::json!([
            {"version": 1, "size": expected_size},
            {"version": 2, "size": expected_size}
        ])
    );
}

#[tokio::test]
async fn get_specific_version_and_missing_version() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());
    mypass_server::app(state.clone())
        .oneshot(common::req(Method::PUT, "/api/vault").body(Body::from(kdbx_bytes(7))).unwrap())
        .await
        .unwrap();

    let resp = mypass_server::app(state.clone())
        .oneshot(common::req(Method::GET, "/api/vault/versions/1").body(common::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(etag_of(&resp), "\"1\"");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), kdbx_bytes(7).as_slice());

    let resp = mypass_server::app(state)
        .oneshot(common::req(Method::GET, "/api/vault/versions/42").body(common::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_on_empty_store_with_if_match_is_409() {
    let dir = tempfile::tempdir().unwrap();
    let app = mypass_server::app(common::test_state(dir.path()));
    let resp = app
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .header("if-match", "\"1\"")
                .body(Body::from(kdbx_bytes(1)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    assert!(resp.headers().get(axum::http::header::ETAG).is_none());
}

#[tokio::test]
async fn put_accepts_unquoted_if_match() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());
    mypass_server::app(state.clone())
        .oneshot(common::req(Method::PUT, "/api/vault").body(Body::from(kdbx_bytes(1))).unwrap())
        .await
        .unwrap();

    let resp = mypass_server::app(state)
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .header("if-match", "1")
                .body(Body::from(kdbx_bytes(2)))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(etag_of(&resp), "\"2\"");
}

#[tokio::test]
async fn put_over_body_limit_is_413() {
    let dir = tempfile::tempdir().unwrap();
    let app = mypass_server::app(common::test_state(dir.path()));
    let mut body = vec![0u8; mypass_server::MAX_BODY + 1];
    body[0..8].copy_from_slice(&kdbx_bytes(1)[0..8]);
    let resp = app
        .oneshot(
            common::req(Method::PUT, "/api/vault")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn vault_routes_require_valid_token() {
    let dir = tempfile::tempdir().unwrap();
    let state = common::test_state(dir.path());

    // Sans header
    let resp = mypass_server::app(state.clone())
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/vault")
                .body(common::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Mauvais token
    let resp = mypass_server::app(state)
        .oneshot(
            axum::http::Request::builder()
                .method(Method::GET)
                .uri("/api/vault")
                .header("authorization", "Bearer wrong")
                .body(common::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
