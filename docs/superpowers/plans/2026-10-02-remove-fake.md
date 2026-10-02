# Retrait du factice et du code mort — Implementation Plan (sous-projet 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Plus aucune donnée inventée ni bouton inerte dans l'app réelle, et le code qui ne sert pas supprimé.

**Architecture:** Suppressions ciblées : passkeys (UI, IPC, protocole navigateur), code mort Rust et TS, Firefox, `extension/`. Le noyau `mypass-core` ne change pas ; un test de caractérisation verrouille la conservation des champs `KPEX_PASSKEY_*`. Le backend mock quitte `src/lib/tauri.ts` pour `src/lib/mock.ts`, chargé seulement si `import.meta.env.DEV`. Les blanket `allow(dead_code)` de `src-tauri/src/lib.rs` disparaissent pour que le compilateur serve de garde-fou.

**Tech Stack:** Rust (Tauri 2, `mypass-core`), React 19 + react-router + Zustand + react-i18next, Vite 6.

**Spec:** `docs/superpowers/specs/2026-10-02-remove-fake-design.md`

## Global Constraints

- **Conserver** `has_passkey` et le filtrage `KPEX_PASSKEY_*` de `crates/mypass-core/src/ops/entries.rs` : aucune donnée de coffre détruite.
- `ADVERTISED_VERSION` (`native_messaging.rs`) reste `"2.7.10"`.
- Code d'erreur `NO_BACKEND`, copie verbatim : EN « MyPass backend not found — reinstall the app », FR « Backend MyPass introuvable — réinstallez l'application » (clé `unlock.errors.NO_BACKEND`).
- Destination de la clé : `C:\Users\lucaa\Documents\Projet\MyPass-hors-depot\extension-dev-key.pem` ; déplacée, présence et hash vérifiés avant de supprimer `extension/`.
- Parité : toute commande retirée de `generate_handler!` est retirée des cas mock (`src/lib/tauri.ts` puis `src/lib/mock.ts`) et web (`src/lib/web.ts`).
- Chaînes UI en EN + FR (`src/i18n/en.json`, `src/i18n/fr.json`), jamais en dur.
- `cargo clippy` (`src-tauri`) sans nouveau warning : référence mesurée avant Task 1 (`cargo clippy 2>&1 | grep -c '^warning'`, attendu 15), notée au ledger.
- Aucun test ne touche le serveur de prod ni un vrai coffre.
- Rust : `export PATH="$HOME/.cargo/bin:$PATH"` dans les shells qui ne l'ont pas.
- Après chaque build, `src-tauri/Cargo.toml` et `src-tauri/gen/schemas/*.json` peuvent changer de fins de ligne seulement : vérifier `git diff --ignore-cr-at-eol --stat` vide, puis `git checkout --` ces fichiers ; ne jamais les committer.

## Review Focus

- Coffre importé de KeePassXC avec des passkeys, entrée modifiée depuis MyPass puis synchronisée → champs `KPEX_PASSKEY_*` intacts (test `passkey_fields_survive_update_and_save`, Task 1).
- Extension avec l'option passkeys activée → `passkeys-get` tombe dans le bras `_ =>` (`Unknown action`, code 0), la boucle native continue (lecture du `match` dans `handle_action`, Task 1 ; pas de test : `handle_action` lit la config navigateur sur disque).
- Build de production ouverte hors Tauri (fichier `dist/` servi par un navigateur) → message `NO_BACKEND` traduit, aucune entrée factice (vérification `vite preview`, Task 3).
- `npm run dev` → le mock marche toujours : coffre factice, entrées (vérification manuelle, Task 3).
- « Mettre à jour » cliqué alors qu'un filtre de type (Cartes…) est actif → l'entrée s'ouvre quand même (`setKindFilter(null)` avant `selectEntry`, vérification manuelle Task 4).

---

### Task 1: Passkeys

**Files:**
- Test: `crates/mypass-core/src/ops/entries.rs` (module `tests`)
- Delete: `src-tauri/src/commands/passkeys.rs`, `src/views/PasskeysView.tsx`, `src/components/passkeys/` (dossier)
- Modify: `src-tauri/src/commands/mod.rs` (`pub mod passkeys;`), `src-tauri/src/lib.rs:91-96` (commentaire `// Passkeys` + 5 commandes), `src-tauri/src/native_messaging.rs` (doc l.24, bras l.391-392, `handle_passkeys_register` et `handle_passkeys_get` l.842-865)
- Modify: `src/App.tsx:7,24`, `src/components/layout/AppSidebar.tsx:132,168` (+ `Key` dans l'import l.11 s'il devient inutilisé), `src/hooks/useKeyboardShortcuts.ts:31`, `src/lib/tauri.ts` (cas `list_passkeys` l.282-286, `import_passkeys` l.426-427), `src/lib/web.ts:348-349`, `src/types/entry.ts` (champ `passkey?` l.12, interface `PasskeyData` l.31-38), `src/types/index.ts:1` (`type PasskeyData`), `src/i18n/en.json` + `fr.json` (`nav.passkeys`, bloc `passkeys`)

