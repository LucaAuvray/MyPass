# Import / export — Design (sous-projet 5)

> Sous-projet 5 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-02.

## Intention

Luca sort ses données de MyPass et les y rentre sans rien perdre, sur desktop comme sur PWA :
un export JSON réimporté dans un coffre vide redonne exactement les mêmes entrées, et un CSV
venu de Google, Apple, KeePassXC ou Bitwarden atterrit dans les bons champs.

**Succès :** export JSON puis réimport dans un coffre vide sans perte de champ, sur desktop et
PWA ; un CSV Google / Apple / KeePassXC / Bitwarden importé met chaque colonne dans le bon champ ;
un doublon avec une entrée du coffre ne crée jamais de deuxième copie.

## Constat de départ

- Le CSV exporté (`Title, Username, Password, URL…`) est relu par position comme
  `titre, url, utilisateur, mdp` (`src/lib/dedup.ts:93`) : URL et identifiant s'inversent. Le JSON
  exporté n'est réimportable par aucun format.
- Doublon avec une entrée du coffre : `resolveDuplicates` (`dedup.ts:325`) renvoie l'entrée choisie
  et `import_entries` l'**ajoute** toujours, même si c'est celle du coffre → deux copies.
- Perdus : TOTP, tags, champs personnalisés et dossiers à l'import (`import_entries` ne les écrit
  pas) ; TOTP vide, et ni tags ni champs dans le JSON, à l'export.
- `.1pux` est une archive ZIP lue comme du texte : l'import 1Password ne marche pas. Le parseur
  CSV coupe sur `\n` (notes multi-lignes cassées) et ne gère pas `""`.
- Export desktop écrit dans le dossier courant de l'app (`ExportDialog.tsx:36`), sans dialogue.
  PWA : import et export tombent dans « Commande indisponible en mode web ».
- 8 commandes Rust d'import ne sont appelées par personne (`preview_csv_import`, `import_csv`,
  `import_1password`, `import_bitwarden`, `import_google`, `import_apple`, `import_protonpass`,
  `read_file_content`) ; `get_entries_for_dedup` renvoie des champs personnalisés vides.
- Textes en dur dans `ImportWizard.tsx` : « Aucune entrée trouvée dans le fichier »,
  « Échec de l'analyse », « Échec de l'import ».

## Décisions

| Sujet | Décision |
|---|---|
| Où vivent les formats | Dans `mypass-core` : export, lecture et écriture au même endroit, partagés desktop + wasm, testés en Rust. |
| Formats d'import | CSV lu par en-têtes (Google, Apple, KeePassXC, Bitwarden CSV, MyPass CSV) + JSON MyPass. Bitwarden JSON, Proton Pass JSON et 1Password retirés. |
| Formats d'export | JSON MyPass complet (réimportable sans perte) + CSV (sans champs personnalisés). |
| Doublon avec le coffre | Une seule entrée reste : garder celle du coffre = rien n'est importé ; garder l'importée = l'entrée du coffre est mise à jour sur place. |
| Dossiers | Exportés (chemin) et recréés à l'import. |
| Hors périmètre | Pièces jointes, historique, icônes, dates de création/modification (le `.kdbx` est la sauvegarde complète). |

## Design

### 1. Formats

**Entrée importée** (`ImportedEntry`, forme unique : fichier JSON, IPC, wasm ; champs en camelCase) :
`group` (chemin depuis la racine, segments séparés par `/`, `""` = racine), `title`, `username`,
`password`, `url`, `notes`, `tags: string[]`, `totp` (`""` si aucun), `customFields:
Record<string,string>` (tous les autres champs texte de l'entrée : `MyPass_Type`, `ID_*`, `CC_*`,
`DOC_*`, `SSH_*`, `KPEX_PASSKEY_*`…).

**JSON MyPass :** `{"format":"mypass","version":1,"entries":[ImportedEntry…]}`, indenté.

**CSV exporté :** en-tête `Group,Title,Username,Password,URL,Notes,TOTP,Tags` (tags joints par `,`),
RFC 4180, sans champs personnalisés.

**CSV importé** (`csv` crate, déplacé de `src-tauri` vers `mypass-core`) :
- BOM UTF-8 ignoré ; séparateur `;` si la ligne d'en-tête contient plus de `;` que de `,`, sinon `,`.
- Colonnes reconnues par nom, sans casse ni espaces en bord :

