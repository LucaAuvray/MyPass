# Desktop sur deux PC — Implementation Plan (sous-projet 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** L'app desktop garde un coffre unique dans `%APPDATA%\MyPass\`, sait le récupérer depuis le serveur sur un PC neuf sans jamais écraser un coffre, et affiche des erreurs lisibles.

**Architecture:** Rust devient le seul à décider du chemin du coffre (`default_vault_path`) ; `open_database` / `create_database` perdent leur paramètre `path`. Une couche fichier pure dans `commands/database.rs` (`read_vault_bytes`, `create_vault_file`, `install_remote_vault`, `load_into_state`) est testée sans réseau ; `commands/sync.rs` ajoute la commande HTTP `fetch_vault_from_server`. Côté front, `tauriCommand` normalise les rejets en `Error`, et `UnlockView` choisit son mode selon `get_vault_location`.

**Tech Stack:** Rust (Tauri 2, `mypass-core` réexporté en `kdbx`, `reqwest` déjà présent), React 19 + TanStack Query + react-i18next.

**Spec:** `docs/superpowers/specs/2026-10-02-desktop-two-pcs-design.md`

## Global Constraints

- Emplacement unique du coffre : `%APPDATA%\MyPass\mypass-vault.kdbx`.
- Codes d'erreur, verbatim : `NO_VAULT`, `VAULT_EXISTS`, `NOT_A_VAULT`, `WRONG_PASSWORD`, `SERVER_UNREACHABLE`, `TOKEN_REFUSED`, `NO_REMOTE_VAULT`, `SERVER_ERROR`.
- Délai réseau de `fetch_vault_from_server` : 30 s.
- Les octets récupérés sont écrits **tels quels** (pas de ré-encodage), **après** un déchiffrement réussi.
- Aucun test automatisé ne parle au serveur de prod ; fichiers temporaires sous `std::env::temp_dir()` uniquement.
- Le jeton n'est jamais loggé ni renvoyé au front.
- `std::sync::Mutex<DbState>` n'est jamais tenu à travers un `await`.
- Toute commande ajoutée : `tauri::generate_handler![]` (`src-tauri/src/lib.rs`) + cas mock (`src/lib/tauri.ts`).
- Mode web (`src/lib/web.ts`, `isWebMode()`) : comportement inchangé.
- Chaînes UI en EN + FR (`src/i18n/en.json`, `src/i18n/fr.json`), jamais en dur.
- `cargo clippy` : pas de nouveau warning (référence `src-tauri` : 11).
- Rust : `export PATH="$HOME/.cargo/bin:$PATH"` dans les shells qui ne l'ont pas encore.

## Review Focus

