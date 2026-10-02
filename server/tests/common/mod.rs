#![allow(dead_code)] // helpers consommés progressivement par les tâches suivantes
use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
use argon2::Argon2;
use axum::body::Body;
use axum::http::{Method, Request};
use mypass_server::AppState;
use std::sync::Arc;

pub const TEST_TOKEN: &str = "test-token";

pub fn test_state(dir: &std::path::Path) -> AppState {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(TEST_TOKEN.as_bytes(), &salt)
        .unwrap()
        .to_string();
    AppState {
        store: Arc::new(tokio::sync::Mutex::new(
            mypass_server::store::VaultStore::new(dir.join("vaults")).unwrap(),
        )),
        token_hash: Arc::new(hash),
    }
}

pub fn req(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
}

pub fn empty() -> Body {
    Body::empty()
}
