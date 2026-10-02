# Import / export — Implementation Plan (sous-projet 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Exporter en JSON (complet) ou CSV et réimporter sans perte, sur desktop et PWA ; un CSV Google / Apple / KeePassXC / Bitwarden atterrit dans les bons champs ; un doublon avec le coffre ne crée jamais de copie.

**Architecture:** Un module `crates/mypass-core/src/ops/transfer.rs` possède les formats dans les deux sens (`export`, `parse_import`) et l'écriture (`apply_import`) ; desktop (`src-tauri/src/commands/import_export.rs`, réécrit) et wasm (`crates/mypass-wasm`) l'exposent sous trois commandes IPC de même forme (`parse_import`, `import_entries`, `export_entries`). Le front garde le choix du fichier, le dédoublonnage (`src/lib/dedup.ts`, sans ses parseurs) et l'enregistrement : dialogue Rust sur desktop, téléchargement `Blob` en PWA.

**Tech Stack:** Rust (`csv` 1 et `serde_json` 1 ajoutés à `mypass-core`, `tauri-plugin-dialog` 2 déjà présent), wasm-bindgen, React 19 + react-i18next.

**Spec:** `docs/superpowers/specs/2026-10-02-import-export-design.md`

## Global Constraints

- Formats verbatim : JSON `{"format":"mypass","version":1,"entries":[…]}` ; en-tête CSV exporté `Group,Title,Username,Password,URL,Notes,TOTP,Tags` ; champs d'`ImportedEntry` en camelCase : `group`, `title`, `username`, `password`, `url`, `notes`, `tags`, `totp`, `customFields`.
- Alias d'en-têtes CSV (minuscules, espaces de bord retirés) : title `title|name|account` ; username `username|user name|login|login_username|user` ; password `password|login_password` ; url `url|uri|website|web site|login_uri` ; notes `notes|note|comments|extra` ; totp `totp|otp|otpauth|login_totp` ; group `group|folder` ; tags `tags`.
- Codes d'erreur verbatim : `IMPORT_EMPTY`, `CSV_NO_HEADER`, `JSON_NOT_MYPASS` (traduits `import.errors.<CODE>`, EN + FR).
- TOTP stocké dans le champ `otp` en `otpauth://` ; secret nu → `otpauth://totp/<titre percent-encodé>?secret=<secret sans espaces, en majuscules>`.
- Une entrée importée sans `replaceUuid` reçoit **toujours un nouvel UUID**.
- Dialogue « Enregistrer sous » ouvert **côté Rust** ; aucune commande n'écrit à un chemin venu du front ; aucune permission `fs` ajoutée ; le `Mutex<DbState>` n'est jamais tenu pendant le dialogue.
- Nom de fichier par défaut : `mypass-export-AAAA-MM-JJ.<json|csv>` (calculé côté TS, passé en `fileName`).
- Parité : chaque commande ajoutée ou retirée l'est dans `generate_handler!`, `src/lib/web.ts`, `src/lib/mock.ts`.
- Chaînes UI en EN + FR, jamais en dur. Aucun test ne touche le serveur de prod ni un vrai coffre.
- `cargo clippy` (`src-tauri`) sans nouveau warning : référence mesurée avant Task 1 (attendu 8), notée au ledger.
- Rust : `export PATH="$HOME/.cargo/bin:$PATH"`. Après chaque build, `src-tauri/Cargo.toml` et `src-tauri/gen/schemas/*.json` peuvent ne changer que de fins de ligne : `git diff --ignore-cr-at-eol --stat` vide, puis `git checkout --` ; ne jamais les committer pour ça.

## Review Focus

- CSV ré-enregistré par Excel en français : BOM, séparateur `;`, fins de ligne CRLF → lu correctement (test `csv_bom_semicolon_crlf`, Task 1).
- Coffre venu de KeePassXC avec une corbeille pleine → ses entrées ne partent pas dans l'export (test `export_skips_recycle_bin`, Task 1).
- Entrée KeePass 2 avec `TOTP Seed` (secret nu) exportée puis réimportée → `otp` en `otpauth://` avec le même secret (test `totp_bare_secret_becomes_otpauth`, Task 1 ; aller-retour Task 2).
- `replaceUuid` vers une entrée supprimée entre-temps (par la sync) → entrée créée, pas d'erreur (test `apply_import_unknown_replace_creates`, Task 2).
- Même fichier importé deux fois → la 2ᵉ fois tout est doublon ; « garder celle du coffre » → 0 ajoutée, 0 copie (E2E, Task 5).

