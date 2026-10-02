# Extraction des opérations de coffre (sous-projet 4a) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extraire la logique PURE des commandes (générateur, TOTP, force, opérations CRUD sur `KeePassFile`) de `src-tauri/src/commands/` vers `crates/mypass-core`, pour qu'elle soit réutilisable par le futur crate WASM (sous-projet 4b) ET reste utilisée par les commandes Tauri desktop — une seule implémentation, zéro duplication.

**Architecture:** Nouveau sous-arbre `crates/mypass-core/src/ops/` (`entries.rs`, `groups.rs`) contenant les DTO (`EntryInfo`, `NewEntry`, `UpdateEntry`, `GroupInfo`) et des fonctions pures prenant `&KeePassFile`/`&mut KeePassFile`. Les modules purs `generator`, `totp`, `strength` remontent aussi dans `mypass-core`. Les `#[tauri::command]` de `src-tauri` deviennent des wrappers minces : lock `DbState` → appel `mypass_core::ops::...` → `db.save()` → retour. Aucun changement de comportement ; les tests existants (desktop) le prouvent.

**Tech Stack:** Rust. `mypass-core` gagne les dépendances des modules déplacés : `totp-rs`, `zxcvbn`(si utilisé par strength), `zeroize` si nécessaire — copiées verbatim depuis `src-tauri/Cargo.toml`. Doit continuer à compiler en `wasm32-unknown-unknown`.

## Global Constraints