**Interfaces:**
- Consumes: `ops::entries::{update, UpdateEntry, find_entry, entry_to_info, set_string_field, set_string_field_protected}`, `crate::writer::write_database_bytes`, `crate::reader::read_database_bytes`, `crate::crypto::Cipher`, `crate::keys::KdfParams`.
- Produces: IPC sans `list_passkeys`, `register_passkey`, `authenticate_passkey`, `export_passkeys`, `import_passkeys` ; plus de type TS `PasskeyData`.

- [ ] **Step 1: Écrire le test de caractérisation** dans `tests` de `entries.rs`

```rust
#[test]
fn passkey_fields_survive_update_and_save() {
    let mut kf = xml::KeePassFile::new("Test Vault");
    let mut entry = Entry::new("GitHub", "dev", "pw", "https://github.com");
    set_string_field(&mut entry, "KPEX_PASSKEY_CREDENTIAL_ID", "cred-id");
    set_string_field_protected(&mut entry, "KPEX_PASSKEY_PRIVATE_KEY_PEM", "pem-blob");
    let uuid = entry.uuid.clone();
    kf.root.group.entries.push(entry);

    let mut fields = HashMap::new();
    fields.insert("Note".to_string(), "x".to_string());
    update(&mut kf, &uuid, UpdateEntry {
        title: Some("GitHub perso".to_string()), username: None, password: None, url: None,
        notes: None, tags: None, group_uuid: None, custom_fields: Some(fields),
    }).unwrap();

    let bytes = crate::writer::write_database_bytes(
        &kf, "pw", None, crate::crypto::Cipher::Aes256, &crate::keys::KdfParams::default(),
    ).unwrap();
    let back = crate::reader::read_database_bytes(&bytes, "pw", None).unwrap().keepass_file;
    let e = find_entry(&back.root.group, &uuid).unwrap();
    let value = |k: &str| e.strings.iter().find(|s| s.key == k).map(|s| s.value.content.clone());
    assert_eq!(value("KPEX_PASSKEY_CREDENTIAL_ID").as_deref(), Some("cred-id"));
    assert_eq!(value("KPEX_PASSKEY_PRIVATE_KEY_PEM").as_deref(), Some("pem-blob"));
    assert_eq!(e.title(), "GitHub perso");
    assert!(entry_to_info(e, "").has_passkey);
}
```

- [ ] **Step 2: Lancer le test**

Run: `cd crates/mypass-core && cargo test passkey_fields_survive_update_and_save`
Expected: PASS dès maintenant — test de caractérisation (le noyau ne change pas dans ce plan) ; un échec est un vrai bug de conservation, à traiter avant de continuer.

- [ ] **Step 3: Retirer les passkeys côté Rust** (fichiers ci-dessus). Le bras `_ =>` de `handle_action` répond alors `Unknown action: passkeys-get`.

- [ ] **Step 4: Retirer les passkeys côté front** (fichiers ci-dessus). `Mod+2` disparaît sans renuméroter les autres raccourcis.

- [ ] **Step 5: Vérifier**

Run: `cd src-tauri && cargo test 2>&1 | tail -3` → Expected: `test result: ok`, 0 failed.
Run: `npx tsc -b && npm run lint` → Expected: aucune erreur, 0 warning.
Run: `grep -rni passkey src src-tauri/src` → Expected: une seule ligne, le commentaire de `ADVERTISED_VERSION` dans `native_messaging.rs` (« passkeys ≥ 2.7.7 »).

- [ ] **Step 6: Commit**

```bash
git add -A crates/mypass-core/src/ops/entries.rs src-tauri/src src/
git commit -m "feat: remove the fake passkeys feature (vault KPEX_PASSKEY_* fields are kept)"
```

### Task 2: Code mort Rust

**Files:**
- Modify: `src-tauri/src/lib.rs` (l.1 `#![allow(dead_code)]`, l.3 `#[allow(dead_code)]` sur `mod commands`, l.6 sur `pub mod security`, `greet` l.15-18 et sa ligne de `generate_handler!`)
- Delete: `src-tauri/src/security/hibp.rs`, `src-tauri/src/security/zxcvbn.rs` (+ leurs `pub mod` dans `security/mod.rs`)
- Modify: `src-tauri/src/native_messaging.rs:114-115` (champ `id` de `NativeRequest` et son `#[serde(default)]`), `src-tauri/src/commands/import_export.rs:33` (`CsvColumnMapping.totp`), `:441-447` (`ResolvedEntry.tags`, `custom_fields`, `totp`)