---

### Task 1: Formats dans le noyau (`export`, `parse_import`)

**Files:**
- Create: `crates/mypass-core/src/ops/transfer.rs` (+ `pub mod transfer;` dans `ops/mod.rs`)
- Modify: `crates/mypass-core/Cargo.toml` (`csv = "1"`, `serde_json = "1"`)

**Interfaces:**
- Consumes: `xml::{KeePassFile, Group, Entry}` (`entry.strings`, `entry.tags: Option<String>`, `group.name`, `group.groups`, `group.entries`, `kf.meta.recycle_bin_uuid: Option<String>`), `Entry::{title, username, password, url, notes}`.
- Produces (Tasks 2-3) :
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)] #[serde(rename_all = "camelCase", default)] pub struct ImportedEntry { pub group: String, pub title: String, pub username: String, pub password: String, pub url: String, pub notes: String, pub tags: Vec<String>, pub totp: String, pub custom_fields: BTreeMap<String, String> }`
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum ExportFormat { Json, Csv }` + `impl ExportFormat { pub fn parse(s: &str) -> Result<Self, String> }` (`"json"`, `"csv"`, sinon `Err("UNKNOWN_FORMAT")`)
  - `pub fn export(kf: &KeePassFile, format: ExportFormat, uuids: &[String]) -> Result<String, String>`
  - `pub fn parse_import(content: &str) -> Result<Vec<ImportedEntry>, String>`
  - `pub(crate) fn normalize_totp(value: &str, title: &str) -> String`
- Règles : `customFields` à l'export = tous les `strings` sauf `Title`, `UserName`, `Password`, `URL`, `Notes`, `otp`, `TOTP Seed` ; `totp` = `otp`, sinon `TOTP Seed`, sinon `""` ; `group` = noms des groupes depuis la racine (racine exclue) joints par `/` ; tags KDBX séparés par `,` ou `;`. Import : titre vide → URL ; ligne sans titre, identifiant, mot de passe ni URL écartée.

- [ ] **Step 1: Tests qui échouent** (module `tests` de `transfer.rs`, coffres construits en mémoire)

```rust
#[test] fn csv_google_headers()            // "name,url,username,password,note\nGmail,https://mail.google.com,me@x.io,pw1,hello"
    // → title "Gmail", url "https://mail.google.com", username "me@x.io", password "pw1", notes "hello"
#[test] fn csv_apple_headers_with_otpauth() // "Title,URL,Username,Password,Notes,OTPAuth" + ligne avec "otpauth://totp/A?secret=JBSWY3DPEHPK3PXP"
    // → totp == cette URI telle quelle
#[test] fn csv_keepassxc_headers_with_group() // "Group,Title,Username,Password,URL,Notes,TOTP,Icon,Last Modified,Created" + "Perso/Banque,…"
    // → group "Perso/Banque"
#[test] fn csv_bitwarden_headers()          // "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp"
    // → title = name, url = login_uri, username = login_username, password = login_password, totp normalisé, group = folder
#[test] fn csv_quoted_multiline_and_escaped_quotes() // notes "\"ligne 1\nligne 2\"" et titre "\"Dit \"\"bonjour\"\"\""
    // → notes "ligne 1\nligne 2", title "Dit \"bonjour\""
#[test] fn csv_bom_semicolon_crlf()         // "\u{feff}name;url;username;password\r\nA;https://a.io;u;p\r\n"
    // → 1 entrée, champs corrects
#[test] fn csv_unknown_headers_error()      // "foo,bar\n1,2" → Err("CSV_NO_HEADER")
#[test] fn csv_header_only_is_empty()       // "name,url,username,password\n" → Err("IMPORT_EMPTY")
#[test] fn json_not_mypass_errors()         // "{\"items\":[]}", "{\"format\":\"mypass\",\"version\":2,\"entries\":[]}", "{oops" → Err("JSON_NOT_MYPASS")
#[test] fn totp_bare_secret_becomes_otpauth() // normalize_totp("jbsw y3dp ehpk 3pxp", "Mon site")
    // == "otpauth://totp/Mon%20site?secret=JBSWY3DPEHPK3PXP" ; une valeur "OTPAUTH://…" reste telle quelle
