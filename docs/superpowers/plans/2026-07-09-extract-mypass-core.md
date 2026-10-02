# Extraction `mypass-core` — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extraire `src-tauri/src/kdbx/` dans un crate partagé `crates/mypass-core` compilable en natif ET en `wasm32-unknown-unknown`, sans aucun changement de comportement (tous les tests Rust existants passent à l'identique).

**Architecture:** Nouveau crate lib `crates/mypass-core` à la racine du repo (pas de workspace Cargo — simple dépendance par chemin). Les 5 modules (`crypto`, `keys`, `reader`, `writer`, `xml`) sont déplacés tels quels ; `src-tauri` les ré-importe via `pub use mypass_core as kdbx;` dans `lib.rs`, ce qui garde tous les chemins `crate::kdbx::...` valides — zéro modification des 10+ fichiers consommateurs (`commands/*`, `native_messaging.rs`, `ssh/agent.rs`).

**Tech Stack:** Rust 2021 (rust-version 1.85), cargo. Crypto pure-Rust déjà compatible wasm (aes-gcm, chacha20poly1305, argon2, sha2). Seul ajout wasm : `getrandom` avec la feature `js`.

## Global Constraints

- Aucun changement de comportement : mêmes signatures publiques, mêmes messages d'erreur, mêmes tests.
- Pas de workspace Cargo : `mypass-core` est un crate autonome référencé par chemin (`path = "../crates/mypass-core"`). Le futur `server/` fera pareil.
- Nom du crate : `mypass-core` (import Rust : `mypass_core`), version `0.1.0`, `edition = "2021"`, `rust-version = "1.85"` (identique à `src-tauri/Cargo.toml`).
- Sur cette machine, cargo est dans `C:\Users\Utilisateur\.cargo\bin` (déjà dans le PATH des shells normaux).
- Les commandes cargo pour `src-tauri` se lancent depuis `src-tauri/` ; celles de `mypass-core` depuis `crates/mypass-core/`.

**Note pour le sous-projet 3 (hors scope ici) :** `xml.rs::chrono_now()` (ligne ~371) est buggé — il retourne toujours la date `2024-01-01` avec seulement l'heure réelle. La fusion KDBX du sous-projet 3 repose sur `LastModificationTime` : ce bug DEVRA être corrigé à ce moment-là. Ne pas le corriger ici (changement de comportement).

---

### Task 1: Créer `crates/mypass-core` et rebrancher `src-tauri` dessus

Un seul task atomique : déplacer les fichiers ET rebrancher l'app dans le même commit, sinon `src-tauri` ne compile plus entre les deux.

**Files:**
- Create: `crates/mypass-core/Cargo.toml`
- Create: `crates/mypass-core/src/lib.rs`
- Move: `src-tauri/src/kdbx/{crypto,keys,reader,writer,xml}.rs` → `crates/mypass-core/src/`
- Delete: `src-tauri/src/kdbx/mod.rs`
- Modify: `src-tauri/Cargo.toml` (ajout dépendance)
- Modify: `src-tauri/src/lib.rs:5` (`pub mod kdbx;` → ré-export)
- Modify: `.gitignore` (racine du repo)
- Test: les `#[cfg(test)]` existants dans `crypto.rs` et `keys.rs` (déplacés avec les fichiers) + tests de `src-tauri` (`commands/database.rs`, etc.)

**Interfaces:**
- Consumes: rien (premier task).
- Produces: crate `mypass-core` exposant `mypass_core::{crypto, keys, reader, writer, xml}` avec exactement les API actuelles, notamment `reader::read_database(&Path, &str, Option<&[u8]>) -> Result<DatabaseReadResult, String>`, `writer::write_database(&Path, &KeePassFile, &str, Option<&[u8]>, Cipher, &KdfParams) -> Result<(), String>`, `keys::{KdfParams, derive_composite_key_with_salt, generate_salt}`, `crypto::{Cipher, generate_random_key}`, `xml::{KeePassFile, Group, Entry, EntryString, Value, CustomDataItem, ...}`. Dans `src-tauri`, `crate::kdbx::...` reste le chemin d'accès (alias).

- [ ] **Step 1: Enregistrer la baseline de tests**

```powershell
cd src-tauri
cargo test 2>&1 | Select-String "test result"
```

Noter le nombre exact de tests qui passent (ex. `X passed; 0 failed` par binaire de test). C'est la référence de non-régression.

- [ ] **Step 2: Créer la structure du crate et déplacer les modules**

```powershell
mkdir crates/mypass-core/src
git mv src-tauri/src/kdbx/crypto.rs crates/mypass-core/src/crypto.rs
git mv src-tauri/src/kdbx/keys.rs   crates/mypass-core/src/keys.rs
git mv src-tauri/src/kdbx/reader.rs crates/mypass-core/src/reader.rs
git mv src-tauri/src/kdbx/writer.rs crates/mypass-core/src/writer.rs
git mv src-tauri/src/kdbx/xml.rs    crates/mypass-core/src/xml.rs
git rm src-tauri/src/kdbx/mod.rs
```

Aucune modification du contenu des 5 fichiers déplacés : les `use super::crypto` etc. dans `reader.rs`/`writer.rs` restent valides (les modules deviennent frères à la racine du crate, `super` = racine du crate, comme avant sous `kdbx/mod.rs`).

- [ ] **Step 3: Écrire `crates/mypass-core/Cargo.toml`**