**Interfaces:**
- Consumes: rien des autres tâches.
- Produces: crate `src-tauri` sans `allow(dead_code)` global ; IPC sans `greet`.

- [ ] **Step 1: RED — retirer les trois `allow(dead_code)` de `lib.rs`, puis**

Run: `cd src-tauri && cargo check --lib --message-format short 2>&1 | grep -E "never (used|read|constructed)"`
Expected: 3 lignes — `id` (`native_messaging.rs`), `totp` (`import_export.rs:33`), `tags`/`custom_fields`/`totp` (`import_export.rs:441`). Toute ligne en plus est traitée selon la règle de la spec (suppression si inutilisé, sinon `allow` ciblé et commenté) et notée au ledger.

- [ ] **Step 2: Supprimer** `greet` (fonction + ligne du handler), `hibp.rs`, `zxcvbn.rs` et leurs `pub mod`, le champ `id`. Ne retirer aucune dépendance Cargo : `reqwest` sert encore à `sync.rs`, `zxcvbn.rs` n'en importe aucune.

- [ ] **Step 3: `allow` ciblés** sur `CsvColumnMapping.totp` et sur les trois champs de `ResolvedEntry`, avec le commentaire : `// Received but not written to the vault yet (data loss on import) — roadmap sub-project 5.`

- [ ] **Step 4: GREEN**

Run: `cd src-tauri && cargo check --lib --message-format short 2>&1 | grep -cE "never (used|read|constructed)"` → Expected: `0`.
Run: `cargo test 2>&1 | tail -3` → Expected: `test result: ok`.
Run: `cargo clippy 2>&1 | grep -c '^warning'` → Expected: ≤ la référence du ledger.

- [ ] **Step 5: Commit**

```bash
git add -A src-tauri/src
git commit -m "chore(desktop): drop dead Rust code and the crate-wide dead_code allows"
```

### Task 3: Code mort front et mock réservé au dev

**Files:**
- Create: `src/lib/mock.ts`
- Modify: `src/lib/tauri.ts` (en-tête l.1-6, imports l.7-8, fin de `getInvoke` l.70-73, bloc mock l.76-463 déplacé)
- Modify: `src/lib/crypto.ts:57-60` (supprimer `checkHibp`)
- Delete: `src/lib/index.ts`
- Modify: `src/i18n/en.json`, `src/i18n/fr.json` (`unlock.errors.NO_BACKEND`)

**Interfaces:**
- Consumes: `TauriInvokeFn` (type exporté par `tauri.ts`).
- Produces: `export function createMockInvoke(): TauriInvokeFn` dans `src/lib/mock.ts` ; `getInvoke()` lève `Error("NO_BACKEND")` hors Tauri, hors mode web et hors dev.

- [ ] **Step 1: RED — le mock est dans la build de production**

Run: `npm run build > /dev/null 2>&1; grep -rl "mock-vault.kdbx" dist`
Expected: un fichier `dist/assets/index-*.js`.

- [ ] **Step 2: Déplacer le mock** : tout le bloc « Mock backend for browser-only development » (`mockStore`, `createMockInvoke` désormais exporté, `parseCsvLine`) passe dans `src/lib/mock.ts`, avec `import type { Entry } from "@/stores/entriesStore"`, `import type { Group } from "@/types/group"` et `import type { TauriInvokeFn } from "./tauri"`. Dans `tauri.ts`, retirer les imports devenus inutiles, mettre l'en-tête à jour (le mock ne sert qu'à `npm run dev`) et finir `getInvoke` par :

```ts
  if (import.meta.env.DEV) {
    console.info("[MyPass] Browser dev mode. Using mock backend.");
    const { createMockInvoke } = await import("./mock");
    _invoke = createMockInvoke();
    return _invoke;
  }

  throw new Error("NO_BACKEND");
```

- [ ] **Step 3: Supprimer** `checkHibp` et `src/lib/index.ts` ; ajouter `unlock.errors.NO_BACKEND` (copie des Global Constraints).

- [ ] **Step 4: GREEN**

Run: `npm run build > /dev/null 2>&1; echo $?; grep -rl "mock-vault.kdbx" dist; grep -l "mock-vault.kdbx" src/lib/mock.ts`
Expected: `0`, aucun fichier de `dist/`, puis `src/lib/mock.ts`.
Run: `npm run lint` → Expected: 0 warning.