#[test] fn export_csv_header_and_row()      // coffre 1 entrée dans "Perso" avec otp et tags "a,b"
    // → 1ʳᵉ ligne exactement "Group,Title,Username,Password,URL,Notes,TOTP,Tags", 2ᵉ commence par "Perso,"
#[test] fn export_json_shape()              // → serde_json::Value : format == "mypass", version == 1, entries[0].customFields contient "MyPass_Type"
#[test] fn export_skips_recycle_bin()       // meta.recycle_bin_uuid = uuid d'un sous-groupe contenant 1 entrée → absente de l'export
#[test] fn export_filters_by_uuid()         // 2 entrées, uuids = [la 1ʳᵉ] → 1 seule exportée
```

- [ ] **Step 2: Lancer** — `cd crates/mypass-core && cargo test transfer 2>&1 | tail -5` → Expected: FAIL à la compilation (`export`, `parse_import`… introuvables).

- [ ] **Step 3: Implémenter** dans `transfer.rs`. CSV : `csv::ReaderBuilder::new().has_headers(true).flexible(true).delimiter(d)` sur le contenu sans BOM, `d` = `b';'` si la 1ʳᵉ ligne compte plus de `;` que de `,`, sinon `b','` ; écriture via `csv::Writer::from_writer(Vec::new())`. JSON : `serde_json::to_string_pretty` d'une struct `{format, version, entries}` ; lecture d'une struct de même forme. Percent-encoding du titre : tout octet hors `A-Z a-z 0-9 - . _ ~` → `%XX` (quelques lignes, aucune dépendance).

- [ ] **Step 4: Lancer** — `cd crates/mypass-core && cargo test 2>&1 | grep "test result"` → Expected: `ok`, 0 failed (les 36 tests existants + les nouveaux).

- [ ] **Step 5: Commit**

```bash
git add crates/mypass-core Cargo.lock
git commit -m "feat(core): MyPass JSON/CSV export and header-aware CSV import"
```

### Task 2: Écriture de l'import et allers-retours (`apply_import`)

**Files:**
- Modify: `crates/mypass-core/src/ops/transfer.rs`

**Interfaces:**
- Consumes: `ImportedEntry`, `export`, `parse_import`, `normalize_totp` (Task 1) ; `ops::entries::{find_entry_mut, set_string_field, set_string_field_protected, apply_custom_fields}`, `Entry::new`, `Group::new`, `times.touch()`.
- Produces (Task 3) :
  - `#[derive(Deserialize, Clone, Debug)] #[serde(rename_all = "camelCase")] pub struct ResolvedImport { #[serde(flatten)] pub entry: ImportedEntry, #[serde(default)] pub replace_uuid: Option<String> }`
  - `#[derive(Serialize, Debug, PartialEq, Default)] pub struct ImportResult { pub imported: usize, pub updated: usize, pub skipped: usize }`
  - `pub fn apply_import(kf: &mut KeePassFile, entries: Vec<ResolvedImport>) -> Result<ImportResult, String>`
- Règles : avec `replace_uuid` trouvé → titre, identifiant, mot de passe (protégé), URL, notes, tags (`join(",")`), `otp` (protégé, via `normalize_totp`, retiré si `totp` vide) et `apply_custom_fields` sur l'entrée existante, dossier inchangé, `times.touch()`, `updated += 1` ; sinon nouvelle entrée (`Entry::new`, donc nouvel UUID) dans le dossier `group` (chaque segment non vide cherché par `name` parmi les enfants, créé par `Group::new` s'il manque), `imported += 1` ; entrée sans titre, identifiant, mot de passe ni URL → `skipped += 1`.

- [ ] **Step 1: Tests qui échouent**

