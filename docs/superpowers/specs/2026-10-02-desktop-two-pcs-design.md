# Desktop utilisable sur deux PC — Design (sous-projet 1)

> Sous-projet 1 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-02.

## Intention

Luca installe MyPass sur son PC fixe et son portable. Chaque PC garde **une** copie locale du
coffre, synchronisée par `mypass-server` avec l'autre PC et le téléphone. Sur un PC neuf, il
récupère son coffre existant depuis le serveur en une étape, sans risque d'écraser quoi que ce soit.

**Succès :** le `.msi` est installé sur les deux PC ; une entrée créée sur le fixe apparaît sur le
portable et sur le téléphone en ≤ 60 s ; un mauvais mot de passe affiche une erreur lisible.

## Constat de départ

- `src/views/UnlockView.tsx:17` passe `"mypass-vault.kdbx"` (relatif) à `open_database` et
  `create_database`. Le fichier est résolu depuis le dossier courant du process : non inscriptible
  une fois installé dans `C:\Program Files\MyPass`.
- `create_database` (`src-tauri/src/commands/database.rs:127`) écrit sans vérifier l'existence du
  fichier : il **écraserait** un coffre existant.
- `src/hooks/useDatabase.ts:73-74` lit `error.message`, mais l'IPC Tauri rejette avec une chaîne :
  aucun message d'erreur n'est affiché sur desktop.
- La config de sync (`%APPDATA%\MyPass\sync.json`, `src-tauri/src/commands/sync.rs`) est propre au
  PC et indépendante du coffre ; aujourd'hui elle ne se règle qu'après déverrouillage (`SyncView`).
- Créer un coffre vide puis synchroniser pour « récupérer » le coffre serveur fonctionnerait mal :
  le groupe racine local aurait un UUID différent du distant.
- `tauri.conf.json` déclare une association `.kdbx`, mais `main.rs` ignore le chemin reçu.

## Décisions

| Sujet | Décision |
|---|---|
| Nombre de coffres | **Un seul par PC**, toujours au même endroit. Pas de sélecteur de fichier. Un ancien coffre KeePassXC se reprend via l'import. |
| Emplacement | `%APPDATA%\MyPass\mypass-vault.kdbx` (même dossier que `sync.json` et `browser_settings.json`). |
| Qui décide du chemin | **Rust uniquement.** Le paramètre `path` disparaît de l'IPC `open_database` / `create_database`. |
| PC neuf | Bouton « Récupérer depuis le serveur » sur l'écran de déverrouillage. |
| Mode web (PWA) | Inchangé (le coffre vit sur le serveur). |
| Association `.kdbx` | Retirée du `.msi`. |
| Hors périmètre | Fichier clé dans l'UI, plusieurs coffres, sélecteur de fichier, URL serveur pré-remplie. |

## Design

### 1. Emplacement unique (Rust)

- `fn default_vault_path() -> Result<PathBuf, String>` dans `commands/database.rs` :
  `%APPDATA%\MyPass\mypass-vault.kdbx`, erreur explicite si `APPDATA` est absent.
- Nouvelle commande `get_vault_location() -> { path: String, exists: bool }`, pour choisir le mode
  de l'écran de déverrouillage (et afficher le chemin en petit).
- `open_database(password, keyfile_path?)` et `create_database(password, name, encryption?, keyfile_path?)`
  perdent le paramètre `path` et utilisent `default_vault_path()`. Les fonctions internes prennent
  le chemin en argument, pour être testables avec un fichier temporaire.