| Champ | En-têtes acceptés |
|---|---|
| title | `title`, `name`, `account` |
| username | `username`, `user name`, `login`, `login_username`, `user` |
| password | `password`, `login_password` |
| url | `url`, `uri`, `website`, `web site`, `login_uri` |
| notes | `notes`, `note`, `comments`, `extra` |
| totp | `totp`, `otp`, `otpauth`, `login_totp` |
| group | `group`, `folder` |
| tags | `tags` (séparés par `,` ou `;`) |

- Colonnes inconnues ignorées. Aucun des en-têtes `title`, `username`, `password`, `url` reconnu →
  erreur `CSV_NO_HEADER`.

**TOTP** stocké dans le champ `otp` (convention KeePassXC) sous forme `otpauth://` : valeur qui
commence par `otpauth://` (sans casse) gardée telle quelle ; sinon secret nettoyé (espaces retirés,
majuscules) et converti en `otpauth://totp/<titre encodé>?secret=<secret>`. À l'export, `totp` =
valeur de `otp`, sinon de `TOTP Seed`, sinon `""`. L'affichage des codes relève du sous-projet 3.

### 2. Noyau : `crates/mypass-core/src/ops/transfer.rs`

- `export(kf, format: ExportFormat, uuids: &[String]) -> Result<String, String>` —
  `ExportFormat::{Json, Csv}` ; toutes les entrées de l'arbre si `uuids` est vide, sinon celles-là ;
  les entrées du groupe `RecycleBinUUID` (coffre venu de KeePassXC) sont exclues.
- `parse_import(content: &str) -> Result<Vec<ImportedEntry>, String>` — contenu dont le premier
  caractère non blanc est `{` : JSON MyPass (`format` ≠ `"mypass"`, `version` ≠ 1 ou JSON invalide
  → `JSON_NOT_MYPASS`) ; sinon CSV. Titre vide → URL ; entrée sans titre, identifiant, mot de passe
  ni URL écartée. Aucune entrée → `IMPORT_EMPTY`.
