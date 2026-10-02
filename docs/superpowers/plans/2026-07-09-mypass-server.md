# `mypass-server` — Implementation Plan (sous-projet 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Le serveur de sync : un binaire Rust/Axum (`server/` dans le repo) qui stocke et versionne le fichier `.kdbx` chiffré, avec auth par token Bearer (hash Argon2), détection de conflit par `If-Match`, et tests d'intégration.

**Architecture:** Crate binaire autonome `server/` (pas de workspace, comme `crates/mypass-core`). Le serveur ne dépend PAS de `mypass-core` : il stocke des blobs opaques (il valide seulement les 8 octets magiques KDBX au PUT). Stockage = fichiers `vault.v{n}.kdbx` + `index.json` dans un répertoire de données, écritures atomiques (fichier temporaire + rename), rétention 50 versions. Un `tokio::sync::Mutex` global sérialise les accès (un seul utilisateur).

**Tech Stack:** Rust 2021, axum 0.8, tokio 1, argon2 0.5, rand 0.8, hex, serde/serde_json, tracing. Dev-deps tests : tower (oneshot), http-body-util, tempfile.

## Global Constraints

- Spec de référence : `docs/superpowers/specs/2026-07-09-sync-backend-design.md`, section « Composants → 1. mypass-server ».
- Endpoints exacts : `GET /api/health` (sans auth), `GET /api/vault`, `PUT /api/vault` (avec `If-Match`), `GET /api/vault/versions`, `GET /api/vault/versions/{n}` — les 4 derniers derrière l'auth Bearer.
- Conflit = **409** (choix du spec, pas 412) avec l'`ETag` de la version courante dans la réponse.
- Contrat `If-Match` : premier PUT (coffre vide) = SANS header `If-Match` ; ensuite `If-Match: "<version courante>"` obligatoire (guillemets d'ETag tolérés ou non). Tout écart → 409.
- Token : généré au premier démarrage (32 octets aléatoires → hex), affiché UNE fois sur la sortie du serveur, seul le hash Argon2 (format PHC) est stocké dans `token.hash`.
- Rétention : 50 dernières versions (`const RETENTION: u64 = 50`).
- Limite de corps : 100 Mo (`const MAX_BODY: usize = 100 * 1024 * 1024`) — le coffre contient des scans de documents.
- Pas de TLS : le VPN chiffre le transport et le coffre est chiffré E2E. Décision actée, à documenter dans un commentaire de `main.rs`.
- Pas de dépendance à `mypass-core` ni à tauri. Crate name `mypass-server`, version `0.1.0`, `edition = "2021"`, `rust-version = "1.85"`.
- Config par variables d'environnement : `MYPASS_DATA_DIR` (défaut `./data`), `MYPASS_BIND` (défaut `0.0.0.0:8787`).
- Le serveur tournera sous Linux (LXC) mais est développé/testé sous Windows : uniquement du std cross-platform (`std::fs::rename` remplace la destination sur les deux OS).
- axum 0.8 : syntaxe de routes `{n}` (pas `:n`), `axum::middleware::from_fn_with_state`, `axum::serve`.
- Le service des fichiers statiques de l'app web est HORS scope (sous-projet 4).

---

### Task 1: Scaffold du crate `server/` + `/api/health`

**Files:**
- Create: `server/Cargo.toml`
- Create: `server/src/lib.rs`
- Create: `server/src/main.rs` (stub, complété en Task 6)
- Create: `server/tests/common/mod.rs`
- Create: `server/tests/api.rs`
- Modify: `.gitignore` (racine)

**Interfaces:**
- Consumes: rien.
- Produces: `mypass_server::AppState { store: Arc<Mutex<store::VaultStore>>, token_hash: Arc<String> }` (Clone), `mypass_server::app(state: AppState) -> axum::Router`, helpers de test `common::{test_state, req, TEST_TOKEN}`. Les modules `store`/`auth`/`api` sont déclarés au fur et à mesure des tâches suivantes.

- [ ] **Step 1: Créer `server/Cargo.toml`**

```toml
[package]
name = "mypass-server"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
description = "MyPass sync server — versioned encrypted KDBX blob store"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
argon2 = "0.5"
rand = "0.8"
hex = "0.4"
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
tempfile = "3"
```

- [ ] **Step 2: Écrire le test qui échoue (`server/tests/api.rs` + helper commun)**

`server/tests/common/mod.rs` :

```rust
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

/// Requête pré-authentifiée (le header est inoffensif tant que l'auth n'existe pas).
pub fn req(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
}

pub fn empty() -> Body {
    Body::empty()
}
```

Note : `test_state` référence `mypass_server::store::VaultStore` qui n'existe qu'en Task 2. Pour cette tâche UNIQUEMENT, mettre dans `common/mod.rs` la version sans store ni argon2 ci-dessous, qui sera remplacée par la version complète ci-dessus au début de la Task 2 :

```rust
use axum::body::Body;
use axum::http::{Method, Request};

pub const TEST_TOKEN: &str = "test-token";

pub fn req(method: Method, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {TEST_TOKEN}"))
}

pub fn empty() -> Body {
    Body::empty()
}
```

`server/tests/api.rs` :

```rust
mod common;

use axum::http::{Method, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn health_is_ok_without_auth() {
    let app = mypass_server::app(mypass_server::AppState::default());
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
```

Note : `AppState::default()` n'existe que pour cette étape de scaffold ; la Task 2 fait disparaître `Default` (le state exige un vrai store) et ce test passera par `common::test_state`.

- [ ] **Step 3: Vérifier l'échec**

```powershell
cd server
cargo test
```

Expected: FAIL — compile error, `mypass_server` n'a pas encore de `lib.rs` avec `app`/`AppState`.

- [ ] **Step 4: Écrire `server/src/lib.rs` et `server/src/main.rs`**

`server/src/lib.rs` :

```rust
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;

/// Le coffre contient des scans de documents — laisser de la marge.
pub const MAX_BODY: usize = 100 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct AppState {
    // Champs réels ajoutés en Task 2 (store) et Task 5 (token_hash).
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}
```

`server/src/main.rs` (stub) :

```rust
fn main() {
    // Câblé en Task 6 (config env, token, store, axum::serve).
    eprintln!("mypass-server: wiring pending (Task 6)");
}
```

- [ ] **Step 5: Vérifier que le test passe**

```powershell
cd server
cargo test
```

Expected: PASS — 1 test (`health_is_ok_without_auth`). Warnings de items inutilisés dans `common` tolérés à ce stade (ils seront consommés dès la Task 3) ; s'ils apparaissent, ajouter `#![allow(dead_code)]` en tête de `common/mod.rs` avec le commentaire `// helpers consommés progressivement par les tâches suivantes`.

- [ ] **Step 6: Ignorer les artefacts de build**

Ajouter à la fin du `.gitignore` racine :

```
server/target/
```

(`server/Cargo.lock` EST committé — crate binaire.)

- [ ] **Step 7: Commit**

```powershell
git add server .gitignore
git status   # vérifier : server/{Cargo.toml,Cargo.lock,src,tests} + .gitignore, jamais target/
git commit -m "feat(server): scaffold mypass-server crate with /api/health

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `store.rs` — stockage versionné atomique

**Files:**
- Create: `server/src/store.rs`
- Modify: `server/src/lib.rs` (déclarer `pub mod store;`, vrai champ `store` dans `AppState`, suppression de `Default`)
- Modify: `server/tests/common/mod.rs` (version complète de `test_state`, voir Task 1 Step 2)
- Modify: `server/tests/api.rs` (le test health utilise `common::test_state`)
- Test: `#[cfg(test)] mod tests` dans `store.rs`

**Interfaces:**
- Consumes: rien.
- Produces: `store::VaultStore` avec exactement :
  - `pub fn new(dir: impl Into<PathBuf>) -> io::Result<VaultStore>` (crée le répertoire)
  - `pub fn current_version(&self) -> io::Result<Option<u64>>`
  - `pub fn read_current(&self) -> io::Result<Option<(u64, Vec<u8>)>>`
  - `pub fn read_version(&self, version: u64) -> io::Result<Option<Vec<u8>>>`
  - `pub fn write_new_version(&self, data: &[u8]) -> io::Result<u64>`
  - `pub fn list_versions(&self) -> io::Result<Vec<(u64, u64)>>` — `(version, taille octets)`, croissant
  - `pub const RETENTION: u64 = 50;`
  Et `AppState { pub store: Arc<tokio::sync::Mutex<VaultStore>> }` (plus de `Default`).

- [ ] **Step 1: Écrire les tests unitaires dans `store.rs` (module de test d'abord, corps vide du module principal)**

Tests à écrire (dans `#[cfg(test)] mod tests`, avec `tempfile::tempdir()`) :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn empty_store_has_no_version() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        assert_eq!(s.current_version().unwrap(), None);
        assert!(s.read_current().unwrap().is_none());
        assert!(s.list_versions().unwrap().is_empty());
    }

    #[test]
    fn write_then_read_roundtrip_and_versions_increment() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        assert_eq!(s.write_new_version(b"one").unwrap(), 1);
        assert_eq!(s.write_new_version(b"two").unwrap(), 2);
        assert_eq!(s.current_version().unwrap(), Some(2));
        let (v, data) = s.read_current().unwrap().unwrap();
        assert_eq!((v, data.as_slice()), (2, b"two".as_slice()));
        assert_eq!(s.read_version(1).unwrap().unwrap(), b"one");
        assert!(s.read_version(99).unwrap().is_none());
    }

    #[test]
    fn list_versions_reports_sizes_ascending() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        s.write_new_version(b"a").unwrap();
        s.write_new_version(b"bb").unwrap();
        assert_eq!(s.list_versions().unwrap(), vec![(1, 1), (2, 2)]);
    }

    #[test]
    fn retention_prunes_old_versions() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        for _ in 0..(RETENTION + 3) {
            s.write_new_version(b"x").unwrap();
        }
        let versions = s.list_versions().unwrap();
        assert_eq!(versions.len(), RETENTION as usize);
        assert_eq!(versions.first().unwrap().0, 4); // 1..=3 élaguées
        assert_eq!(versions.last().unwrap().0, RETENTION + 3);
        // les fichiers élagués sont bien partis du disque
        assert!(s.read_version(3).unwrap().is_none());
    }

    #[test]
    fn no_stray_tmp_files_after_write() {
        let dir = tempdir().unwrap();
        let s = VaultStore::new(dir.path().join("v")).unwrap();
        s.write_new_version(b"data").unwrap();
        let stray: Vec<_> = std::fs::read_dir(dir.path().join("v"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(stray.is_empty(), "tmp restants: {stray:?}");
    }
}
```

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd server
cargo test store
```

Expected: FAIL — `VaultStore` non défini.

- [ ] **Step 3: Implémenter `store.rs`**

```rust
/// Stockage versionné du coffre : blobs opaques `vault.v{n}.kdbx` + `index.json`.
/// Écritures atomiques (tmp + rename). Le serveur ne déchiffre jamais rien.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

pub const RETENTION: u64 = 50;

#[derive(Serialize, Deserialize)]
struct Index {
    current: u64,
}

pub struct VaultStore {
    dir: PathBuf,
}

impl VaultStore {
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn index_path(&self) -> PathBuf {
        self.dir.join("index.json")
    }

    fn vault_path(&self, version: u64) -> PathBuf {
        self.dir.join(format!("vault.v{version}.kdbx"))
    }

    pub fn current_version(&self) -> io::Result<Option<u64>> {
        match fs::read_to_string(self.index_path()) {
            Ok(s) => {
                let idx: Index = serde_json::from_str(&s)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(idx.current))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn read_current(&self) -> io::Result<Option<(u64, Vec<u8>)>> {
        match self.current_version()? {
            Some(v) => Ok(self.read_version(v)?.map(|data| (v, data))),
            None => Ok(None),
        }
    }

    pub fn read_version(&self, version: u64) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.vault_path(version)) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn write_new_version(&self, data: &[u8]) -> io::Result<u64> {
        let next = self.current_version()?.unwrap_or(0) + 1;

        // Blob d'abord, index ensuite : si on crashe entre les deux,
        // l'index pointe toujours sur une version complète.
        let vault_tmp = self.dir.join(format!("vault.v{next}.kdbx.tmp"));
        fs::write(&vault_tmp, data)?;
        fs::rename(&vault_tmp, self.vault_path(next))?;

        let index_tmp = self.dir.join("index.json.tmp");
        fs::write(&index_tmp, serde_json::to_string(&Index { current: next })?)?;
        fs::rename(&index_tmp, self.index_path())?;

        self.prune(next)?;
        Ok(next)
    }

    fn prune(&self, current: u64) -> io::Result<()> {
        for (version, _) in self.list_versions()? {
            if current.saturating_sub(version) >= RETENTION {
                fs::remove_file(self.vault_path(version))?;
            }
        }
        Ok(())
    }

    pub fn list_versions(&self) -> io::Result<Vec<(u64, u64)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(v) = name
                .strip_prefix("vault.v")
                .and_then(|s| s.strip_suffix(".kdbx"))
                .and_then(|s| s.parse::<u64>().ok())
            {
                out.push((v, entry.metadata()?.len()));
            }
        }
        out.sort_unstable();
        Ok(out)
    }
}
```

- [ ] **Step 4: Brancher dans `lib.rs` et mettre à jour les tests**

`server/src/lib.rs` — remplacer `AppState` et déclarer le module :

```rust
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use std::sync::Arc;

pub mod store;

/// Le coffre contient des scans de documents — laisser de la marge.
pub const MAX_BODY: usize = 100 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<tokio::sync::Mutex<store::VaultStore>>,
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}
```

`server/tests/common/mod.rs` — passer à la version complète (Task 1 Step 2), SANS le champ `token_hash` pour l'instant (il arrive en Task 5) :

```rust
#![allow(dead_code)] // helpers consommés progressivement par les tâches suivantes
use axum::body::Body;
use axum::http::{Method, Request};
use mypass_server::AppState;
use std::sync::Arc;

pub const TEST_TOKEN: &str = "test-token";

pub fn test_state(dir: &std::path::Path) -> AppState {
    AppState {
        store: Arc::new(tokio::sync::Mutex::new(
            mypass_server::store::VaultStore::new(dir.join("vaults")).unwrap(),
        )),
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
```

`server/tests/api.rs` — le test health devient :

```rust
mod common;

use axum::http::{Method, StatusCode};
use tower::ServiceExt;

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
```

- [ ] **Step 5: Vérifier que tout passe**

```powershell
cd server
cargo test
```

Expected: PASS — 5 tests unitaires store + 1 test health.

- [ ] **Step 6: Commit**

```powershell
git add server
git commit -m "feat(server): versioned atomic vault store with retention

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: `api.rs` — `GET`/`PUT /api/vault` avec `If-Match`/409

**Files:**
- Create: `server/src/api.rs`
- Modify: `server/src/lib.rs` (déclarer `pub mod api;`, router les endpoints)
- Test: `server/tests/api.rs` (tests d'intégration ajoutés)

**Interfaces:**
- Consumes: `store::VaultStore` (Task 2), `AppState`.
- Produces: handlers `api::get_vault`, `api::put_vault`. Contrat HTTP :
  - `GET /api/vault` → 200 + corps binaire + header `ETag: "<n>"` ; 404 si coffre vide.
  - `PUT /api/vault` corps binaire KDBX → 200 + `ETag: "<n+1>"` ; 400 si les 8 octets magiques KDBX manquent ; 409 + `ETag` courant si `If-Match` absent/différent de la version courante (sauf premier PUT : `If-Match` doit être ABSENT) ; 500 sur erreur disque.
  - Helper partagé `api::etag(n: u64) -> HeaderValue` (format `"n"` avec guillemets).

- [ ] **Step 1: Écrire les tests d'intégration qui échouent (ajout à `server/tests/api.rs`)**

```rust
use axum::body::Body;
use http_body_util::BodyExt;

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
```

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd server
cargo test
```

Expected: FAIL — routes absentes → les nouveaux tests reçoivent 404 au lieu des statuts attendus (ou erreur de compilation si `api` référencé).

- [ ] **Step 3: Implémenter `server/src/api.rs`**

```rust
use crate::AppState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

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
```

- [ ] **Step 4: Router dans `lib.rs`**

Dans `server/src/lib.rs`, ajouter `pub mod api;` sous `pub mod store;` et remplacer le `Router` :

```rust
pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/vault", get(api::get_vault).put(api::put_vault))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}
```

- [ ] **Step 5: Vérifier que tout passe**

```powershell
cd server
cargo test
```

Expected: PASS — 5 store + 6 intégration.

- [ ] **Step 6: Commit**

```powershell
git add server
git commit -m "feat(server): GET/PUT /api/vault with If-Match conflict detection

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Historique — `GET /api/vault/versions` et `GET /api/vault/versions/{n}`

**Files:**
- Modify: `server/src/api.rs` (2 handlers)
- Modify: `server/src/lib.rs` (2 routes)
- Test: `server/tests/api.rs`

**Interfaces:**
- Consumes: `store::{list_versions, read_version}` (Task 2), `api::etag` (Task 3).
- Produces:
  - `GET /api/vault/versions` → 200 JSON `[{"version": n, "size": octets}, ...]` croissant (liste vide si coffre vide).
  - `GET /api/vault/versions/{n}` → 200 + corps binaire + `ETag: "<n>"` ; 404 si inexistante. (Rollback = le client télécharge une ancienne version puis la re-`PUT`.)

- [ ] **Step 1: Tests d'intégration qui échouent (ajout à `server/tests/api.rs`)**

```rust
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
```

Ajouter en tête de `server/tests/api.rs` (si absent) : `use serde_json;` n'est pas nécessaire (chemin qualifié), mais `serde_json` doit être dans `[dev-dependencies]` — l'ajouter à `server/Cargo.toml` :

```toml
# dans [dev-dependencies]
serde_json = "1"
```

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd server
cargo test
```

Expected: FAIL — 404 sur les nouvelles routes.

- [ ] **Step 3: Implémenter les handlers (ajout à `server/src/api.rs`)**

```rust
use axum::extract::Path;
use serde::Serialize;

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
```

Routes dans `lib.rs` (avant le `.layer(...)`) :

```rust
        .route("/api/vault/versions", get(api::list_versions))
        .route("/api/vault/versions/{n}", get(api::get_version))
```

- [ ] **Step 4: Vérifier que tout passe**

```powershell
cd server
cargo test
```

Expected: PASS — 5 store + 8 intégration.

- [ ] **Step 5: Commit**

```powershell
git add server
git commit -m "feat(server): version history endpoints for rollback

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: Auth Bearer (token + hash Argon2)

**Files:**
- Create: `server/src/auth.rs`
- Modify: `server/src/lib.rs` (champ `token_hash`, middleware sur les routes vault)
- Modify: `server/tests/common/mod.rs` (state avec hash du `TEST_TOKEN`)
- Test: `server/tests/api.rs` + `#[cfg(test)]` dans `auth.rs`

**Interfaces:**
- Consumes: `AppState` (Tasks 2-4).
- Produces:
  - `AppState` gagne `pub token_hash: Arc<String>` (hash PHC Argon2 du token).
  - `auth::load_or_create_token_hash(data_dir: &Path) -> Result<String, String>` — lit `token.hash` s'il existe ; sinon génère 32 octets aléatoires → token hex (64 chars), affiche le token UNE fois sur stderr, écrit le hash, retourne le hash.
  - `auth::require_token` — middleware axum : toutes les routes `/api/vault*` exigent `Authorization: Bearer <token>` vérifié contre le hash ; sinon 401. `/api/health` reste ouvert.

- [ ] **Step 1: Tests qui échouent**

Ajout à `server/tests/api.rs` :

```rust
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
```

Test unitaire du bootstrap dans `auth.rs` :

```rust
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
```

(Rien à changer dans `Cargo.toml` : `tempfile` en `[dev-dependencies]` couvre aussi les tests unitaires `#[cfg(test)]` de la lib.)

- [ ] **Step 2: Vérifier l'échec**

```powershell
cd server
cargo test
```

Expected: FAIL — compile error (`auth` absent) après ajout du test unitaire, et le test 401 reçoit 200/404 tant que le middleware n'existe pas.

- [ ] **Step 3: Implémenter `server/src/auth.rs`**

```rust
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
```

- [ ] **Step 4: Brancher dans `lib.rs` et le helper de test**

`server/src/lib.rs` — état final du fichier :

```rust
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
    let vault = Router::new()
        .route("/api/vault", get(api::get_vault).put(api::put_vault))
        .route("/api/vault/versions", get(api::list_versions))
        .route("/api/vault/versions/{n}", get(api::get_version))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ));
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .merge(vault)
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}
```

`server/tests/common/mod.rs` — `test_state` complet :

```rust
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
```

- [ ] **Step 5: Vérifier que tout passe**

```powershell
cd server
cargo test
```

Expected: PASS — 5 store + 1 auth + 9 intégration. Les tests des Tasks 3-4 passent sans modification : `common::req` envoie le token depuis le début.

- [ ] **Step 6: Commit**

```powershell
git add server
git commit -m "feat(server): bearer token auth with argon2-hashed token bootstrap

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: `main.rs` — câblage, config env, smoke test manuel

**Files:**
- Modify: `server/src/main.rs`
- Test: smoke test manuel (curl) + `cargo clippy`

**Interfaces:**
- Consumes: tout ce qui précède.
- Produces: binaire `mypass-server` utilisable : `MYPASS_DATA_DIR` (défaut `./data`), `MYPASS_BIND` (défaut `0.0.0.0:8787`).

- [ ] **Step 1: Implémenter `server/src/main.rs`**

```rust
use mypass_server::{AppState, app, auth, store};
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
    axum::serve(listener, app(state)).await.expect("serveur HTTP");
}
```

- [ ] **Step 2: Vérifications automatiques**

```powershell
cd server
cargo test
cargo clippy
```

Expected: tests tous PASS, clippy zéro warning (crate neuf — exigence stricte, contrairement à src-tauri).

- [ ] **Step 3: Smoke test manuel**

Terminal 1 :

```powershell
cd server
$env:MYPASS_DATA_DIR = "$env:TEMP\mypass-server-smoke"; cargo run
```

Noter le token affiché au premier démarrage. Terminal 2 (remplacer `<TOKEN>`) :

```powershell
curl.exe -s http://127.0.0.1:8787/api/health                     # → ok
curl.exe -s -o NUL -w "%{http_code}" http://127.0.0.1:8787/api/vault                          # → 401
curl.exe -s -o NUL -w "%{http_code}" -H "Authorization: Bearer <TOKEN>" http://127.0.0.1:8787/api/vault   # → 404 (coffre vide)
```

Puis avec le coffre de test du repo (`src-tauri\test-vault.kdbx` — vérifier le nom exact du fichier .kdbx présent dans `src-tauri\`) :

```powershell
curl.exe -s -w "%{http_code}" -X PUT -H "Authorization: Bearer <TOKEN>" --data-binary "@..\src-tauri\test-vault.kdbx" http://127.0.0.1:8787/api/vault   # → 200
curl.exe -s -o NUL -w "%{http_code}" -H "Authorization: Bearer <TOKEN>" http://127.0.0.1:8787/api/vault   # → 200
curl.exe -s -H "Authorization: Bearer <TOKEN>" http://127.0.0.1:8787/api/vault/versions   # → [{"version":1,"size":...}]
```

Consigner les codes obtenus dans le rapport. Arrêter le serveur (Ctrl+C), supprimer `$env:TEMP\mypass-server-smoke`.

- [ ] **Step 4: Commit**

```powershell
git add server
git commit -m "feat(server): wire main with env config and token bootstrap

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Vérification finale du sous-projet

```powershell
cd server && cargo test && cargo clippy
cd ../src-tauri && cargo test        # inchangé — le serveur ne touche à rien d'existant
```

Critère de sortie (spec, « Découpage », point 2) : API vault versionnée + auth token + tests d'intégration, tous verts, clippy propre. Le déploiement LXC/systemd est le sous-projet 5 ; la consommation par l'app desktop est le sous-projet 3.