- Le corps de `open_database` qui remplit `DbState` après lecture est extrait dans un helper
  réutilisé par la récupération serveur (une seule façon d'ouvrir un coffre).

### 2. Garde-fou contre l'écrasement

- `create_database` échoue si le fichier existe déjà (« un coffre existe déjà sur ce PC »).
  Le dossier parent est créé s'il manque (`create_dir_all`).

### 3. Récupération depuis le serveur

Nouvelle commande `fetch_vault_from_server(serverUrl, token, password) -> DatabaseInfo` :

1. Normaliser l'URL comme `set_sync_config` (trim, sans `/` final) : même helper, pas de copie.
2. Refuser si le coffre local existe déjà.
3. `GET {url}/api/vault` avec `Authorization: Bearer <token>` :
   - réseau en échec → « serveur injoignable » ;
   - 401 → « jeton refusé » ; 404 → « aucun coffre sur le serveur » ; autre → « HTTP <code> ».
4. **Déchiffrer les octets avec le mot de passe avant toute écriture**
   (`kdbx::reader::read_database_bytes`) ; échec → « mot de passe incorrect pour ce coffre ».
5. Écrire les octets **tels quels** (pas de ré-encodage) dans `default_vault_path()`.
6. Enregistrer `sync.json` : URL, jeton, `enabled = true`, `last_etag` = ETag reçu.
7. Ouvrir le coffre via le helper de l'étape 1 et renvoyer `DatabaseInfo`.

Découpage pour les tests : les étapes 2, 4 et 5 forment une fonction pure
`install_remote_vault(bytes, password, path)` sans réseau ; la commande n'ajoute que le HTTP et la
config. Le front enchaîne ensuite comme après un déverrouillage (`unlock()` + `syncNowAndRefresh`).

### 4. Erreurs visibles partout

`tauriCommand` (`src/lib/tauri.ts:461`) convertit tout rejet qui n'est pas une `Error` en
`new Error(String(e))`. Un seul point de passage : tous les appelants (déverrouillage, export,
sync…) reçoivent une `Error` avec un `message`. Le mode web lève déjà des `Error`, inchangé.

**Codes d'erreur** (convention existante, cf. `PASSPHRASE_REQUIRED` dans `ssh/keys.rs`) : les
commandes de coffre renvoient un code stable que l'écran de déverrouillage traduit via
`unlock.errors.<CODE>` (EN + FR), et affichent le message brut pour tout autre texte.

| Code | Cas |
|---|---|
| `NO_VAULT` | `open_database` : aucun fichier à l'emplacement du coffre |
| `VAULT_EXISTS` | création ou récupération alors qu'un coffre existe déjà |
| `NOT_A_VAULT` | octets sans signature KDBX (ex. page HTML d'une mauvaise URL) |
| `WRONG_PASSWORD` | signature KDBX valide mais déchiffrement en échec |
| `SERVER_UNREACHABLE` | erreur réseau / délai dépassé (30 s) |
| `TOKEN_REFUSED` | HTTP 401 |
| `NO_REMOTE_VAULT` | HTTP 404 |
| `SERVER_ERROR` | tout autre statut HTTP ≠ 200 |

Les messages des étapes 3-4 de la section 3 correspondent respectivement à ces codes.

### 5. Écran de déverrouillage (desktop)

Au montage, `get_vault_location` détermine le mode :

- **Coffre présent** → mot de passe + « Déverrouiller ». Pas de bouton « Créer » (il écraserait
  le coffre ; le garde-fou Rust le refuserait de toute façon).
- **Aucun coffre** → deux choix :
  - **Récupérer depuis le serveur** : URL serveur, jeton, mot de passe maître → `fetch_vault_from_server` ;
  - **Créer un nouveau coffre** : nom + mot de passe → `create_database` (formulaire actuel).
- Le mode web garde son comportement actuel (jeton + déverrouiller / créer).
- Toutes les nouvelles chaînes en EN + FR (`src/i18n/`).

### 6. Parité IPC

- Mock (`src/lib/tauri.ts`) : `get_vault_location` (`exists` reflète l'état du mock),
  `fetch_vault_from_server` (ouvre le mock comme `open_database`).
- Web (`src/lib/web.ts`) : `open_database` / `create_database` ignorent déjà `path` ; aucune
  nouvelle commande (l'écran web n'appelle pas `get_vault_location`).
- `generate_handler![]` (`src-tauri/src/lib.rs`) : ajouter `get_vault_location` et
  `fetch_vault_from_server`.

### 7. Association `.kdbx`

Supprimer `bundle.fileAssociations` de `src-tauri/tauri.conf.json`.

## Tests et vérification

**Tests Rust automatisés (fichiers temporaires, jamais le serveur de prod) :**
- `install_remote_vault` avec un mauvais mot de passe → erreur, **aucun fichier créé**.
- `install_remote_vault` avec un fichier déjà présent → refus, fichier intact (octets inchangés).
- `install_remote_vault` nominal → fichier identique octet pour octet, rouvrable avec le mot de passe.
- Création de coffre sur un chemin existant → refus, fichier intact.

**Non-régression :** `cargo test` (`src-tauri`, `crates/mypass-core`), `cargo clippy` sans nouveau
warning (référence : 11), `npm run lint`, `npm run build`, `npm run build:web`.

**E2E manuel (Luca + Claude) :**
1. `npm run tauri dev` sur ce PC sans coffre : l'écran propose « Récupérer » / « Créer ».
2. Récupérer avec un mauvais jeton → « jeton refusé » ; avec un mauvais mot de passe → erreur, et
   aucun fichier dans `%APPDATA%\MyPass\`.
3. Récupérer avec les bons identifiants → coffre ouvert, entrées présentes, sync « synchronisé ».
4. Verrouiller puis mauvais mot de passe → message d'erreur affiché.
5. `npm run tauri build`, installer le `.msi` sur le fixe et le portable (Smart App Control
   désactivé sur chacun), récupérer le coffre sur chacun.
6. Créer une entrée sur le fixe → visible sur le portable et sur le téléphone en ≤ 60 s.

## Risques

- **Smart App Control** sur l'autre PC : bloquerait le `.msi` non signé. Vérifier
  `VerifiedAndReputablePolicyState` avant l'étape 5 de l'E2E (voir sous-projet 7 pour la signature).
- **Jeton :** stocké en clair dans `sync.json` (choix déjà assumé, `ponytail:` dans `sync.rs:15`) ;
  jamais loggé ni renvoyé au front.
- **E2E sur le coffre réel :** la récupération ne fait que lire le serveur ; la création d'entrée de
  l'étape 6 écrit dans le vrai coffre. Utiliser une entrée de test clairement nommée et la supprimer
  à la fin.