- `apply_import(kf, entries: Vec<ResolvedImport>) -> Result<ImportResult, String>` —
  `ResolvedImport` = `ImportedEntry` + `replaceUuid: Option<String>`.
  - Avec `replaceUuid` : l'entrée existante reçoit titre, identifiant, mot de passe, URL, notes,
    tags, `otp` et les champs personnalisés (upsert via `apply_custom_fields`) ; son dossier ne change
    pas ; `times.touch()` pour que la sync la fasse gagner. Compte dans `updated`.
  - Sinon : nouvelle entrée, **toujours avec un nouvel UUID**, dans le dossier `group` (chaque segment
    trouvé par nom sous son parent, créé s'il manque). Compte dans `imported`.
  - `ImportResult { imported, updated, skipped }` ; `replaceUuid` introuvable → entrée créée.

### 3. Commandes et parité

| Commande | Desktop (`src-tauri`) | Web (`web.ts` + wasm) | Mock |
|---|---|---|---|
| `parse_import(content) → ImportedEntry[]` | `ops::transfer::parse_import` | binding wasm | 2 entrées factices |
| `import_entries(entries: ResolvedImport[]) → ImportResult` | verrou → `apply_import` → `save()` | binding wasm puis `schedulePush()` | `{imported: n, updated: 0, skipped: 0}` |
| `export_entries(format, uuids) → { saved: boolean }` | contenu généré sous verrou, verrou relâché, puis dialogue « Enregistrer sous » Rust (`tauri_plugin_dialog::DialogExt`, filtre `.json`/`.csv`, nom `mypass-export-AAAA-MM-JJ.<ext>`), écriture du fichier ; annulé → `saved: false` | binding wasm → `Blob` téléchargé sous le même nom → `saved: true` | `{saved: true}` |

- Le dialogue est ouvert côté Rust : le chemin vient de l'utilisateur, aucune commande n'écrit à
  un chemin fourni par le front, aucune permission `fs` à ajouter.
- Supprimés : les 8 commandes mortes, `export_csv`, `export_json`, `get_entries_for_dedup` (Rust,
  `generate_handler!`, mock) et leurs structs ; dans `dedup.ts`, `parseCsvToEntries`,
  `parseJsonToEntries`, `parseCsvLine`. Les `allow(dead_code)` ciblés du sous-projet 2 disparaissent
  avec `CsvColumnMapping` et `ResolvedEntry`.
- Le dédoublonnage compare aux entrées de `get_entries` (déjà complètes, disponible en web).

### 4. Interface

- **Import** (`ImportWizard.tsx`) : un seul bouton « Importer un fichier » (`accept=".csv,.json"`),
  à la place des 6 tuiles ; contenu lu par `file.text()` puis `parse_import`.
- **Doublons** (`resolveDuplicates`) : groupe dont l'entrée choisie vient du coffre → rien n'est
  envoyé ; entrée choisie importée et groupe contenant une entrée du coffre → envoyée avec
  `replaceUuid` = la première entrée du coffre du groupe ; groupe sans entrée du coffre → envoyée
  sans `replaceUuid`.
- **Export** (`ExportDialog.tsx`) : JSON (par défaut) ou CSV ; mention sous CSV « Sans les champs
  des cartes, identités, documents et clés SSH : utilisez JSON pour tout garder » ; `saved: false`
  laisse le dialogue ouvert sans erreur.
- **Erreurs** traduites (`import.errors.<CODE>`, EN + FR) : `IMPORT_EMPTY`, `CSV_NO_HEADER`,
  `JSON_NOT_MYPASS` ; les 3 textes en dur passent en i18n.
- **Résultat** : « N ajoutées, M mises à jour, K ignorées ».
- Après import : invalidation des requêtes entrées et groupes (la sync suit comme pour toute écriture).
- `README.md` : formats d'import et d'export mis à jour, XML / HTML retirés.

## Tests et vérification

**Tests Rust automatisés (`mypass-core`, coffres en mémoire) :**
- Aller-retour JSON : coffre avec login + TOTP + tags, carte, identité, document, clé SSH, entrée avec
  `KPEX_PASSKEY_*`, dossiers imbriqués `Perso/Banque` → `export` → `parse_import` → `apply_import`
  dans un coffre vide → mêmes champs, mêmes chemins de dossiers.
- Aller-retour CSV : mêmes colonnes retrouvées (titre, identifiant, mot de passe, URL, notes, TOTP,
  tags, dossier).
- Un échantillon d'en-tête réel par source : Google (`name,url,username,password,note`), Apple
  (`Title,URL,Username,Password,Notes,OTPAuth`), KeePassXC (`Group,Title,Username,Password,URL,Notes,TOTP,…`),
  Bitwarden (`folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp`).
- Note multi-ligne entre guillemets, `""` dans un champ, BOM, séparateur `;`.
- `CSV_NO_HEADER`, `JSON_NOT_MYPASS`, `IMPORT_EMPTY`.
- `replaceUuid` : entrée mise à jour sur place, nombre d'entrées inchangé.
- Deux entrées du même dossier → un seul groupe créé.
- Aucune entrée importée ne reprend un UUID du fichier.
- Secret TOTP base32 nu → `otpauth://`.

**Non-régression :** `cargo test` (`crates/mypass-core`, `src-tauri`, `server`), `cargo clippy` sans
nouveau warning, `npm run lint`, `npm run build`, `npm run build:web`, `npm run smoke:wasm`.

**E2E manuel, jamais sur la prod :**
- Desktop isolé : `npm run tauri dev` avec `APPDATA` pointant vers un dossier temporaire et un
  identifiant d'app distinct → coffre de test ; créer login + TOTP, carte, dossier ; exporter en JSON
  via « Enregistrer sous ».
- PWA : `npm run build:web`, puis `mypass-server` local (`cargo run` dans `server/`,
  `MYPASS_DATA_DIR` = dossier temporaire, `MYPASS_STATIC_DIR` = `dist`, `MYPASS_BIND` =
  `127.0.0.1:<port libre>` ; `web.ts` appelle `/api` en relatif, donc la PWA doit être servie par ce
  serveur) ; coffre vide ; importer le JSON du desktop → entrées identiques ; exporter en CSV
  (téléchargement), réimporter → doublons proposés, « garder celle du coffre » → 0 ajoutée.

## Risques

- **CSV d'un autre outil aux en-têtes inattendus** : `CSV_NO_HEADER` plutôt qu'une lecture fausse ;
  ajouter un alias suffit.
- **Chemin de dossier KeePassXC** : son CSV préfixe le nom du groupe racine (`Racine/Banque`) ; un
  dossier du même nom est créé sous la racine. Acceptable (déplaçable), noté.
- **Secret en mémoire JS** : le contenu exporté transite par le front en PWA (téléchargement) ; c'est
  déjà le cas des mots de passe affichés. L'UI rappelle que le fichier n'est pas chiffré.
- **Mise à jour sur place** écrase les champs de l'entrée du coffre par ceux du fichier ; c'est le
  sens de « garder l'importée ».