```toml
[package]
name = "mypass-core"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
description = "MyPass core — KDBX4 format (crypto, KDF, reader/writer, XML)"

[dependencies]
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4"] }
aes-gcm = "0.10"
chacha20poly1305 = "0.10"
argon2 = "0.5"
sha2 = "0.10"
rand = "0.8"
quick-xml = { version = "0.37", features = ["serialize"] }
```

(Versions copiées de `src-tauri/Cargo.toml` — ne pas les changer.)

- [ ] **Step 4: Écrire `crates/mypass-core/src/lib.rs`**

```rust
pub mod crypto;
pub mod keys;
pub mod reader;
pub mod writer;
pub mod xml;
```

- [ ] **Step 5: Vérifier que les tests du crate passent**

```powershell
cd crates/mypass-core
cargo test
```

Expected: PASS — les tests de `crypto::tests` et `keys::tests` (déplacés avec leurs fichiers) passent, même nombre qu'en baseline pour ces deux modules.

- [ ] **Step 6: Rebrancher `src-tauri`**

Dans `src-tauri/Cargo.toml`, section `[dependencies]`, ajouter :

```toml
mypass-core = { path = "../crates/mypass-core" }
```

Dans `src-tauri/src/lib.rs`, remplacer la ligne 5 :

```rust
pub mod kdbx;
```

par :

```rust
pub use mypass_core as kdbx;
```

C'est TOUT : tous les `crate::kdbx::xml::Entry`, `kdbx::reader::read_database`, etc. des fichiers consommateurs résolvent à travers l'alias sans être modifiés.

- [ ] **Step 7: Vérifier la non-régression complète**

```powershell
cd src-tauri
cargo test
cargo clippy
```

Expected: `cargo test` = exactement la même baseline qu'au Step 1 (les tests de `crypto`/`keys` sont maintenant comptés dans `mypass-core`, ceux de `commands/database.rs`/`browser.rs`/`ssh` toujours dans `mypass`). `cargo clippy` : zéro nouveau warning.

- [ ] **Step 8: Ignorer les artefacts de build du nouveau crate**

Ajouter à la fin du `.gitignore` racine :

```
crates/mypass-core/target/
crates/mypass-core/Cargo.lock
```

- [ ] **Step 9: Commit**

```powershell
git add -A
git status   # vérifier : uniquement les moves, Cargo.toml x2, lib.rs x2, .gitignore — PAS de target/ ni de .log
git commit -m "refactor: extract kdbx into shared mypass-core crate

No behavior change. src-tauri re-exports it as kdbx so all
crate::kdbx::* paths keep working unchanged.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Compilation `wasm32-unknown-unknown`

**Files:**
- Modify: `crates/mypass-core/Cargo.toml` (dépendance getrandom ciblée wasm)

**Interfaces:**
- Consumes: le crate `mypass-core` du Task 1.
- Produces: `mypass-core` compile en `wasm32-unknown-unknown` (prérequis du sous-projet 4 — app web). Aucune API nouvelle.

- [ ] **Step 1: Installer la cible wasm32**

```powershell
rustup target add wasm32-unknown-unknown
```

- [ ] **Step 2: Constater l'échec actuel (équivalent du test rouge)**

```powershell
cd crates/mypass-core
cargo check --target wasm32-unknown-unknown
```

Expected: FAIL — erreur `getrandom` : `the wasm32-unknown-unknown targets are not supported by default` (rand/uuid/argon2 tirent getrandom 0.2, qui exige la feature `js` sur cette cible).

Si contre toute attente ça passe déjà : tant mieux, sauter le Step 3 et aller au Step 4.

- [ ] **Step 3: Activer getrandom/js pour la cible wasm**

Ajouter à `crates/mypass-core/Cargo.toml` :

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom = { version = "0.2", features = ["js"] }
```

- [ ] **Step 4: Vérifier que les deux cibles compilent et que rien n'a régressé**

```powershell
cd crates/mypass-core
cargo check --target wasm32-unknown-unknown
cargo test
```

Expected: check wasm = PASS (warnings tolérés, zéro erreur) ; `cargo test` natif = même résultat qu'avant.

Notes connues, acceptées pour ce sous-projet (elles compilent, elles ne cassent rien à l'exécution native ; le sous-projet 4 introduira des API sur octets et une abstraction du temps quand l'app web en aura besoin) :
- `reader::read_database`/`writer::write_database` utilisent `std::fs` → compilent en wasm mais inutilisables à l'exécution dans un navigateur.
- `xml.rs::chrono_now()` utilise `SystemTime::now()` → panique à l'exécution en wasm.

- [ ] **Step 5: Commit**

```powershell
git add crates/mypass-core/Cargo.toml
git commit -m "feat(core): support wasm32-unknown-unknown compilation

getrandom js feature on wasm32 target. File-based and clock APIs
still native-only at runtime; byte-based APIs come with the web app.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Vérification finale du sous-projet

Depuis la racine :

```powershell
cd crates/mypass-core && cargo test && cargo check --target wasm32-unknown-unknown
cd ../../src-tauri && cargo test && cargo clippy
```

Critère de sortie (= critère du spec, section « Découpage », point 1) : tests identiques à la baseline, clippy propre, wasm32 compile. Optionnel mais recommandé : lancer `npm run tauri dev` et ouvrir un coffre existant pour confirmer à la main que rien n'a bougé.