- [ ] **Step 5: Production hors Tauri** — `npx vite preview --port 4173` (sert le `dist/` desktop de l'étape 4), ouvrir `http://localhost:4173` dans Chrome.
Expected: écran « aucun coffre » avec « Backend MyPass introuvable — réinstallez l'application », aucune entrée. Arrêter le serveur.

- [ ] **Step 6: Build web**

Run: `npm run build:web > /dev/null 2>&1; echo $?; grep -rl "mock-vault.kdbx" dist`
Expected: `0`, aucun fichier.

- [ ] **Step 7: Le mock marche en dev** — `npm run dev`, ouvrir `http://localhost:1420`, créer un coffre factice.
Expected: entrées factices affichées. Arrêter le serveur.

- [ ] **Step 8: Commit**

```bash
git add -A src/lib src/i18n
git commit -m "fix(front): the mock backend only exists in npm run dev"
```

### Task 4: Boutons inertes

**Files:**
- Modify: `src/views/SecurityView.tsx` (`PasswordList`, bouton `common.update` ; import `useNavigate` de `react-router-dom`)
- Modify: `src/components/layout/AppSidebar.tsx:167,169` (icônes repliées « Tous les éléments » et « Sécurité », sans `onClick` aujourd'hui)

**Interfaces:**
- Consumes: `useEntriesStore` (`selectEntry(uuid: string | null)`, `setKindFilter(kind)`), `useNavigate`.
- Produces: rien.

- [ ] **Step 1: « Mettre à jour »** — `onClick={() => { setKindFilter(null); selectEntry(e.uuid); navigate("/"); }}` dans cet ordre (`setKindFilter` remet `selectedEntryId` à `null`).

- [ ] **Step 2: Icônes repliées** — mêmes `onClick` que les items étendus : « Tous les éléments » = `navigate("/"); setKindFilter(null); selectEntry(null);` (l.127), « Sécurité » = `navigate("/security")` (l.134). Hors spec, même intention (« aucun bouton qui ne fait rien ») : noter au ledger.

- [ ] **Step 3: Vérifier**

Run: `npx tsc -b && npm run lint` → Expected: aucune erreur, 0 warning.
Manuel (`npm run dev`, coffre factice) : filtre « Cartes » actif → Sécurité → « Mettre à jour » sur une entrée → la vue principale s'ouvre sur cette entrée ; barre repliée → les deux icônes naviguent.

- [ ] **Step 4: Commit**

```bash
git add src/views/SecurityView.tsx src/components/layout/AppSidebar.tsx
git commit -m "fix(front): Update opens the entry; collapsed sidebar icons navigate"
```

### Task 5: Firefox, `extension/`, README, fin du sous-projet

**Files:**
- Modify: `register-nhm.ps1` (bloc Firefox l.13-19, message final l.22 → « Chrome, Edge »)
- Modify: `src-tauri/src/commands/browser.rs:87` (`"allowed_extensions"` et la virgule qui le précède)
- Modify: `README.md` (l.13 « , Passkeys », l.15 « (Chrome, Edge) », l.26 supprimée)
- Delete: `extension/` (après déplacement de `dev-key.pem`)
- Modify: `docs/superpowers/specs/2026-10-02-roadmap-design.md` (suivi : sous-projet 2 fait)

**Interfaces:**
- Consumes: rien.
- Produces: rien.

- [ ] **Step 1: Firefox** — modifications ci-dessus.
Run: `grep -rni "firefox\|allowed_extensions" register-nhm.ps1 src-tauri/src` → Expected: rien.

- [ ] **Step 2: Clé `.pem`** (PowerShell)

```powershell
$dst = "C:\Users\lucaa\Documents\Projet\MyPass-hors-depot\extension-dev-key.pem"
$h = (Get-FileHash extension\dev-key.pem).Hash
New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
Move-Item extension\dev-key.pem $dst
(Get-FileHash $dst).Hash -eq $h
```
Expected: `True`. Sinon, arrêter : ne rien supprimer.

- [ ] **Step 3: Supprimer `extension/`** — `git rm -r -q extension`, puis vérifier que le dossier n'existe plus.

- [ ] **Step 4: README** — modifications ci-dessus.

- [ ] **Step 5: Non-régression complète**

Run: `cd crates/mypass-core && cargo test 2>&1 | tail -2`, `cd src-tauri && cargo test 2>&1 | tail -2`, `cd server && cargo test 2>&1 | grep "test result"` → Expected: tout `ok`.
Run: `cd src-tauri && cargo clippy 2>&1 | grep -c '^warning'` → Expected: ≤ référence.
Run: `npm run lint && npm run build && npm run build:web` → Expected: succès.

- [ ] **Step 6: Manuel** — `npm run tauri dev` : pas d'entrée Passkeys (barre étendue et repliée), `Ctrl+2` sans effet, Sécurité → « Mettre à jour » ouvre l'entrée.

- [ ] **Step 7: Commit** (suivi de la feuille de route : sous-projet 2 `[x] 2026-10-02`)

```bash
git add -A register-nhm.ps1 src-tauri/src/commands/browser.rs README.md docs/superpowers/specs/2026-10-02-roadmap-design.md
git commit -m "chore: Chrome-only browser registration, drop the obsolete extension/ folder"
```