```rust
#[test] fn roundtrip_json_keeps_every_field()
    // coffre A : login (otp, tags ["perso","web"], notes multi-ligne) dans "Perso/Banque",
    // carte (MyPass_Type=card, CC_Number, CC_CVC), identité (ID_*), document (DOC_*), clé SSH (SSH_*),
    // entrée avec KPEX_PASSKEY_CREDENTIAL_ID, entrée à la racine.
    // export(A, Json, &[]) → parse_import → apply_import(B vide, sans replace) →
    // pour chaque entrée de A, une entrée de B a les mêmes title/username/password/url/notes/tags/otp,
    // les mêmes customFields et le même chemin de dossier ; result == ImportResult { imported: 7, updated: 0, skipped: 0 }
#[test] fn roundtrip_csv_keeps_its_columns()
    // même coffre A, Csv → titre, identifiant, mot de passe, URL, notes, otp, tags, dossier identiques
#[test] fn apply_import_creates_group_once()   // 2 entrées "Perso/Banque" → un seul "Perso", un seul "Banque" dessous, 2 entrées dedans
#[test] fn apply_import_replace_updates_in_place() // replace_uuid = uuid existant → nombre d'entrées inchangé, mot de passe mis à jour, LMT avancée, updated == 1
#[test] fn apply_import_unknown_replace_creates()  // replace_uuid inconnu → entrée créée, imported == 1
#[test] fn apply_import_never_reuses_uuids()       // UUID de A ∩ UUID de B (après roundtrip JSON) == ∅
#[test] fn apply_import_skips_empty()              // ImportedEntry::default() → skipped == 1
```

- [ ] **Step 2: Lancer** — `cd crates/mypass-core && cargo test transfer 2>&1 | tail -5` → Expected: FAIL à la compilation (`apply_import`, `ResolvedImport` introuvables).

- [ ] **Step 3: Implémenter** `apply_import` selon les règles ci-dessus.

- [ ] **Step 4: Lancer** — `cd crates/mypass-core && cargo test 2>&1 | grep "test result"` → Expected: `ok`, 0 failed.

- [ ] **Step 5: Commit**

```bash
git add crates/mypass-core
git commit -m "feat(core): apply imports in place or into their folders, lossless round trips"
```

### Task 3: Commandes desktop, wasm, web et mock