- URL collée avec espaces ou `/` final, jeton collé avec espaces ou retour ligne → normalisés, la récupération marche (test : `fetched_sync_config_normalizes_inputs`, Task 2).
- PC neuf : `%APPDATA%\MyPass\` n'existe pas encore → créé à la création comme à la récupération (tests : `create_vault_file_creates_missing_parent`, Task 1 ; `install_remote_vault_writes_bytes_verbatim_and_creates_parent`, Task 2).
- Mauvaise URL qui répond 200 avec une page HTML → `NOT_A_VAULT` (pas « mauvais mot de passe »), rien n'est écrit (test : `install_remote_vault_rejects_html`, Task 2).
- Double clic / double Entrée sur « Récupérer » ou « Créer » → bouton désactivé pendant l'appel, et la 2ᵉ écriture refusée atomiquement (`create_new`) sans abîmer la 1ʳᵉ (test : `install_remote_vault_refuses_existing_file`, Task 2 ; vérification manuelle Task 4).
- Verrouiller puis revenir à l'écran de déverrouillage → mode « déverrouiller » (le coffre existe), pas « aucun coffre » (vérification manuelle Task 4, étape E2E Task 5).

---

### Task 1: Coffre local à emplacement fixe (Rust)

**Files:**
- Modify: `src-tauri/src/commands/database.rs` (commandes `open_database` l.71-124, `create_database` l.126-183, module `tests` en fin de fichier)
- Modify: `src-tauri/src/lib.rs:50-55` (enregistrer `get_vault_location`)

**Interfaces:**
- Consumes: `kdbx::reader::{read_database_bytes, DatabaseReadResult}`, `kdbx::writer::{write_database, write_database_bytes}`, `kdbx::xml::KeePassFile`.
- Produces (utilisés par Task 2 et 3) :
  - `pub fn default_vault_path() -> Result<PathBuf, String>` — `%APPDATA%\MyPass\mypass-vault.kdbx`, `Err("APPDATA introuvable")` si la variable manque.
  - `pub fn read_vault_bytes(bytes: &[u8], password: &str, keyfile: Option<&[u8]>) -> Result<kdbx::reader::DatabaseReadResult, String>` — `Err("NOT_A_VAULT")` si moins de 8 octets ou signature ≠ `9A A2 D9 03 B5 4B FB 67` (LE : `0x03D9A29A`, `0x67FB4BB5`) ; toute autre erreur de lecture → `Err("WRONG_PASSWORD")` (`// ponytail:` : un fichier corrompu est indiscernable d'un mauvais mot de passe).
  - `pub fn load_into_state(db: &mut DbState, path: PathBuf, result: kdbx::reader::DatabaseReadResult, password: &str, keyfile: Option<Vec<u8>>) -> DatabaseInfo` — extrait du corps actuel d'`open_database` (l.94-123), sans changement de comportement.
  - `#[derive(serde::Serialize)] pub struct VaultLocation { pub path: String, pub exists: bool }`
  - `#[tauri::command] pub async fn get_vault_location() -> Result<VaultLocation, String>`
  - IPC : `open_database(password, keyfilePath?)`, `create_database(password, name, encryption?, keyfilePath?)` — plus de `path`.

- [ ] **Step 1: Écrire les tests qui échouent** dans le module `tests` de `database.rs` (chemins uniques via `std::env::temp_dir().join(format!("mypass-…-{}", uuid::Uuid::new_v4()))`, nettoyés en fin de test)

```rust
fn sample_vault(password: &str) -> Vec<u8> {
    kdbx::writer::write_database_bytes(&KeePassFile::new("T"), password, None, Cipher::Aes256, &KdfParams::default()).unwrap()
}

#[test]
fn read_vault_bytes_rejects_non_kdbx() {
    assert_eq!(read_vault_bytes(b"<!doctype html><html></html>", "pw", None).unwrap_err(), "NOT_A_VAULT");
    assert_eq!(read_vault_bytes(b"", "pw", None).unwrap_err(), "NOT_A_VAULT");
}

#[test]
fn read_vault_bytes_maps_wrong_password() {
    let bytes = sample_vault("right");
    assert_eq!(read_vault_bytes(&bytes, "wrong", None).unwrap_err(), "WRONG_PASSWORD");
    assert_eq!(read_vault_bytes(&bytes, "right", None).unwrap().keepass_file.meta.database_name, "T");
}

#[test]
fn create_vault_file_refuses_existing() {
    // fichier existant contenant b"keep me"
    // create_vault_file(&path, &KeePassFile::new("T"), "pw", None, Cipher::Aes256, &KdfParams::default())
    //   == Err("VAULT_EXISTS".to_string()) ; std::fs::read(&path) == b"keep me"
}

#[test]
fn create_vault_file_creates_missing_parent() {
    // path = temp/<uuid>/MyPass/mypass-vault.kdbx (dossiers absents)
    // create_vault_file(...).is_ok() ; kdbx::reader::read_database(&path, "pw", None).is_ok()
}
```

- [ ] **Step 2: Vérifier que les tests échouent**

Run: `cd src-tauri && cargo test --lib commands::database`
Expected: erreur de compilation (`read_vault_bytes`, `create_vault_file` introuvables).

