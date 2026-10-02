# Sous-projet 4b — Crate WASM `mypass-wasm` : Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un crate `crates/mypass-wasm` compilé en wasm32 qui expose au navigateur le cycle de vie complet du coffre (ouvrir/CRUD/fusionner/sauvegarder), en réutilisant `mypass-core` sans réimplémentation — fondation du mode « web » de `src/lib/tauri.ts` (sous-projet 4c).

**Architecture:** Une session unique en mémoire (`thread_local`, wasm est mono-thread) qui mime le `DbState` du desktop : `KeePassFile` déchiffré + credentials + cipher/KDF pour re-chiffrer à la sauvegarde. Les fonctions exportées reflètent les commandes Tauri, avec JSON (strings) en entrée/sortie — les DTOs serde de `mypass_core::ops` sont réutilisés tels quels. Prérequis réglé en premier : `SystemTime::now()` panique en wasm à l'exécution → helper `time::unix_now()` cfg-gated sur `js_sys::Date`.

**Tech Stack:** Rust 2021 (rust-version 1.85), wasm-bindgen 0.2, serde_json, wasm-pack (via npm), Node ≥ 24 pour le smoke test.

## Global Constraints

- Pas de workspace Cargo : `mypass-wasm` dépend de `mypass-core` par chemin (`path = "../mypass-core"`), comme `src-tauri` et conformément au sous-projet 1.
- Cible `wasm32-unknown-unknown` (déjà installée sur la machine — validée au sous-projet 1). `getrandom` 0.2 feature `js` est déjà déclarée dans `mypass-core` pour cette cible.
- Erreurs : `Result<_, String>` partout (wasm-bindgen convertit `Err(String)` en exception JS ; testable nativement sans types JS).
- API JSON : les fonctions retournent des `String` JSON sérialisées avec serde_json ; le TS fera `JSON.parse` (4c). Pas de `serde-wasm-bindgen` (dépendance en plus pour rien).
- `crate-type = ["cdylib", "rlib"]` obligatoire : `cdylib` pour wasm-pack, `rlib` pour que `cargo test` natif fonctionne.
- Les fonctions `#[wasm_bindgen]` restent des fonctions Rust normales sur cible native → tous les tests unitaires tournent en natif avec `cargo test` ; la vérification runtime wasm est le smoke test Node de la Task 5.
- Commentaires en français, style des fichiers existants de `mypass-core`.
- `npm run lint` n'est pas concerné (aucun fichier TS modifié dans ce sous-projet).
- Après toute modification de `mypass-core`, vérifier que le desktop compile encore : `cd src-tauri && cargo check`.

## File Structure

- `crates/mypass-core/src/time.rs` (nouveau) — `unix_now()` cfg-gated, seule source d'heure du crate.
- `crates/mypass-core/src/{lib,xml,totp,merge}.rs` + `Cargo.toml` (modifiés) — branchement sur `time::unix_now()`, `js-sys` en dépendance wasm, `MergeOutcome` sérialisable.
- `crates/mypass-wasm/Cargo.toml` + `src/lib.rs` (nouveaux) — tout le crate tient dans un seul fichier source (~300 lignes bindings + tests) : session, cycle de vie, bindings entries/groups/merge/outils.
- `crates/mypass-wasm/smoke.mjs` (nouveau) — smoke test Node du binaire wasm réel.
- `package.json`, `.gitignore` (modifiés) — script `build:wasm`, ignore de `crates/mypass-wasm/pkg/`.

---

### Task 1: Source de temps wasm-safe dans `mypass-core`

`SystemTime::now()` compile en wasm32 mais **panique à l'exécution**. Deux points d'appel : `xml::chrono_now()` (tous les timestamps KDBX — la fusion en dépend) et `totp.rs` (dont `totp.generate_current()` de totp-rs, qui appelle lui-même `SystemTime::now()` en interne — il faut passer à `totp.generate(now)` avec notre heure).

**Files:**
- Create: `crates/mypass-core/src/time.rs`
- Modify: `crates/mypass-core/src/lib.rs`
- Modify: `crates/mypass-core/src/xml.rs:384-392` (fonction `chrono_now`)
- Modify: `crates/mypass-core/src/totp.rs:10-19` (fonction `generate_totp_code`)
- Modify: `crates/mypass-core/Cargo.toml` (section `[target.'cfg(target_arch = "wasm32")'.dependencies]`)

**Interfaces:**
- Consumes: rien (base de tout le reste).
- Produces: `mypass_core::time::unix_now() -> u64` (secondes Unix). Les Tasks 2-5 supposent que plus AUCUN chemin de code de `mypass-core` n'appelle `SystemTime::now()` directement.