**Files:**
- Modify (réécrit): `src-tauri/src/commands/import_export.rs` — ne garde que `parse_import`, `import_entries`, `export_entries`
- Modify: `src-tauri/src/lib.rs` (bloc `// Import/Export` de `generate_handler!`), `src-tauri/Cargo.toml` (retirer `csv`)
- Modify: `crates/mypass-wasm/src/lib.rs` (3 bindings), `crates/mypass-wasm/smoke.mjs`
- Modify: `src/lib/web.ts` (3 cas), `src/lib/mock.ts` (3 cas ; retirer `import_csv`, `import_google`, `import_apple`, `import_1password`, `import_bitwarden`, `import_protonpass`, `export_csv`, `export_json`, `get_entries_for_dedup` et `parseCsvLine` s'il devient inutilisé)

**Interfaces:**
- Consumes: `ops::transfer::{ImportedEntry, ResolvedImport, ImportResult, ExportFormat, export, parse_import, apply_import}` (Tasks 1-2).
- Produces (Task 4), IPC :
  - `parse_import({ content: string }) → ImportedEntry[]`
  - `import_entries({ entries: ResolvedImport[] }) → { imported, updated, skipped }`
  - `export_entries({ format: "json" | "csv", uuids: string[], fileName: string }) → { saved: boolean }`
- Desktop `export_entries(app: tauri::AppHandle, state, format: String, uuids: Vec<String>, file_name: String) -> Result<ExportOutcome, String>` avec `#[derive(Serialize)] struct ExportOutcome { saved: bool }` : contenu généré dans un bloc qui prend puis relâche le verrou ; puis `tauri::async_runtime::spawn_blocking(move || app.dialog().file().add_filter(<"JSON"|"CSV">, &[<"json"|"csv">]).set_file_name(&file_name).blocking_save_file())` ; `None` → `saved: false` ; sinon `std::fs::write(path.into_path()?, content)` → `saved: true`.
- Desktop `import_entries` : verrou → `apply_import` → `db.save()?` (même patron que `create_entry`).
- Wasm : `parse_import(content: &str) -> Result<String, String>` (JSON du tableau), `import_entries(entries_json: &str) -> Result<String, String>` (`with_kf_mut`), `export_entries(format: &str, uuids_json: &str) -> Result<String, String>` (contenu, `with_kf`).
- Web : `parse_import` → `JSON.parse(wasm.parse_import(…))` ; `import_entries` → wasm puis `schedulePush()` ; `export_entries` → contenu wasm dans un `Blob` (`application/json` ou `text/csv`), lien `<a download={fileName}>` cliqué puis URL révoquée → `{ saved: true }`.
- Mock : `parse_import` → 2 `ImportedEntry` factices ; `import_entries` → entrées ajoutées à `mockStore.entries`, `{ imported, updated: 0, skipped: 0 }` ; `export_entries` → `{ saved: true }`.

- [ ] **Step 1: RED wasm** — ajouter à `smoke.mjs`, sur le coffre ouvert du smoke : `export_entries("json", "[]")` → `parse_import` → longueur = nombre d'entrées ; `import_entries` de ce tableau → `imported` = cette longueur ; `parse_import("foo,bar\n1,2")` lève `CSV_NO_HEADER`.
Run: `npm run build:wasm > /dev/null 2>&1; npm run smoke:wasm` → Expected: échec à l'import des fonctions (bindings absents).

- [ ] **Step 2: Implémenter** desktop, wasm, web, mock selon les Interfaces ; supprimer les anciennes commandes et structs (`ImportPreviewEntry`, `CsvColumnMapping`, `DedupEntry`, `ResolvedEntry`, `import_csv_internal`, `collect_all_entries`, l'ancien `ImportResult`) ; retirer `csv` de `src-tauri/Cargo.toml`.

- [ ] **Step 3: GREEN**

Run: `npm run build:wasm > /dev/null 2>&1 && npm run smoke:wasm` → Expected: toutes les lignes `ok`.
Run: `cd src-tauri && cargo test 2>&1 | grep "test result" && cargo clippy 2>&1 | grep -c '^warning'` → Expected: `ok` ; ≤ référence.
Run: `grep -rn "import_csv\|import_1password\|import_bitwarden\|import_google\|import_apple\|import_protonpass\|preview_csv_import\|read_file_content\|export_csv\|export_json\|get_entries_for_dedup" src src-tauri/src` → Expected: rien.

- [ ] **Step 4: Commit**

```bash
git add -A src-tauri crates/mypass-wasm src/lib Cargo.lock
git commit -m "feat: parse_import / import_entries / export_entries on desktop, PWA and mock"
```

### Task 4: Interface d'import et d'export

**Files:**
- Modify: `src/types/import.ts`, `src/lib/dedup.ts`, `src/components/import-export/ImportWizard.tsx`, `src/components/import-export/ExportDialog.tsx`, `src/components/import-export/DedupDialog.tsx` (si le type change), `src/i18n/en.json`, `src/i18n/fr.json`, `README.md`

**Interfaces:**
- Consumes: IPC de Task 3.
- Produces: types TS `ImportedEntry` (champs de Global Constraints), `ParsedEntry = ImportedEntry & { tempId: string }`, `ResolvedEntry = ImportedEntry & { replaceUuid?: string }`, `ImportResult = { imported: number; updated: number; skipped: number }`, `ExportFormat = "json" | "csv"` ; `DuplicateEntry` gagne `group: string` (`""` côté coffre). Supprimés : `ImportFormat`, `ImportPreview`, `ImportPreviewEntry`, `ColumnMapping`, `parseCsvToEntries`, `parseJsonToEntries`, `parseCsvLine`.

- [ ] **Step 1: Types et `dedup.ts`** — `resolveDuplicates` : entrée choisie `source === "vault"` → rien d'envoyé (comptée dans `totalDiscarded`) ; choisie `source === "import"` et groupe contenant une entrée du coffre → envoyée avec `replaceUuid` = `uuid` de la première entrée du coffre du groupe ; groupe sans entrée du coffre → envoyée sans `replaceUuid`. `vaultEntryToDupEntry` lit la forme de `get_entries`.

- [ ] **Step 2: `ImportWizard.tsx`** — un seul bouton « Importer un fichier » (`accept=".csv,.json"`) ; `file.text()` → `parse_import` → `tempId` par index → `get_entries` (plus de `get_entries_for_dedup` ni de repli) → dédoublonnage → `import_entries` ; erreurs : message `/^[A-Z_]+$/` et clé `import.errors.<CODE>` existante → traduit, sinon message brut ; les 3 textes en dur → `import.errors.IMPORT_EMPTY`, `import.analyzeFailed`, `import.importFailed` ; écran final « N ajoutées, M mises à jour, K ignorées » ; invalidation des clés de requête des entrées et des groupes.

- [ ] **Step 3: `ExportDialog.tsx`** — JSON en premier et par défaut, puis CSV avec la mention sous la tuile ; `export_entries({ format, uuids, fileName: \`mypass-export-${new Date().toISOString().slice(0, 10)}.${format}\` })` ; `saved: false` → rester sur le dialogue sans erreur.

- [ ] **Step 4: i18n et README** — EN + FR : `import.pickFile`, `import.pickFileDesc` (« CSV de Google, Apple, KeePassXC, Bitwarden… ou export JSON de MyPass »), `import.errors.{IMPORT_EMPTY,CSV_NO_HEADER,JSON_NOT_MYPASS}`, `import.analyzeFailed`, `import.importFailed`, `import.updated`, `import.skipped`, `export.csvHint` ; retirer `import.csv`, `import._1pux`, `import.bitwarden`, `import.google`, `import.apple`, `import.proton`. README : import « CSV (Google, Apple, KeePassXC, Bitwarden…) et JSON MyPass », export « JSON, CSV ».

- [ ] **Step 5: Vérifier**

Run: `npx tsc -b && npm run lint` → Expected: aucune erreur, 0 warning.
Run: `grep -rn "Aucune entrée trouvée\|Échec de l'analyse\|Échec de l'import" src` → Expected: rien.
Manuel (`npm run dev`, mock) : Import → un seul bouton ; Export → JSON sélectionné par défaut, mention sous CSV.

- [ ] **Step 6: Commit**

```bash
git add -A src README.md
git commit -m "feat(front): one import button, real dedup, JSON/CSV export dialog"
```

### Task 5: E2E isolé et fin du sous-projet

**Files:**
- Modify: `docs/superpowers/specs/2026-10-02-roadmap-design.md` (résultat + suivi)

**Interfaces:**
- Consumes: tout ce qui précède.
- Produces: rien.

- [ ] **Step 1: Non-régression** — `cargo test` (`crates/mypass-core`, `src-tauri`, `server`), `cargo clippy` ≤ référence, `npm run lint`, `npm run build`, `npm run build:web`, `npm run build:wasm && npm run smoke:wasm` → Expected: tout vert.

- [ ] **Step 2: Desktop isolé** — `APPDATA=<dossier temporaire>` et `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` pour `npm run tauri dev -- --config '{"identifier":"com.mypass.e2e"}'` ; sauvegarder puis restaurer `%APPDATA%\MyPass\mypass-nhm.bat` s'il change. Via CDP (`__TAURI_INTERNALS__.invoke`) : créer un coffre de test, une entrée login avec `otp`, une carte, une entrée dans `Perso/Banque`. **Luca** clique Exporter → JSON → « Enregistrer sous » dans un chemin donné, puis rouvre l'export et clique Annuler.
Expected: fichier JSON écrit avec les 3 entrées ; annulation → dialogue resté ouvert, aucune erreur.

- [ ] **Step 3: PWA locale** — `npm run build:web` ; `mypass-server` local (`cargo run` dans `server/`, `MYPASS_DATA_DIR` temporaire, `MYPASS_STATIC_DIR` = `dist`, `MYPASS_BIND=127.0.0.1:<port libre>`), jeton lu dans sa sortie ; dans Chrome, coffre vide ; importer le JSON du Step 2 (`file_upload`).
Expected: 3 entrées, mêmes champs (TOTP, carte, dossier `Perso/Banque`) ; « 3 ajoutées ».

- [ ] **Step 4: Doublons** — exporter en CSV dans la PWA (contenu capturé en interceptant le clic du lien, sans téléchargement réel), le réimporter.
Expected: doublons proposés ; tout « garder celle du coffre » → « 0 ajoutée, 0 mise à jour », toujours 3 entrées.

- [ ] **Step 5: Nettoyage** — arrêter `tauri dev` et le serveur local, supprimer leurs dossiers temporaires et `%LOCALAPPDATA%\com.mypass.e2e`.

- [ ] **Step 6: Commit** (feuille de route : résultat du sous-projet 5, suivi `[x] 2026-10-02`)

```bash
git add docs/superpowers/specs/2026-10-02-roadmap-design.md
git commit -m "docs: roadmap — sub-project 5 delivered"
```
