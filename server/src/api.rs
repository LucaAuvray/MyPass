use crate::AppState;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Signature KDBX4 (voir mypass-core writer.rs) — seule "compréhension" du
/// format côté serveur : tout le reste est un blob opaque.
fn is_kdbx(data: &[u8]) -> bool {
    data.len() >= 8
        && data[0..4] == 0x03D9A29Au32.to_le_bytes()
        && data[4..8] == 0x67FB4BB5u32.to_le_bytes()
}

pub fn etag(version: u64) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{version}\"")).expect("etag toujours ASCII")
}

fn internal(e: std::io::Error) -> Response {
    tracing::error!("vault store error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

pub async fn get_vault(State(state): State<AppState>) -> Response {
    let store = state.store.lock().await;
    match store.read_current() {
        Ok(Some((version, data))) => (
            StatusCode::OK,
            [
                (header::ETAG, etag(version)),
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                ),
            ],
            data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => internal(e),
    }
}

pub async fn put_vault(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !is_kdbx(&body) {
        return (StatusCode::BAD_REQUEST, "not a KDBX file").into_response();
    }
    let store = state.store.lock().await;
    let current = match store.current_version() {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    // Contrat : premier PUT sans If-Match ; ensuite If-Match = version courante.
    let if_match = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().trim_matches('"').to_string());
    if if_match != current.map(|v| v.to_string()) {
        let mut resp = StatusCode::CONFLICT.into_response();
        if let Some(v) = current {
            resp.headers_mut().insert(header::ETAG, etag(v));
        }
        return resp;
    }
    match store.write_new_version(&body) {
        Ok(new_version) => (StatusCode::OK, [(header::ETAG, etag(new_version))]).into_response(),
        Err(e) => internal(e),
    }
}

#[derive(Serialize)]
pub struct VersionInfo {
    pub version: u64,
    pub size: u64,
}

pub async fn list_versions(State(state): State<AppState>) -> Response {
    let store = state.store.lock().await;
    match store.list_versions() {
        Ok(versions) => axum::Json(
            versions
                .into_iter()
                .map(|(version, size)| VersionInfo { version, size })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => internal(e),
    }
}

pub async fn get_version(State(state): State<AppState>, Path(n): Path<u64>) -> Response {
    let store = state.store.lock().await;
    match store.read_version(n) {
        Ok(Some(data)) => (
            StatusCode::OK,
            [
                (header::ETAG, etag(n)),
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/octet-stream"),
                ),
            ],
            data,
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => internal(e),
    }
}