- [ ] **Step 1: Écrire le test de non-régression TOTP** (il n'existe aucun test dans `totp.rs` ; celui-ci verrouille le comportement avant le refactor)

Ajouter à la fin de `crates/mypass-core/src/totp.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_code_is_six_digits_with_valid_window() {
        let secret = generate_totp_secret().unwrap();
        let out = generate_totp_code(secret, None, None, None).unwrap();
        assert_eq!(out.code.len(), 6);
        assert!(out.code.chars().all(|c| c.is_ascii_digit()));
        assert!((1..=30).contains(&out.seconds_remaining));
    }
}
```

- [ ] **Step 2: Vérifier que le test passe sur le code actuel**

Run: `cd crates/mypass-core && cargo test totp` — Expected: `1 passed` (c'est un garde-fou de refactor, pas un test rouge : le bug wasm est un panic runtime, attrapé par le smoke test de la Task 5).

- [ ] **Step 3: Créer `time.rs` et brancher les deux appelants**

Create `crates/mypass-core/src/time.rs` :

```rust
//! Horloge wasm-safe. SystemTime::now() compile en wasm32-unknown-unknown
//! mais panique à l'exécution : sur cette cible on passe par js_sys::Date.
//! Toute heure courante du crate DOIT venir d'ici (la fusion repose sur
//! LastModificationTime — une horloge morte casse la sync silencieusement).

#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(target_arch = "wasm32")]
pub fn unix_now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}
```

Dans `crates/mypass-core/src/lib.rs`, ajouter (ordre alphabétique des modules) :

```rust
pub mod time;
```

Dans `crates/mypass-core/src/xml.rs`, remplacer le corps de `chrono_now` (lignes 385-392) par :

```rust
/// Get current timestamp in KDBX format (ISO 8601 UTC).
fn chrono_now() -> String {
    format_timestamp(crate::time::unix_now())
}
```

(supprimer le `use std::time::SystemTime;` local devenu inutile)

Dans `crates/mypass-core/src/totp.rs`, remplacer les lignes 16-18 de `generate_totp_code` :

```rust
    // totp.generate_current() appelle SystemTime::now() en interne (panic wasm)
    // → on fournit notre heure via generate(now).
    let now = crate::time::unix_now();
    let code = totp.generate(now);
    Ok(TotpCode { code, seconds_remaining: (period_val - (now % period_val)) as u32 })
```

Dans `crates/mypass-core/Cargo.toml`, ajouter à la section wasm32 existante :

```toml
# wasm32: horloge via Date.now() (SystemTime panique à l'exécution wasm)
js-sys = "0.3"
```

- [ ] **Step 4: Vérifier natif + wasm + desktop**

Run: `cd crates/mypass-core && cargo test` — Expected: tous les tests passent (dont `chrono_now_is_not_frozen_in_2024` et le test TOTP du Step 1).
Run: `cd crates/mypass-core && cargo build --target wasm32-unknown-unknown` — Expected: compile sans erreur.
Run: `grep -rn "SystemTime" crates/mypass-core/src --include="*.rs"` — Expected: uniquement dans `time.rs`.
Run: `cd src-tauri && cargo check` — Expected: compile (le desktop consomme `mypass-core` par chemin).

- [ ] **Step 5: Commit**

```bash
git add crates/mypass-core
git commit -m "feat(core): source de temps wasm-safe (time::unix_now, js_sys::Date en wasm32)"
```

---

### Task 2: Crate `mypass-wasm` — session et cycle de vie du coffre

**Files:**
- Create: `crates/mypass-wasm/Cargo.toml`
- Create: `crates/mypass-wasm/src/lib.rs`

**Interfaces:**
- Consumes: `mypass_core::reader::read_database_bytes(data: &[u8], password: &str, keyfile_data: Option<&[u8]>) -> Result<DatabaseReadResult, String>` (champs `keepass_file`, `cipher`, `kdf`), `mypass_core::writer::write_database_bytes(&KeePassFile, &str, Option<&[u8]>, Cipher, &KdfParams) -> Result<Vec<u8>, String>`, `mypass_core::xml::KeePassFile::new(name)`.
- Produces (pour Tasks 3-5 et le TS de 4c) :
  - `create_vault(name: &str, password: &str) -> Result<Vec<u8>, String>`
  - `open_vault(data: &[u8], password: &str, keyfile: Option<Vec<u8>>) -> Result<(), String>`
  - `close_vault()`
  - `is_unlocked() -> bool`
  - `save_vault() -> Result<Vec<u8>, String>`
  - Helpers internes pour Tasks 3-4 : `with_kf`, `with_kf_mut`, `json` (signatures dans le code ci-dessous).

- [ ] **Step 1: Créer le Cargo.toml**

Create `crates/mypass-wasm/Cargo.toml` :

```toml
[package]
name = "mypass-wasm"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
description = "MyPass web — bindings wasm du coffre (réutilise mypass-core)"

[lib]
# cdylib pour wasm-pack, rlib pour que `cargo test` natif fonctionne
crate-type = ["cdylib", "rlib"]

[dependencies]
mypass-core = { path = "../mypass-core" }
wasm-bindgen = "0.2"
serde = "1"
serde_json = "1"

# wasm-opt (binaryen) désactivé : téléchargement flaky sous Windows et le gain
# de taille est sans intérêt pour une PWA servie en LAN/VPN.
[package.metadata.wasm-pack.profile.release]
wasm-opt = false
```

- [ ] **Step 2: Écrire les tests du cycle de vie (rouges : rien n'existe encore)**

Create `crates/mypass-wasm/src/lib.rs` avec, pour l'instant, uniquement le module de test :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_open_save_reopen_roundtrip() {
        let bytes = create_vault("Coffre Test", "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();
        assert!(is_unlocked());
        let saved = save_vault().unwrap();
        close_vault();
        assert!(!is_unlocked());
        assert!(save_vault().is_err(), "save sans session doit échouer");
        open_vault(&saved, "s3cret", None).unwrap();
        assert!(is_unlocked());
        close_vault();
    }

    #[test]
    fn open_with_wrong_password_fails() {
        let bytes = create_vault("Coffre Test", "s3cret").unwrap();
        assert!(open_vault(&bytes, "mauvais", None).is_err());
        assert!(!is_unlocked());
    }
}
```

- [ ] **Step 3: Vérifier que ça ne compile pas**

Run: `cd crates/mypass-wasm && cargo test` — Expected: erreurs `cannot find function create_vault` etc.

- [ ] **Step 4: Implémenter la session et le cycle de vie**

Contenu de `crates/mypass-wasm/src/lib.rs` au-dessus du module de test :

```rust
//! Bindings wasm du coffre MyPass. Une seule session en mémoire (wasm est
//! mono-thread) ; le mode « web » de src/lib/tauri.ts appelle ces fonctions
//! comme il invoquerait les commandes Tauri. Entrées/sorties structurées en
//! JSON (String) — les DTOs serde de mypass_core::ops sont réutilisés tels
//! quels. Erreurs : Err(String) → exception JS via wasm-bindgen.
use mypass_core::crypto::Cipher;
use mypass_core::keys::KdfParams;
use mypass_core::xml::KeePassFile;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

/// Miroir du DbState desktop : le coffre déchiffré + ce qu'il faut pour le
/// re-chiffrer à la sauvegarde. Le mot de passe reste en mémoire de page,
/// comme le desktop le garde en mémoire de process.
struct Session {
    kf: KeePassFile,
    password: String,
    keyfile: Option<Vec<u8>>,
    cipher: Cipher,
    kdf: KdfParams,
}

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
}

fn with_kf<T>(f: impl FnOnce(&KeePassFile) -> Result<T, String>) -> Result<T, String> {
    SESSION.with(|s| {
        let s = s.borrow();
        let session = s.as_ref().ok_or("No open vault")?;
        f(&session.kf)
    })
}

fn with_kf_mut<T>(f: impl FnOnce(&mut KeePassFile) -> Result<T, String>) -> Result<T, String> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let session = s.as_mut().ok_or("No open vault")?;
        f(&mut session.kf)
    })
}