- [ ] **Step 3: Implémenter dans `database.rs`**
  - `default_vault_path`, `read_vault_bytes`, `load_into_state`, `VaultLocation`, `get_vault_location` selon Interfaces.
  - `fn create_vault_file(path: &Path, keepass_file: &KeePassFile, password: &str, keyfile: Option<&[u8]>, cipher: Cipher, kdf: &KdfParams) -> Result<(), String>` : `Err("VAULT_EXISTS")` si `path.exists()`, sinon `create_dir_all(parent)` puis `kdbx::writer::write_database`.
  - `open_database(state, password, keyfile_path)` : `path = default_vault_path()?` ; `std::fs::read` → `ErrorKind::NotFound` ⇒ `Err("NO_VAULT")` ; puis `read_vault_bytes` et `load_into_state`.
  - `create_database(state, password, name, encryption, keyfile_path)` : `path = default_vault_path()?`, écrit via `create_vault_file`, remplit `DbState` comme aujourd'hui.
  - `lib.rs` : ajouter `commands::database::get_vault_location` dans `generate_handler!`.

- [ ] **Step 4: Vérifier que les tests passent**

Run: `cd src-tauri && cargo test --lib commands::database && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: 5 tests `ok` (les 4 nouveaux + `test_save_persists_custom_data`) ; compteur clippy ≤ 11.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/database.rs src-tauri/src/lib.rs
git commit -m "feat(desktop): fixed vault location in APPDATA, never overwrite on create"
```

---

### Task 2: Récupération du coffre depuis le serveur (Rust)

**Files:**
- Modify: `src-tauri/src/commands/database.rs` (ajout `install_remote_vault` + tests)
- Modify: `src-tauri/src/commands/sync.rs` (`set_sync_config` l.84-104, nouvelle commande, module `tests` l.~300)
- Modify: `src-tauri/src/lib.rs:108-112` (enregistrer `fetch_vault_from_server`)

**Interfaces:**
- Consumes (Task 1) : `default_vault_path`, `read_vault_bytes`, `load_into_state`, `DatabaseInfo`, `DbState`.
- Produces :
  - `pub fn install_remote_vault(bytes: &[u8], password: &str, path: &Path) -> Result<kdbx::reader::DatabaseReadResult, String>` (dans `database.rs`) : `read_vault_bytes(bytes, password, None)?` **avant** toute écriture ; `create_dir_all(parent)` ; écriture via `OpenOptions::new().write(true).create_new(true)` (`AlreadyExists` ⇒ `Err("VAULT_EXISTS")`) des octets tels quels.
  - `fn normalize_server_url(url: &str) -> String` (dans `sync.rs`) : `trim()` puis sans `/` final — extrait de `set_sync_config`, qui l'appelle désormais.
  - `fn fetched_sync_config(server_url: &str, token: &str, etag: Option<String>) -> SyncConfig` : URL normalisée, jeton `trim()`, `enabled: true`, `last_etag: etag`.
  - IPC : `fetch_vault_from_server(serverUrl, token, password) -> DatabaseInfo`.

- [ ] **Step 1: Écrire les tests qui échouent**

Dans `database.rs` (réutiliser `sample_vault` de Task 1) :

```rust
#[test]
fn install_remote_vault_wrong_password_writes_nothing() {
    // install_remote_vault(&sample_vault("right"), "wrong", &path).unwrap_err() == "WRONG_PASSWORD" ; !path.exists()
}
#[test]
fn install_remote_vault_rejects_html() {
    // install_remote_vault(b"<!doctype html>", "pw", &path).unwrap_err() == "NOT_A_VAULT" ; !path.exists()
}
#[test]
fn install_remote_vault_refuses_existing_file() {
    // path contient b"keep me" ; install_remote_vault(&sample_vault("pw"), "pw", &path).unwrap_err() == "VAULT_EXISTS"
    // std::fs::read(&path) == b"keep me"
}
#[test]
fn install_remote_vault_writes_bytes_verbatim_and_creates_parent() {
    // path = temp/<uuid>/MyPass/mypass-vault.kdbx ; bytes = sample_vault("pw")
    // install_remote_vault(&bytes, "pw", &path).is_ok() ; std::fs::read(&path) == bytes
    // kdbx::reader::read_database(&path, "pw", None).is_ok()
}
```