- Périmètre STRICT : entrées, groupes, générateur, TOTP, force. **Hors scope, NON déplacés** : passkeys, import/export, dedup, hibp, browser, ssh (l'app web ne les utilise pas au départ — YAGNI). Ne pas y toucher.
- Aucun changement de comportement : mêmes DTO (sérialisation camelCase identique), mêmes messages d'erreur, mêmes tests desktop passants.
- Signatures des `#[tauri::command]` INCHANGÉES (le frontend desktop et `tauri.ts` ne bougent pas).
- `mypass-core` doit rester compilable en `wasm32-unknown-unknown` après chaque tâche (`cargo check --target wasm32-unknown-unknown`). Si un module déplacé tire une dépendance incompatible wasm, l'isoler derrière `#[cfg(not(target_arch = "wasm32"))]` et le NOTER — mais générateur/totp/strength sont du calcul pur et doivent passer.
- Nom des nouveaux modules : `mypass_core::ops::{entries, groups}`, `mypass_core::{generator, totp, strength}`.
- Les fonctions pures d'opération prennent le `KeePassFile` en argument et NE font PAS d'I/O (pas de `db.save()` — c'est le wrapper Tauri qui sauve). Le `touch()`/tombstone déjà en place reste dans la fonction pure (c'est de la logique de données, pas de l'I/O).
- Non-régression finale : `cargo test` (mypass-core + src-tauri), `cargo clippy` sans nouveau warning, `cargo check --target wasm32-unknown-unknown`, `npm run build`.

---

### Task 1: Déplacer générateur + TOTP + force dans mypass-core

Ces 3 modules sont déjà purs (aucun `State`/`DbState`). Déplacement simple + wrappers Tauri.

**Files:**
- Move: `src-tauri/src/commands/generator.rs` → `crates/mypass-core/src/generator.rs`
- Move: `src-tauri/src/commands/totp.rs` → `crates/mypass-core/src/totp.rs`
- Locate & move the strength logic: si `evaluate_strength` vit dans `generator.rs` il part avec ; s'il délègue à `src-tauri/src/security/zxcvbn.rs`, déplacer aussi ce module en `crates/mypass-core/src/strength.rs` (vérifier `use` dans generator.rs d'abord).
- Modify: `crates/mypass-core/src/lib.rs` (déclarer les modules)
- Modify: `crates/mypass-core/Cargo.toml` (deps `totp-rs`, `zxcvbn` si besoin)
- Create: `src-tauri/src/commands/generator.rs` + `totp.rs` NOUVEAUX, réduits aux `#[tauri::command]` qui délèguent.
- Modify: `src-tauri/Cargo.toml` (retirer les deps devenues inutilisées côté src-tauri UNIQUEMENT si plus aucun autre fichier ne les utilise — vérifier par grep avant de retirer)

**Interfaces:**
- Consumes: rien.
- Produces: `mypass_core::generator::{generate_password(PasswordConfig)->Result<String,String>, generate_passphrase(PassphraseConfig)->Result<String,String>, evaluate_strength(String)->Result<StrengthResult,String>}` et leurs structs de config/résultat ; `mypass_core::totp::{generate_totp_code(...)->Result<TotpCode,String>, generate_totp_secret()->Result<String,String>}`. Tous les types de config/résultat déménagent avec (restent `pub`, serde inchangé).

- [ ] **Step 1: Baseline**

```powershell
cd src-tauri; cargo test 2>&1 | Select-String "test result"
cd ../crates/mypass-core; cargo test 2>&1 | Select-String "test result"
```

Noter les comptes.

- [ ] **Step 2: Inspecter les dépendances des modules à déplacer**

```powershell
cd C:\projet\mypass
```
Lire `src-tauri/src/commands/generator.rs` et `totp.rs` en entier : relever chaque `use` (notamment `use crate::...` = dépendance interne à réorienter, et `use <crate>` = dépendance Cargo à copier). Repérer si `evaluate_strength` utilise `crate::security::zxcvbn`.

- [ ] **Step 3: Déplacer les fichiers**

```powershell
git mv src-tauri/src/commands/generator.rs crates/mypass-core/src/generator.rs
git mv src-tauri/src/commands/totp.rs crates/mypass-core/src/totp.rs
# si strength séparé :
git mv src-tauri/src/security/zxcvbn.rs crates/mypass-core/src/strength.rs
```

Dans les fichiers déplacés : retirer les `#[tauri::command]` (le crate mypass-core ne connaît pas `tauri`), transformer chaque `pub fn` de commande en `pub fn` ordinaire (signatures identiques). Réorienter les `use crate::security::zxcvbn` → `use crate::strength` etc.

- [ ] **Step 4: Déclarer dans lib.rs + Cargo.toml de mypass-core**

`crates/mypass-core/src/lib.rs` : ajouter `pub mod generator;`, `pub mod totp;`, `pub mod strength;` (ordre alphabétique).

`crates/mypass-core/Cargo.toml` `[dependencies]` : ajouter les crates relevées au Step 2 (ex. `totp-rs = { version = "5", features = ["gen_secret", "otpauth"] }`, `zxcvbn = "..."`), versions copiées verbatim depuis `src-tauri/Cargo.toml`.

- [ ] **Step 5: Recréer les wrappers Tauri minces**

Nouveau `src-tauri/src/commands/generator.rs` :

```rust
//! Wrappers Tauri : la logique vit dans mypass_core::generator.
use crate::kdbx::generator::{self, PasswordConfig, PassphraseConfig, StrengthResult};

#[tauri::command]
pub fn generate_password(config: PasswordConfig) -> Result<String, String> {
    generator::generate_password(config)
}

#[tauri::command]
pub fn generate_passphrase(config: PassphraseConfig) -> Result<String, String> {
    generator::generate_passphrase(config)
}

#[tauri::command]
pub fn evaluate_strength(password: String) -> Result<StrengthResult, String> {
    generator::evaluate_strength(password)
}
```

(Rappel : `crate::kdbx` est le ré-export de `mypass_core` dans src-tauri — voir `lib.rs`. Utiliser ce chemin.) Faire de même pour `totp.rs`. Les `generate_handler![]` de `lib.rs` restent inchangés (mêmes noms de commandes).

- [ ] **Step 6: Nettoyer src-tauri/Cargo.toml**

Pour chaque dépendance déplacée, `grep` dans `src-tauri/src` : si plus AUCUN usage hors des wrappers (qui ne l'utilisent plus directement), retirer la ligne de `src-tauri/Cargo.toml`. Sinon la laisser. Ne rien retirer sans avoir vérifié.

- [ ] **Step 7: Vérifier**

```powershell
cd crates/mypass-core; cargo test; cargo check --target wasm32-unknown-unknown
cd ../../src-tauri; cargo test; cargo clippy
```

Expected: mêmes comptes de tests qu'en baseline (les tests unitaires de generator/totp déménagent dans mypass-core), wasm OK, clippy sans nouveau warning.

- [ ] **Step 8: Commit**

```powershell
git add -A
git status  # vérifier : moves + Cargo.toml x2 + lib.rs, jamais target/ ni *.log
git commit -m "refactor(core): move generator/totp/strength into mypass-core

Pure computation, no Tauri/vault dependency. Tauri commands become thin
wrappers so the wasm crate (sub-project 4b) can reuse the same logic.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Extraire les opérations sur les entrées

**Files:**
- Create: `crates/mypass-core/src/ops/mod.rs` (`pub mod entries; pub mod groups;` — groups arrive en Task 3, déclarer au fur et à mesure)
- Create: `crates/mypass-core/src/ops/entries.rs`
- Modify: `crates/mypass-core/src/lib.rs` (`pub mod ops;`)
- Modify: `src-tauri/src/commands/entries.rs` (wrappers minces)
- Test: déplacer les `#[cfg(test)]` d'entries.rs qui testent la logique pure vers `ops/entries.rs`

**Interfaces:**
- Consumes: `xml::{KeePassFile, Group, Entry, EntryString, Value}`, `Times::touch`, `DeletedObject::now` (déjà dans mypass-core).
- Produces dans `mypass_core::ops::entries` :
  - DTO `EntryInfo`, `NewEntry`, `UpdateEntry` (déplacés verbatim avec leur serde camelCase).
  - `pub fn list(kf: &KeePassFile, group_uuid: Option<&str>) -> Result<Vec<EntryInfo>, String>`
  - `pub fn get(kf: &KeePassFile, uuid: &str) -> Result<EntryInfo, String>`
  - `pub fn create(kf: &mut KeePassFile, new: NewEntry) -> Result<EntryInfo, String>`
  - `pub fn update(kf: &mut KeePassFile, uuid: &str, update: UpdateEntry) -> Result<EntryInfo, String>` (appelle `touch()`)
  - `pub fn delete(kf: &mut KeePassFile, uuid: &str) -> Result<(), String>` (retire + pousse la tombstone)
  - `pub fn duplicate(kf: &mut KeePassFile, uuid: &str) -> Result<EntryInfo, String>`
  - helpers `pub(crate)` : `entry_to_info`, `collect_entries`, `find_entry`, `find_entry_mut`, `find_group`, `find_group_mut`, `remove_entry_from_group`, `set_string_field`, `set_string_field_protected`, `apply_custom_fields` (déplacés verbatim).

- [ ] **Step 1: Créer `ops/entries.rs` en déplaçant la logique**

Créer `crates/mypass-core/src/ops/mod.rs` avec `pub mod entries;`. Créer `ops/entries.rs`. Y déplacer depuis `src-tauri/src/commands/entries.rs` : les 3 structs DTO, tous les helpers purs (lignes ~236-460 : entry_to_info, collect_entries, find_*, set_string_field*, apply_custom_fields, remove_entry_from_group), et transformer le CORPS de chaque commande en fonction pure `list/get/create/update/delete/duplicate` prenant `kf` au lieu de `state.lock()`. Retirer tout `use tauri`/`State`/`Arc`/`Mutex`/`DbState`. Ajouter `pub mod ops;` dans `lib.rs`.

Les `#[cfg(test)]` d'entries.rs testant la logique pure (entry_to_info, custom fields, update via helper…) sont déplacés ici et réécrits pour appeler les fonctions pures directement (plus de State).

- [ ] **Step 2: Vérifier que mypass-core compile et teste**

```powershell
cd crates/mypass-core; cargo test; cargo check --target wasm32-unknown-unknown
```

Expected: PASS. (Les tests déplacés passent contre les fonctions pures.)

- [ ] **Step 3: Réécrire `src-tauri/src/commands/entries.rs` en wrappers**

Chaque commande devient :

```rust
use crate::commands::database::DbState;
use crate::kdbx::ops::entries::{self, EntryInfo, NewEntry, UpdateEntry};
use std::sync::{Arc, Mutex};
use tauri::State;

#[tauri::command]
pub async fn get_entries(
    state: State<'_, Arc<Mutex<DbState>>>,
    group_uuid: Option<String>,
) -> Result<Vec<EntryInfo>, String> {
    let db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_ref().ok_or("No open database")?;
    entries::list(kf, group_uuid.as_deref())
}

#[tauri::command]
pub async fn create_entry(
    state: State<'_, Arc<Mutex<DbState>>>,
    entry: NewEntry,
) -> Result<EntryInfo, String> {
    let mut db = state.lock().map_err(|e| format!("Lock error: {e}"))?;
    let kf = db.keepass_file.as_mut().ok_or("No open database")?;
    let info = entries::create(kf, entry)?;
    db.save()?;
    Ok(info)
}
// update_entry, delete_entry, duplicate_entry : même schéma (mutation → save → retour).
// get_entry, search_entries : lecture seule, pas de save.
```

Vérifier que `search_entries` (qui filtrait via `collect_entries`) utilise `entries::list` puis filtre, OU exposer `entries::search(kf, query)` pur si c'était non trivial — au choix, comportement identique. Les autres fichiers de src-tauri qui importaient `find_entry`/`remove_entry_from_group` depuis `commands::entries` (ex. `native_messaging.rs`, `browser.rs`, `ssh/agent.rs`) doivent réorienter leurs `use` vers `crate::kdbx::ops::entries::...`. Grep `commands::entries::` et `find_entry\|remove_entry_from_group` dans src-tauri pour tous les corriger.

- [ ] **Step 4: Vérifier la non-régression**

```powershell
cd src-tauri; cargo test; cargo clippy
```

Expected: mêmes comptes qu'en baseline, clippy propre.

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "refactor(core): extract entry vault operations into mypass-core::ops::entries

Tauri commands become thin lock/save wrappers over pure functions the
wasm crate will reuse. No behavior change.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Extraire les opérations sur les groupes

**Files:**
- Create: `crates/mypass-core/src/ops/groups.rs`
- Modify: `crates/mypass-core/src/ops/mod.rs` (`pub mod groups;`)
- Modify: `src-tauri/src/commands/groups.rs` (wrappers minces)
- Test: déplacer les tests purs de groups vers `ops/groups.rs`

**Interfaces:**
- Consumes: `xml::{KeePassFile, Group}`, helpers de `ops::entries` si partagés (`find_group_mut` etc. — préférer réutiliser ceux d'`ops::entries` via `pub(crate)` plutôt que redupliquer).
- Produces dans `mypass_core::ops::groups` : DTO `GroupInfo` (+ éventuels `NewGroup`/`UpdateGroup`), et fonctions pures `list/create/update/delete/move_entry` prenant `kf`, avec la même sémantique que les commandes actuelles (y compris les tombstones de `delete_group` déjà en place et le `touch()` de `move_entry`).

- [ ] **Step 1: Déplacer la logique dans `ops/groups.rs`**

Lire `src-tauri/src/commands/groups.rs`. Déplacer DTO + logique pure de `create_group/update_group/delete_group/move_entry/get_groups` en fonctions pures prenant `kf`. Réutiliser les helpers de `ops::entries` (`find_group_mut`, `remove_entry_from_group`, `collect_all_entries`-équivalent pour les tombstones de sous-arbre) — les rendre `pub(crate)` dans `ops::entries` si nécessaire, ne pas dupliquer. Déclarer `pub mod groups;` dans `ops/mod.rs`. Déplacer les tests purs.

- [ ] **Step 2: Vérifier mypass-core**

```powershell
cd crates/mypass-core; cargo test; cargo check --target wasm32-unknown-unknown
```

Expected: PASS.

- [ ] **Step 3: Wrappers Tauri dans `groups.rs`**

Même schéma que Task 2 Step 3 : lock → `groups::create(kf, ...)` → `db.save()` → retour. Lecture seule (`get_groups`) sans save. Réorienter les `use` des autres fichiers src-tauri important des symboles de `commands::groups` (grep).

- [ ] **Step 4: Vérifier**

```powershell
cd src-tauri; cargo test; cargo clippy
cd ..; npm run build
```

Expected: comptes baseline, clippy propre, build front OK (le front n'a pas changé mais on confirme).

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "refactor(core): extract group vault operations into mypass-core::ops::groups

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

## Vérification finale du sous-projet 4a

```powershell
cd crates/mypass-core; cargo test; cargo clippy; cargo check --target wasm32-unknown-unknown
cd ../../src-tauri; cargo test; cargo clippy
cd ..; npm run build
```

Critère de sortie : toute la surface web-MVP (entrées, groupes, générateur, TOTP, force) vit en fonctions pures dans `mypass-core`, réutilisable en wasm ; les commandes Tauri desktop sont des wrappers minces au comportement identique (tests baseline verts) ; wasm compile. Débloque le sous-projet 4b (crate WASM). Restent hors scope, à extraire quand l'app web en aura besoin : passkeys, import/export, dedup, hibp.