fn json<T: serde::Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

/// Crée un coffre vierge et retourne ses octets KDBX (mêmes défauts que la
/// commande desktop create_database : AES-256, KdfParams::default()).
#[wasm_bindgen]
pub fn create_vault(name: &str, password: &str) -> Result<Vec<u8>, String> {
    let kf = KeePassFile::new(name);
    mypass_core::writer::write_database_bytes(&kf, password, None, Cipher::Aes256, &KdfParams::default())
}

#[wasm_bindgen]
pub fn open_vault(data: &[u8], password: &str, keyfile: Option<Vec<u8>>) -> Result<(), String> {
    let result = mypass_core::reader::read_database_bytes(data, password, keyfile.as_deref())?;
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            kf: result.keepass_file,
            password: password.to_string(),
            keyfile,
            cipher: result.cipher,
            kdf: result.kdf,
        });
    });
    Ok(())
}

#[wasm_bindgen]
pub fn close_vault() {
    SESSION.with(|s| *s.borrow_mut() = None);
}

#[wasm_bindgen]
pub fn is_unlocked() -> bool {
    SESSION.with(|s| s.borrow().is_some())
}

/// Re-chiffre l'état courant et retourne les octets KDBX (le JS fait le PUT).
#[wasm_bindgen]
pub fn save_vault() -> Result<Vec<u8>, String> {
    SESSION.with(|s| {
        let s = s.borrow();
        let session = s.as_ref().ok_or("No open vault")?;
        mypass_core::writer::write_database_bytes(
            &session.kf,
            &session.password,
            session.keyfile.as_deref(),
            session.cipher,
            &session.kdf,
        )
    })
}
```

Note pour l'implémenteur : `with_kf`, `with_kf_mut` et `json` déclencheront un warning `dead_code` dans cette task (leurs consommateurs arrivent en Task 3). Poser `#[allow(dead_code)]` sur ces trois fonctions avec le commentaire `// utilisés par les bindings entries/groups (task suivante)` — à retirer en Task 3.