Dans `sync.rs` :

```rust
#[test]
fn fetched_sync_config_normalizes_inputs() {
    let cfg = fetched_sync_config(" https://mypass.example.ts.net/ ", "  tok \n", Some("\"7\"".into()));
    assert_eq!(cfg.server_url, "https://mypass.example.ts.net");
    assert_eq!(cfg.token, "tok");
    assert!(cfg.enabled);
    assert_eq!(cfg.last_etag.as_deref(), Some("\"7\""));
}
```

- [ ] **Step 2: Vérifier que les tests échouent**

Run: `cd src-tauri && cargo test --lib commands::`
Expected: erreur de compilation (`install_remote_vault`, `fetched_sync_config` introuvables).

- [ ] **Step 3: Implémenter**
  - `install_remote_vault` (database.rs), `normalize_server_url` + `fetched_sync_config` (sync.rs) selon Interfaces ; `set_sync_config` utilise `normalize_server_url`.
  - `#[tauri::command] pub async fn fetch_vault_from_server(db: State<'_, Arc<Mutex<DbState>>>, server_url: String, token: String, password: String) -> Result<DatabaseInfo, String>` dans `sync.rs`, dans cet ordre :
    1. `path = default_vault_path()?` ; `path.exists()` ⇒ `Err("VAULT_EXISTS")` (avant tout réseau).
    2. `reqwest::Client::builder().timeout(Duration::from_secs(30))` ; `GET {normalize_server_url(&server_url)}/api/vault` avec `bearer_auth(token.trim())`.
    3. Échec d'envoi ou de lecture du corps ⇒ `SERVER_UNREACHABLE` ; 401 ⇒ `TOKEN_REFUSED` ; 404 ⇒ `NO_REMOTE_VAULT` ; autre ≠ 200 ⇒ `SERVER_ERROR`. Lire l'en-tête `etag` (verbatim).
    4. `install_remote_vault(&bytes, &password, &path)?`, puis `store_config(&fetched_sync_config(&server_url, &token, etag))?`.
    5. Verrouiller `db` (aucun `await` ensuite) et renvoyer `load_into_state(&mut db, path, result, &password, None)`.
  - `lib.rs` : ajouter `commands::sync::fetch_vault_from_server`.

- [ ] **Step 4: Vérifier que les tests passent**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets 2>&1 | grep -c "^warning"`
Expected: tous les tests `ok` (dont les 5 nouveaux) ; compteur clippy ≤ 11.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/database.rs src-tauri/src/commands/sync.rs src-tauri/src/lib.rs
git commit -m "feat(desktop): fetch the vault from the sync server on a fresh PC"
```

---

### Task 3: Couche IPC front (erreurs, mock, hook)

**Files:**
- Modify: `src/lib/tauri.ts` (`tauriCommand` l.461-464, `mockStore` l.80-86, cas `create_database`/`open_database` l.107-129)
- Modify: `src/hooks/useDatabase.ts`
- Modify: `src/types/database.ts`
- Modify: `src/views/UnlockView.tsx:17,34,43` (seulement : supprimer `dbPath` et l'argument `path`, pour que tout compile)

**Interfaces:**
- Consumes (Tasks 1-2) : IPC `get_vault_location`, `open_database(password, keyfilePath?)`, `create_database(password, name, encryption?, keyfilePath?)`, `fetch_vault_from_server(serverUrl, token, password)`.
- Produces (Task 4) :
  - `export interface VaultLocation { path: string; exists: boolean }` dans `src/types/database.ts`.
  - `useDatabase()` renvoie en plus : `vaultLocation` (`useQuery` clé `["database", "location"]`, `enabled: !isWebMode()`), `fetchFromServer(params: { serverUrl: string; token: string; password: string }) => Promise<DatabaseInfo>`, `isFetching: boolean`, `fetchError?: string`. `openDatabase` prend `{ password: string; keyfilePath?: string }` ; `createDatabase` prend `{ password: string; name: string; encryption?: string; keyfilePath?: string }`. Le succès de `fetchFromServer` fait comme `openMutation` (`setQueryData` + `unlock()`).

- [ ] **Step 1: Normaliser les rejets dans `tauriCommand`** : tout rejet qui n'est pas `instanceof Error` est relancé en `new Error(String(e))`.

- [ ] **Step 2: Mock** : `mockStore.vaultExists = false` ; `get_vault_location` → `{ path: "mock-vault.kdbx", exists: mockStore.vaultExists }` ; `create_database`, `open_database` et le nouveau cas `fetch_vault_from_server` passent `vaultExists` et `isOpen` à `true` et renvoient le même objet que `open_database` aujourd'hui.

- [ ] **Step 3: Hook et type** selon Interfaces ; dans `UnlockView.tsx`, supprimer `dbPath` et l'argument `path` des deux appels (rien d'autre).

