use crate::AppState;
use argon2::password_hash::{PasswordHasher, SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rand::RngCore;
use std::fs;
use std::path::Path;

/// Lit token.hash, ou au premier démarrage génère le token (affiché une
/// seule fois — il n'est jamais stocké en clair) et écrit son hash Argon2.
pub fn load_or_create_token_hash(data_dir: &Path) -> Result<String, String> {
    let path = data_dir.join("token.hash");
    match fs::read_to_string(&path) {
        Ok(h) => return Ok(h.trim().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("lecture {}: {e}", path.display())),
    }
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| format!("hash token: {e}"))?
        .to_string();
    fs::write(&path, &hash).map_err(|e| format!("écriture {}: {e}", path.display()))?;
    eprintln!("=== Token d'accès MyPass (affiché une seule fois, note-le) ===");
    eprintln!("{token}");
    eprintln!("==============================================================");
    Ok(hash)
}

// ponytail: vérif Argon2 à chaque requête (~dizaines de ms, bloquant) —
// un seul utilisateur derrière VPN ; cacher le token accepté si ça compte un jour.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .is_some_and(|token| {
            PasswordHash::new(&state.token_hash)
                .map(|h| Argon2::default().verify_password(token.as_bytes(), &h).is_ok())
                .unwrap_or(false)
        });
    if authorized {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bootstrap_creates_then_reuses_hash() {
        let dir = tempdir().unwrap();
        let h1 = load_or_create_token_hash(dir.path()).unwrap();
        assert!(h1.starts_with("$argon2"));
        let h2 = load_or_create_token_hash(dir.path()).unwrap();
        assert_eq!(h1, h2, "le second démarrage relit le même hash");
    }
}