- [ ] **Step 5: Vérifier natif + wasm**

Run: `cd crates/mypass-wasm && cargo test` — Expected: `2 passed` (les tests font 6 dérivations Argon2 à 64 Mio, compter ~5-10 s).
Run: `cd crates/mypass-wasm && cargo build --target wasm32-unknown-unknown` — Expected: compile.

- [ ] **Step 6: Commit**

```bash
git add crates/mypass-wasm
git commit -m "feat(wasm): crate mypass-wasm — session et cycle de vie du coffre"
```

---

### Task 3: Bindings entrées et groupes (JSON in/out)

**Files:**
- Modify: `crates/mypass-wasm/src/lib.rs`

**Interfaces:**
- Consumes: `with_kf`/`with_kf_mut`/`json` (Task 2) ; `mypass_core::ops::entries::{list, get, create, update, delete, duplicate}` et `mypass_core::ops::groups::{list, create, update, delete, move_entry}` — signatures exactes dans le code ci-dessous ; DTOs `NewEntry`/`UpdateEntry` (serde camelCase, tous champs optionnels sauf title/username/password de NewEntry).
- Produces (exports wasm, JSON en camelCase comme l'IPC Tauri) :
  - `get_entries(group_uuid: Option<String>) -> Result<String, String>` — JSON `EntryInfo[]`
  - `get_entry(uuid: &str) -> Result<String, String>` — JSON `EntryInfo`
  - `create_entry(new_json: &str) -> Result<String, String>` — arg JSON `NewEntry`, retour `EntryInfo`
  - `update_entry(uuid: &str, update_json: &str) -> Result<String, String>` — arg JSON `UpdateEntry`, retour `EntryInfo`
  - `delete_entry(uuid: &str) -> Result<(), String>`
  - `duplicate_entry(uuid: &str) -> Result<String, String>` — retour `EntryInfo`
  - `get_groups() -> Result<String, String>` — JSON `GroupInfo[]` (arbre)
  - `create_group(name: &str, parent_uuid: Option<String>) -> Result<String, String>` — retour `GroupInfo`
  - `update_group(uuid: &str, name: Option<String>, icon_id: Option<String>, is_expanded: Option<bool>) -> Result<String, String>` — retour `GroupInfo`
  - `delete_group(uuid: &str) -> Result<(), String>`
  - `move_entry(entry_uuid: &str, group_uuid: &str) -> Result<(), String>`

- [ ] **Step 1: Écrire les tests (rouges)**

Ajouter au module `tests` de `crates/mypass-wasm/src/lib.rs` (les tests tournent chacun sur leur thread → sessions `thread_local` isolées) :

```rust
    fn open_fresh(name: &str) {
        let bytes = create_vault(name, "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();
    }

    #[test]
    fn entry_crud_via_json_bindings() {
        open_fresh("Coffre Entrées");

        let created: serde_json::Value = serde_json::from_str(
            &create_entry(r#"{"title":"Site","username":"luca","password":"p4ss","url":"https://x.io"}"#).unwrap(),
        )
        .unwrap();
        let uuid = created["uuid"].as_str().unwrap().to_string();

        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        update_entry(&uuid, r#"{"username":"luca2"}"#).unwrap();
        let got: serde_json::Value =
            serde_json::from_str(&get_entry(&uuid).unwrap()).unwrap();
        assert_eq!(got["username"], "luca2");
        assert_eq!(got["password"], "p4ss");

        let dup: serde_json::Value =
            serde_json::from_str(&duplicate_entry(&uuid).unwrap()).unwrap();
        assert_eq!(dup["title"], "Site (copy)");

        delete_entry(&uuid).unwrap();
        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1); // il reste le duplicata

        close_vault();
    }

    #[test]
    fn group_bindings_roundtrip() {
        open_fresh("Coffre Groupes");

        let g: serde_json::Value =
            serde_json::from_str(&create_group("Travail", None).unwrap()).unwrap();
        let guid = g["uuid"].as_str().unwrap().to_string();

        let renamed: serde_json::Value = serde_json::from_str(
            &update_group(&guid, Some("Perso".to_string()), None, None).unwrap(),
        )
        .unwrap();
        assert_eq!(renamed["name"], "Perso");

        // Entrée dans le groupe puis move vers la racine
        let e: serde_json::Value = serde_json::from_str(
            &create_entry(&format!(
                r#"{{"title":"Dans groupe","username":"u","password":"p","groupUuid":"{guid}"}}"#
            ))
            .unwrap(),
        )
        .unwrap();
        let tree: serde_json::Value =
            serde_json::from_str(&get_groups().unwrap()).unwrap();
        let root_uuid = tree[0]["uuid"].as_str().unwrap().to_string();
        move_entry(e["uuid"].as_str().unwrap(), &root_uuid).unwrap();

        delete_group(&guid).unwrap();
        let tree: serde_json::Value =
            serde_json::from_str(&get_groups().unwrap()).unwrap();
        assert_eq!(tree[0]["children"].as_array().unwrap().len(), 0);
        // L'entrée déplacée a survécu à la suppression du groupe
        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        close_vault();
    }

    #[test]
    fn entry_ops_without_session_fail() {
        assert!(get_entries(None).is_err());
        assert!(create_entry(r#"{"title":"t","username":"u","password":"p"}"#).is_err());
    }
```

- [ ] **Step 2: Vérifier qu'ils échouent**

Run: `cd crates/mypass-wasm && cargo test` — Expected: erreurs de compilation `cannot find function get_entries` etc.

- [ ] **Step 3: Implémenter les bindings**

Ajouter dans `crates/mypass-wasm/src/lib.rs` (et retirer les `#[allow(dead_code)]` posés en Task 2) :

```rust
// ============================================================================
// Entrées
// ============================================================================

#[wasm_bindgen]
pub fn get_entries(group_uuid: Option<String>) -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::entries::list(kf, group_uuid.as_deref())?))
}

#[wasm_bindgen]
pub fn get_entry(uuid: &str) -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::entries::get(kf, uuid)?))
}

#[wasm_bindgen]
pub fn create_entry(new_json: &str) -> Result<String, String> {
    let new: mypass_core::ops::entries::NewEntry =
        serde_json::from_str(new_json).map_err(|e| e.to_string())?;
    with_kf_mut(|kf| json(&mypass_core::ops::entries::create(kf, new)?))
}

#[wasm_bindgen]
pub fn update_entry(uuid: &str, update_json: &str) -> Result<String, String> {
    let update: mypass_core::ops::entries::UpdateEntry =
        serde_json::from_str(update_json).map_err(|e| e.to_string())?;
    with_kf_mut(|kf| json(&mypass_core::ops::entries::update(kf, uuid, update)?))
}

#[wasm_bindgen]
pub fn delete_entry(uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::entries::delete(kf, uuid))
}

#[wasm_bindgen]
pub fn duplicate_entry(uuid: &str) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::entries::duplicate(kf, uuid)?))
}

// ============================================================================
// Groupes
// ============================================================================

#[wasm_bindgen]
pub fn get_groups() -> Result<String, String> {
    with_kf(|kf| json(&mypass_core::ops::groups::list(kf)))
}

#[wasm_bindgen]
pub fn create_group(name: &str, parent_uuid: Option<String>) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::groups::create(kf, name, parent_uuid.as_deref())?))
}

#[wasm_bindgen]
pub fn update_group(
    uuid: &str,
    name: Option<String>,
    icon_id: Option<String>,
    is_expanded: Option<bool>,
) -> Result<String, String> {
    with_kf_mut(|kf| json(&mypass_core::ops::groups::update(kf, uuid, name, icon_id, is_expanded)?))
}

#[wasm_bindgen]
pub fn delete_group(uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::groups::delete(kf, uuid))
}

#[wasm_bindgen]
pub fn move_entry(entry_uuid: &str, group_uuid: &str) -> Result<(), String> {
    with_kf_mut(|kf| mypass_core::ops::groups::move_entry(kf, entry_uuid, group_uuid))
}
```

- [ ] **Step 4: Vérifier natif + wasm**

Run: `cd crates/mypass-wasm && cargo test` — Expected: `5 passed`.
Run: `cd crates/mypass-wasm && cargo build --target wasm32-unknown-unknown` — Expected: compile.

- [ ] **Step 5: Commit**

```bash
git add crates/mypass-wasm
git commit -m "feat(wasm): bindings entrées et groupes (JSON in/out sur mypass_core::ops)"
```

---

### Task 4: Bindings fusion, générateur et TOTP

**Files:**
- Modify: `crates/mypass-core/src/merge.rs:10` (derive `Serialize` sur `MergeOutcome`)
- Modify: `crates/mypass-wasm/src/lib.rs`

**Interfaces:**
- Consumes: `mypass_core::merge::merge(&mut KeePassFile, &KeePassFile) -> MergeOutcome` (+ `outcome.changed() -> bool`), `mypass_core::generator::{generate_password(PasswordConfig), generate_passphrase(PassphraseConfig), evaluate_strength(String)}`, `mypass_core::totp::{generate_totp_code(String, Option<String>, Option<usize>, Option<u32>), generate_totp_secret()}`.
- Produces (exports wasm) :
  - `merge_remote(remote: &[u8]) -> Result<String, String>` — déchiffre les octets distants avec les credentials de la session, fusionne dans la session, retourne JSON `{entriesAdded, entriesUpdated, entriesDeleted, groupsAdded, changed}` (camelCase). Le TS de 4c s'en sert pour rafraîchir l'UI après pull ; la décision de PUT vient d'un dirty flag côté TS sur les mutations locales (fusion inverse comme sync.rs desktop), PAS de ce `changed`.
  - `generate_password(config_json: &str) -> Result<String, String>` — arg JSON `PasswordConfig`
  - `generate_passphrase(config_json: &str) -> Result<String, String>` — arg JSON `PassphraseConfig`
  - `evaluate_strength(password: &str) -> Result<String, String>` — JSON `StrengthResult`
  - `generate_totp_code(secret: &str, algorithm: Option<String>, digits: Option<u32>, period: Option<u32>) -> Result<String, String>` — JSON `TotpCode`
  - `generate_totp_secret() -> Result<String, String>`

- [ ] **Step 1: Écrire les tests (rouges)**

Ajouter au module `tests` de `crates/mypass-wasm/src/lib.rs` :

```rust
    #[test]
    fn merge_remote_adds_remote_entry() {
        let bytes = create_vault("Coffre Merge", "s3cret").unwrap();
        open_vault(&bytes, "s3cret", None).unwrap();

        // Distant = même lignée (relit les mêmes octets → même uuid racine),
        // avec une entrée en plus.
        let mut remote =
            mypass_core::reader::read_database_bytes(&bytes, "s3cret", None).unwrap();
        remote
            .keepass_file
            .root
            .group
            .entries
            .push(mypass_core::xml::Entry::new("Distant", "u", "p", ""));
        let remote_bytes = mypass_core::writer::write_database_bytes(
            &remote.keepass_file,
            "s3cret",
            None,
            remote.cipher,
            &remote.kdf,
        )
        .unwrap();

        let outcome: serde_json::Value =
            serde_json::from_str(&merge_remote(&remote_bytes).unwrap()).unwrap();
        assert_eq!(outcome["entriesAdded"], 1);
        assert_eq!(outcome["changed"], true);

        let listed: serde_json::Value =
            serde_json::from_str(&get_entries(None).unwrap()).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);

        // Re-fusionner le même distant = no-op
        let outcome: serde_json::Value =
            serde_json::from_str(&merge_remote(&remote_bytes).unwrap()).unwrap();
        assert_eq!(outcome["changed"], false);

        close_vault();
    }

    #[test]
    fn tools_bindings_work() {
        let pw = generate_password(
            r#"{"length":16,"uppercase":true,"lowercase":true,"digits":true,"symbols":false}"#,
        )
        .unwrap();
        assert_eq!(pw.len(), 16);

        let phrase = generate_passphrase(
            r#"{"word_count":4,"separator":"-","word_case":"lower"}"#,
        )
        .unwrap();
        assert_eq!(phrase.split('-').count(), 4);

        let strength: serde_json::Value =
            serde_json::from_str(&evaluate_strength(&pw).unwrap()).unwrap();
        assert!(strength["score"].is_number());

        let secret = generate_totp_secret().unwrap();
        let code: serde_json::Value =
            serde_json::from_str(&generate_totp_code(&secret, None, None, None).unwrap())
                .unwrap();
        assert_eq!(code["code"].as_str().unwrap().len(), 6);
    }
```

Note : `PasswordConfig`/`PassphraseConfig` sont en snake_case côté serde (pas de `rename_all`) — le JSON de test reflète ça ; c'est aussi ce que le TS enverra en 4c (identique aux args des commandes Tauri actuelles).

- [ ] **Step 2: Vérifier qu'ils échouent**

Run: `cd crates/mypass-wasm && cargo test` — Expected: erreurs `cannot find function merge_remote` etc.

- [ ] **Step 3: Rendre `MergeOutcome` sérialisable**

Dans `crates/mypass-core/src/merge.rs`, ligne 10, remplacer :

```rust
#[derive(Debug, Default, PartialEq)]
pub struct MergeOutcome {
```

par :

```rust
#[derive(Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutcome {
```

(Le seul consommateur Serialize est le JSON wasm ; `commands/sync.rs` côté desktop n'utilise que les champs Rust — vérifier avec `cd src-tauri && cargo check`.)

- [ ] **Step 4: Implémenter les bindings**

Ajouter dans `crates/mypass-wasm/src/lib.rs` :

```rust
// ============================================================================
// Fusion. Boucle de sync 4c : GET → merge_remote (`changed` = le distant
// avait du neuf → rafraîchir l'UI et sauvegarder localement). La décision de
// PUT ne vient PAS de ce `changed` : comme sync.rs côté desktop (fusion
// inverse), il faut pousser dès que la session a des mutations locales que le
// serveur n'a pas — le TS de 4c trace un dirty flag sur ses mutations (ou un
// binding needs_push sera ajouté en 4c si nécessaire).
// ============================================================================

#[wasm_bindgen]
pub fn merge_remote(remote: &[u8]) -> Result<String, String> {
    SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let session = s.as_mut().ok_or("No open vault")?;
        let remote = mypass_core::reader::read_database_bytes(
            remote,
            &session.password,
            session.keyfile.as_deref(),
        )?;
        let outcome = mypass_core::merge::merge(&mut session.kf, &remote.keepass_file);
        let mut v = serde_json::to_value(&outcome).map_err(|e| e.to_string())?;
        v["changed"] = serde_json::Value::Bool(outcome.changed());
        serde_json::to_string(&v).map_err(|e| e.to_string())
    })
}

// ============================================================================
// Générateur / TOTP (sans session : purs)
// ============================================================================

#[wasm_bindgen]
pub fn generate_password(config_json: &str) -> Result<String, String> {
    let config = serde_json::from_str(config_json).map_err(|e| e.to_string())?;
    mypass_core::generator::generate_password(config)
}

#[wasm_bindgen]
pub fn generate_passphrase(config_json: &str) -> Result<String, String> {
    let config = serde_json::from_str(config_json).map_err(|e| e.to_string())?;
    mypass_core::generator::generate_passphrase(config)
}

#[wasm_bindgen]
pub fn evaluate_strength(password: &str) -> Result<String, String> {
    json(&mypass_core::generator::evaluate_strength(password.to_string())?)
}

#[wasm_bindgen]
pub fn generate_totp_code(
    secret: &str,
    algorithm: Option<String>,
    digits: Option<u32>,
    period: Option<u32>,
) -> Result<String, String> {
    json(&mypass_core::totp::generate_totp_code(
        secret.to_string(),
        algorithm,
        digits.map(|d| d as usize),
        period,
    )?)
}

#[wasm_bindgen]
pub fn generate_totp_secret() -> Result<String, String> {
    mypass_core::totp::generate_totp_secret()
}
```

- [ ] **Step 5: Vérifier natif + wasm + desktop**

Run: `cd crates/mypass-wasm && cargo test` — Expected: `7 passed`.
Run: `cd crates/mypass-core && cargo test` — Expected: tous passent (merge.rs modifié).
Run: `cd crates/mypass-wasm && cargo build --target wasm32-unknown-unknown` — Expected: compile.
Run: `cd src-tauri && cargo check` — Expected: compile.

- [ ] **Step 6: Commit**

```bash
git add crates/mypass-core/src/merge.rs crates/mypass-wasm
git commit -m "feat(wasm): bindings fusion, générateur et TOTP"
```

---

### Task 5: Build wasm-pack + smoke test Node (preuve runtime)

C'est LA vérification que le prérequis temps (Task 1) et le RNG (`getrandom` js) fonctionnent **à l'exécution** dans un environnement JS — la compilation seule ne le prouve pas.

**Files:**
- Modify: `package.json` (devDependency `wasm-pack`, scripts `build:wasm` et `smoke:wasm`)
- Modify: `.gitignore` (ajouter `crates/mypass-wasm/pkg/`)
- Create: `crates/mypass-wasm/smoke.mjs`

**Interfaces:**
- Consumes: tous les exports des Tasks 2-4, via le paquet généré `crates/mypass-wasm/pkg/mypass_wasm.js` (+ `mypass_wasm_bg.wasm`).
- Produces: script npm `build:wasm` (cible `web` — celle que Vite consommera en 4c) et `smoke:wasm` ; artefact `pkg/` non versionné.

- [ ] **Step 1: Installer wasm-pack et déclarer les scripts**

Run: `npm install -D wasm-pack` — Expected: paquet installé (binaire précompilé, pas de build Rust).

Dans `package.json`, ajouter aux `scripts` :

```json
    "build:wasm": "wasm-pack build crates/mypass-wasm --target web",
    "smoke:wasm": "node crates/mypass-wasm/smoke.mjs"
```

Dans `.gitignore`, ajouter :

```
crates/mypass-wasm/pkg/
```

- [ ] **Step 2: Builder le paquet wasm**

Run: `npm run build:wasm` — Expected: `pkg/` contient `mypass_wasm.js`, `mypass_wasm_bg.wasm`, `mypass_wasm.d.ts`. (Premier run : wasm-pack télécharge wasm-bindgen-cli, compter 1-3 min.)

- [ ] **Step 3: Écrire le smoke test**

Create `crates/mypass-wasm/smoke.mjs` :

```js
// Smoke test runtime du wasm dans Node (mêmes APIs que le navigateur :
// Date.now, crypto.getRandomValues). Prouve que l'horloge js et le RNG
// fonctionnent à l'exécution — la compilation wasm32 seule ne le garantit pas.
// Usage : npm run build:wasm && npm run smoke:wasm
import { readFileSync } from "node:fs";
import init, {
  create_vault, open_vault, close_vault, is_unlocked, save_vault,
  create_entry, get_entries, merge_remote,
  generate_totp_secret, generate_totp_code, generate_password,
} from "./pkg/mypass_wasm.js";

await init({ module_or_path: readFileSync(new URL("./pkg/mypass_wasm_bg.wasm", import.meta.url)) });

let failed = false;
const check = (cond, msg) => {
  console.log(`${cond ? "ok  " : "FAIL"} - ${msg}`);
  if (!cond) failed = true;
};

// Cycle de vie + horloge vivante (LMT doit être l'heure réelle, pas 1970)
const bytes = create_vault("Smoke", "s3cret");
open_vault(bytes, "s3cret", undefined);
check(is_unlocked(), "coffre créé et ouvert");

const entry = JSON.parse(
  create_entry(JSON.stringify({ title: "Site", username: "luca", password: "p4ss" })),
);
check(entry.modified > "2026", `horloge js vivante (modified=${entry.modified})`);

// Save → reopen
const saved = save_vault();
close_vault();
open_vault(saved, "s3cret", undefined);
check(JSON.parse(get_entries(null)).length === 1, "entrée persistée après save/reopen");

// Fusion no-op avec soi-même
const outcome = JSON.parse(merge_remote(saved));
check(outcome.changed === false, "fusion no-op avec sa propre sauvegarde");

// Outils (RNG + horloge TOTP)
const code = JSON.parse(generate_totp_code(generate_totp_secret(), undefined, undefined, undefined));
check(/^\d{6}$/.test(code.code), `code TOTP généré (${code.code})`);
check(
  generate_password(JSON.stringify({ length: 20, uppercase: true, lowercase: true, digits: true, symbols: true })).length === 20,
  "générateur de mot de passe",
);

close_vault();
if (failed) { console.error("SMOKE FAILED"); process.exit(1); }
console.log("SMOKE OK");
```

- [ ] **Step 4: Lancer le smoke test**

Run: `npm run smoke:wasm` — Expected: toutes les lignes `ok  -`, puis `SMOKE OK`, exit 0. (Le déverrouillage Argon2 64 Mio en wasm prend ~1-2 s par appel : quelques secondes au total, c'est attendu — cf. spec.)

- [ ] **Step 5: Vérification finale complète**

Run: `cd crates/mypass-core && cargo test` — Expected: tous passent.
Run: `cd crates/mypass-wasm && cargo test` — Expected: `7 passed`.
Run: `cd src-tauri && cargo test` — Expected: tous passent (aucune régression desktop).

- [ ] **Step 6: Commit**

```bash
git add package.json package-lock.json .gitignore crates/mypass-wasm/smoke.mjs
git commit -m "feat(wasm): build wasm-pack (cible web) + smoke test Node runtime"
```

---

## Hors scope (4c/4d)

- Mode « web » dans `src/lib/tauri.ts` (branchement des commandes sur ces exports + GET/PUT sync) → 4c.
- Le serveur qui sert `pkg/` + le build web → 4c. PWA (manifest, service worker) → 4d.
- Verrouillage auto en arrière-plan d'onglet → 4c (logique UI, rien à faire côté wasm).