- [ ] **Step 4: Vérifier**

Run: `npm run lint && npm run build`
Expected: 0 erreur, 0 warning ; build OK.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri.ts src/hooks/useDatabase.ts src/types/database.ts src/views/UnlockView.tsx
git commit -m "feat(front): normalise Tauri errors, vault location and fetch in useDatabase"
```

---

### Task 4: Écran de déverrouillage selon l'état du coffre

**Files:**
- Modify: `src/views/UnlockView.tsx`
- Modify: `src/i18n/en.json`, `src/i18n/fr.json` (bloc `unlock`)

**Interfaces:**
- Consumes (Task 3) : `useDatabase()` → `vaultLocation`, `openDatabase`, `createDatabase`, `fetchFromServer`, `isOpening`, `isCreating`, `isFetching`, `openError`, `createError`, `fetchError` ; `syncNowAndRefresh`.

- [ ] **Step 1: Ajouter les clés i18n** (EN / FR, verbatim)

| Clé | EN | FR |
|---|---|---|
| `unlock.noVaultTitle` | No vault on this PC yet | Aucun coffre sur ce PC |
| `unlock.fetchFromServer` | Get my vault from the server | Récupérer mon coffre depuis le serveur |
| `unlock.fetchTitle` | Get your vault from your sync server | Récupérez votre coffre depuis votre serveur de synchronisation |
| `unlock.fetch` | Download and unlock | Récupérer et déverrouiller |
| `unlock.serverUrl` | Server URL | URL du serveur |
| `unlock.back` | Back | Retour |
| `unlock.errors.NO_VAULT` | No vault found on this PC | Aucun coffre trouvé sur ce PC |
| `unlock.errors.VAULT_EXISTS` | A vault already exists on this PC | Un coffre existe déjà sur ce PC |
| `unlock.errors.NOT_A_VAULT` | The server did not return a KeePass vault — check the URL | Le serveur n'a pas renvoyé de coffre KeePass — vérifiez l'URL |
| `unlock.errors.WRONG_PASSWORD` | Wrong master password | Mot de passe maître incorrect |
| `unlock.errors.SERVER_UNREACHABLE` | Server unreachable — check the URL and your connection | Serveur injoignable — vérifiez l'URL et votre connexion |
| `unlock.errors.TOKEN_REFUSED` | Server token refused | Token du serveur refusé |
| `unlock.errors.NO_REMOTE_VAULT` | No vault on the server yet | Aucun coffre sur le serveur |
| `unlock.errors.SERVER_ERROR` | Unexpected server response | Réponse inattendue du serveur |

- [ ] **Step 2: Réécrire les modes de `UnlockView`**
  - Desktop (`!isWebMode()`) : tant que `vaultLocation` charge → rien d'autre que le logo. `exists` → formulaire mot de passe + « Déverrouiller » sans lien « Créer ». `!exists` → titre `unlock.noVaultTitle` et deux boutons : `unlock.fetchFromServer` (mode `fetch` : champs URL, jeton, mot de passe, bouton `unlock.fetch`) et `unlock.switchToCreate` (formulaire de création actuel) ; chaque sous-mode a un bouton `unlock.back`. Afficher `vaultLocation.data.path` en petit sous le formulaire.
  - Web : comportement actuel conservé (bascule unlock/create, champ jeton si absent).
  - Après succès de `fetchFromServer` ou `createDatabase` : `void syncNowAndRefresh(queryClient).catch(() => {})`, comme aujourd'hui après déverrouillage.
  - Erreur affichée : si le message est un code `/^[A-Z_]+$/` et que `i18n.exists("unlock.errors." + code)`, afficher `t("unlock.errors." + code)`, sinon le message brut.
  - Boutons de soumission `disabled` pendant `isOpening || isCreating || isFetching`.

- [ ] **Step 3: Vérifier statiquement**

Run: `npm run lint && npm run build`
Expected: 0 erreur, 0 warning ; build OK.

- [ ] **Step 4: Vérifier dans le navigateur (mock)**

Run: `npm run dev`, ouvrir `http://localhost:1420`.
Expected : écran « Aucun coffre sur ce PC » avec les deux choix ; « Récupérer » ouvre le mock ; après « Verrouiller », l'écran revient en mode déverrouiller (le mock existe). Un double clic rapide sur un bouton de soumission ne déclenche qu'un appel (bouton grisé pendant l'appel).

- [ ] **Step 5: Commit**

```bash
git add src/views/UnlockView.tsx src/i18n/en.json src/i18n/fr.json
git commit -m "feat(front): unlock screen picks unlock / fetch / create from vault state"
```

---

### Task 5: Packaging et E2E sur les deux PC

**Files:**
- Modify: `src-tauri/tauri.conf.json` (supprimer `bundle.fileAssociations`)
- Modify: `docs/superpowers/specs/2026-10-02-roadmap-design.md` (tableau de suivi)

- [ ] **Step 1: Retirer l'association `.kdbx`** de `tauri.conf.json`.

- [ ] **Step 2: Non-régression complète**

Run: `cd crates/mypass-core && cargo test` ; `cd src-tauri && cargo test && cargo clippy --all-targets` ; `npm run lint` ; `npm run build` ; `npm run build:web` ; `npm run smoke:wasm`
Expected : tout vert, clippy ≤ 11 warnings.

- [ ] **Step 3: E2E en dev (Luca saisit URL, jeton et mot de passe ; Claude vérifie)**

Avant : `%APPDATA%\MyPass\mypass-vault.kdbx` absent. `npm run tauri dev`.
1. Écran « Aucun coffre sur ce PC ».
2. Récupérer avec un jeton faux → « Token du serveur refusé » ; avec un mauvais mot de passe → « Mot de passe maître incorrect » ; dans les deux cas le fichier n'existe toujours pas.
3. Récupérer avec les bons identifiants → coffre ouvert, entrées visibles, `sync.json` contient l'URL et `enabled: true`, statut « synchronisé ».
4. Verrouiller → mode déverrouiller ; mauvais mot de passe → « Mot de passe maître incorrect ».

- [ ] **Step 4: `.msi` et deux PC**

Run: `npm run tauri build` → `src-tauri/target/release/bundle/msi/MyPass_0.1.0_x64_en-US.msi`.
Sur chaque PC : vérifier `VerifiedAndReputablePolicyState = 0`, installer le `.msi`, récupérer le coffre.
5. Créer sur le fixe une entrée `ZZ-test-sync-<date>` → visible sur le portable et sur le téléphone en ≤ 60 s ; la supprimer ensuite, suppression propagée.

- [ ] **Step 5: Mettre à jour le suivi et commiter**

Cocher Plan et Fait du sous-projet 1 dans la feuille de route.

```bash
git add src-tauri/tauri.conf.json docs/superpowers/specs/2026-10-02-roadmap-design.md
git commit -m "chore(desktop): drop .kdbx association; sub-project 1 done"
```
