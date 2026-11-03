# Inscription assistée et clés API — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** MyPass propose un mot de passe fort sur les formulaires d'inscription, propose de façon fiable d'enregistrer le compte créé, et repère les clés API affichées pour les ranger d'un clic dans un nouveau type d'élément « Clé API ».

**Architecture:** On étend le fork KeePassXC-Browser (`keepassxc-browser/keepassxc-browser/`) et l'hôte natif de l'app (`src-tauri/src/native_messaging.rs` + `commands/browser.rs`). Les heuristiques (champ d'inscription, candidat clé API) sont des fonctions pures dans `content/detect.js`, vérifiées par un script Node. L'attente d'enregistrement vit dans `browser.storage.session` du service worker. Le type `api_key` suit le modèle des cartes (entrée KDBX + `MyPass_Type`, champ protégé dans `mypass-core`).

**Tech Stack:** Rust (Tauri v2, mypass-core), React 19 / TypeScript, extension Chrome MV3 en JS classique (scripts de contenu globaux), Node 26 pour les vérifications sans dépendance.

**Spec:** `docs/superpowers/specs/2026-11-03-signup-and-api-keys-design.md`

## Global Constraints

- Rien ne s'enregistre sans un clic de Luca ; un clic suffit.
- Ne jamais logger une clé API ni un mot de passe (`logDebug`, `console.*`, `eprintln!`, messages d'erreur).
- Aucune chaîne utilisateur en dur : app → `src/i18n/en.json` + `fr.json` ; extension → `_locales/en/messages.json` + `_locales/fr/messages.json`.
- Ne pas toucher la `key` du `manifest.json` ni `allowed_origins` de l'hôte. Le protocole KeePassXC existant reste inchangé ; `check-api-key` et `save-api-key` sont propres à MyPass.
- Mot de passe généré : 20 caractères, majuscules, minuscules, chiffres, symboles, via `mypass-core` (`generator::generate_password`).
- Attente d'enregistrement : consommée seulement par un clic (Enregistrer / Mettre à jour / Annuler), expire après 15 min ou quand l'onglet quitte le site.
- Une seule bannière à la fois (`kpxcBanner.created`).
- Commandes de non-régression (CLAUDE.md) : `cargo test` dans `src-tauri`, `crates/mypass-core`, `server` ; `cargo clippy` sans nouveau warning ; `npm run lint` ; `npm run format:check` ; `npm run build`.
- `cargo test`/clippy réécrivent `src-tauri/Cargo.toml` et `src-tauri/gen/schemas/*.json` en LF sans diff de contenu : `git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas` avant chaque commit.
- Commits courts (anglais comme l'historique), terminés par `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Ne pas pousser.

## Review Focus

1. **Formulaire de connexion avec un lien « Sign up »** : aucune suggestion ne doit s'ouvrir (anti-objectif). → cas dans `detect.check.mjs` (Task 5) + page `login.html` (Task 10).
2. **Inscription qui redirige deux fois puis atterrit sur une page sans champ, avec un rechargement avant le clic** : la bannière apparaît sur la page finale et revient après le rechargement. → page `signup-redirect.html` (Task 10).
3. **Service worker arrêté entre l'envoi et la page suivante** : l'attente survit (stockage de session). → étape `chrome://serviceworker-internals` (Task 10).
4. **Clé posée en `.value` dans un champ en lecture seule, ~1 s après l'ouverture du dialogue** : détectée. → `apikey-dialog.html` (Task 10).
5. **Clé que Luca colle lui-même dans un champ éditable** : aucune bannière. → `apikey-dialog.html`, champ « Paste here » (Task 10).

---

### Task 1: Hôte — `get-logins` ne renvoie que des identifiants, titre = nom d'hôte, générateur du core

**Files:**
- Modify: `src-tauri/src/commands/browser.rs` (`handle_get_logins` l. 157-184, `handle_set_login` l. 222-243, `url_matches` l. 249-265, tests l. 325+)
- Modify: `src-tauri/src/native_messaging.rs` (`handle_generate_password` l. 729-754, `use rand::Rng` l. 28 si devenu inutile)

**Interfaces:**
- Produces: `fn is_login(entry: &Entry) -> bool` (privé, `MyPass_Type` absent ou `"login"`) ; `fn site_title(url: &str) -> String` (privé : `extract_domain(url)` sinon `url`). Utilisés par Task 4.

- [ ] **Step 1: Write the failing tests** (dans `mod tests` de `browser.rs`)

```rust
#[test]
fn get_logins_ignores_typed_items_and_empty_urls() {
    let mut kf = KeePassFile::new("test");
    kf.root.group.entries.push(Entry::new("GitHub", "dev", "pw", "https://github.com"));
    kf.root.group.entries.push(Entry::new("Sans URL", "u", "p", ""));
    kf.root.group.entries.push(entry_with_type("Visa", "card", &[("CC_Number", "4242")]));
    kf.root.group.entries.push(entry_with_type("Mistral", "api_key", &[("API_Key", "k")]));
    let logins = handle_get_logins(&kf, "https://github.com/login").unwrap();
    let names: Vec<&str> = logins.iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["GitHub"]);
}

#[test]
fn set_login_titles_new_entry_with_host() {
    let mut kf = KeePassFile::new("test");
    handle_set_login(&mut kf, "https://www.github.com/signup?x=1", "dev", "pw", None).unwrap();
    let e = &kf.root.group.entries[0];
    assert_eq!(e.title(), "github.com");
    assert_eq!(e.url(), "https://www.github.com/signup?x=1");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib commands::browser`
Expected: FAIL — `names` contient aussi « Sans URL », « Visa », « Mistral » ; titre = URL complète.

- [ ] **Step 3: Implement** — `url_matches` renvoie `false` si `entry.url().trim()` est vide ; `handle_get_logins` filtre `is_login(e) && url_matches(e, url)` ; `handle_set_login` crée `Entry::new(&site_title(url), login, password, url)`. Dans `native_messaging.rs`, `handle_generate_password` appelle `crate::kdbx::generator::generate_password(PasswordConfig { length: 20, uppercase: true, lowercase: true, digits: true, symbols: true, exclude_similar: false, exclude_ambiguous: false })` ; une erreur → `error_response(req, 5, "Password generation failed")`.

- [ ] **Step 4: Run tests + clippy**

Run: `cd src-tauri && cargo test && cargo clippy -- -D warnings`
Expected: PASS, aucun warning.

- [ ] **Step 5: Commit**

```bash
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas
git add src-tauri/src/commands/browser.rs src-tauri/src/native_messaging.rs
git commit -m "fix(browser): get-logins returns only logins with a URL; saved logins are titled by host; core generator"
```

---

### Task 2: L'app se rafraîchit après une écriture venue du navigateur

**Files:**
- Modify: `src-tauri/src/native_messaging.rs` (à côté de `focus_main_window` l. 61 ; appel en fin de `handle_set_login` après `db.save()` réussi)
- Modify: `src/components/layout/AppShell.tsx` (nouvel `useEffect` près de celui de la l. 23)

**Interfaces:**
- Produces: `fn notify_vault_changed()` dans `native_messaging.rs` — émet l'événement Tauri **`"vault-changed-externally"`** (payload `()`) via `APP_HANDLE` + `tauri::Emitter`. Réutilisé par Task 4.

- [ ] **Step 1: Implement `notify_vault_changed()`** et l'appeler dans `handle_set_login` après la sauvegarde réussie.
- [ ] **Step 2: Implement l'écoute dans `AppShell`** : si `"__TAURI_INTERNALS__" in window`, `listen("vault-changed-externally", () => void syncNowAndRefresh(queryClient).catch(() => {}))` (import `listen` de `@tauri-apps/api/event`, comme `SshSignRequestDialog.tsx`), désabonnement au démontage. Rien en mode web.
- [ ] **Step 3: Verify**

Run: `npm run lint && npm run format:check && npm run build && cd src-tauri && cargo clippy -- -D warnings`
Expected: tout passe. (Effet visible vérifié en Task 10.)

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/native_messaging.rs src/components/layout/AppShell.tsx
git commit -m "feat(desktop): the entry list refreshes and syncs as soon as the browser saves a login"
```

---

### Task 3: Type d'élément « Clé API » dans le core et l'app

**Files:**
- Modify: `crates/mypass-core/src/ops/entries.rs:347` (`PROTECTED_CUSTOM_KEYS`) + test à côté de `apply_custom_fields_protects_ssh_private_key` (l. 431)
- Modify: `src/lib/items.ts` (`ItemKind`, nouvel `API_KEY_FIELDS`, `SECRET_FIELDS`, `itemKind`)
- Modify: `src/components/entries/ItemForm.tsx` (`KIND_FIELDS` l. 43, `isSecret` l. 165, `wide` l. 162)
- Modify: `src/components/entries/EntryDetail.tsx` (icônes l. 107-117 et 324-336, `KIND_FIELDS` l. 300, type de `kind` l. 311)
- Modify: `src/components/entries/EntryCard.tsx` (sous-titre l. 22-30, icône l. 45-55)
- Modify: `src/components/entries/EntryList.tsx` (`EMPTY_ICONS` l. 15)
- Modify: `src/components/layout/AppSidebar.tsx` (menu « Nouveau » l. 113-131, filtre après « Clés SSH » l. 184)
- Modify: `src/i18n/en.json`, `src/i18n/fr.json`

**Interfaces:**
- Produces: `ItemKind` inclut `"api_key"` ; `MYPASS_TYPE_KEY` vaut `"api_key"` pour ce type ; `API_KEY_FIELDS = ["API_Key", "API_URL"] as const` ; `"API_Key"` ∈ `SECRET_FIELDS` et `PROTECTED_CUSTOM_KEYS`. Noms de champs repris par Task 4 (Rust) : `MyPass_Type`, `API_Key`, `API_URL`.

- [ ] **Step 1: Write the failing Rust test**

```rust
#[test]
fn apply_custom_fields_protects_api_key() {
    let mut entry = Entry::new("Mistral", "", "", "");
    let mut f = HashMap::new();
    f.insert("MyPass_Type".to_string(), "api_key".to_string());
    f.insert("API_Key".to_string(), "Xq7bR2kLm9TzP4wV8nC1sD6fG3hJ5aYe".to_string());
    f.insert("API_URL".to_string(), "https://console.mistral.ai/api-keys".to_string());
    apply_custom_fields(&mut entry, &f);
    let get = |key: &str| entry.strings.iter().find(|s| s.key == key).unwrap();
    assert_eq!(get("API_Key").value.protected.as_deref(), Some("True"));
    assert_eq!(get("API_URL").value.protected, None);
}
```

- [ ] **Step 2: Run to verify it fails** — `cd crates/mypass-core && cargo test apply_custom_fields_protects_api_key` → FAIL (`protected` = `None`).
- [ ] **Step 3: Implement** — ajouter `"API_Key"` à `PROTECTED_CUSTOM_KEYS`.
- [ ] **Step 4: Run** — `cd crates/mypass-core && cargo test` → PASS.
- [ ] **Step 5: Implement le front**
  - `items.ts` : type, `API_KEY_FIELDS`, `SECRET_FIELDS`, reconnaissance dans `itemKind`.
  - `ItemForm` : `KIND_FIELDS.api_key = API_KEY_FIELDS` ; `isSecret = SECRET_FIELDS.includes(fieldKey)` (remplace la liste en dur) ; `API_Key` et `API_URL` en pleine largeur.
  - Icône lucide **`KeyRound`** partout où une branche `ssh_key` choisit une icône (`EntryCard`, `EntryDetail` ×2, `EMPTY_ICONS`, `AppSidebar`).
  - `EntryCard` : sous-titre = nom d'hôte de `API_URL` (sinon vide) — jamais la clé.
  - `AppSidebar` : entrée de menu `items.new.api_key` (ouvre `ItemForm kind="api_key"`) et filtre `nav.apiKeys` après « Clés SSH ».
  - i18n EN / FR :

| Clé | EN | FR |
|---|---|---|
| `items.api_key` | API key | Clé API |
| `items.empty.api_key` | No API keys | Aucune clé API |
| `items.emptyDesc.api_key` | MyPass offers to save a key when a site shows it to you, or add one here. | MyPass propose de ranger une clé quand un site te l'affiche, ou ajoute-la ici. |
| `items.new.api_key` | New API key | Nouvelle clé API |
| `items.desc.api_key` | A service's API key, stored encrypted. | La clé API d'un service, stockée chiffrée. |
| `items.titlePlaceholder.api_key` | e.g. Mistral | ex. Mistral |
| `items.fields.API_Key` | Key | Clé |
| `items.fields.API_URL` | Created on | Créée sur |
| `nav.apiKeys` | API keys | Clés API |

- [ ] **Step 6: Verify** — `npm run lint && npm run format:check && npm run build` → PASS ; puis `npm run dev`, créer une « Nouvelle clé API » dans le mock : la clé est masquée, l'œil la révèle, la copie marche, le filtre « Clés API » la liste.
- [ ] **Step 7: Commit**

```bash
git add crates/mypass-core/src/ops/entries.rs src/lib/items.ts src/components src/i18n
git commit -m "feat: API key item type — protected key, form, list filter, detail"
```

---

### Task 4: Hôte — actions `check-api-key` et `save-api-key`

**Files:**
- Modify: `src-tauri/src/commands/browser.rs` (fonctions pures + tests)
- Modify: `src-tauri/src/native_messaging.rs` (routeur l. 379-391 ; handlers sur le modèle de `handle_get_identities_action` l. 600-640 et `handle_set_login` l. 642-689)

**Interfaces:**
- Consumes: `notify_vault_changed()` (Task 2) ; champs `MyPass_Type`/`API_Key`/`API_URL` (Task 3) ; `apply_custom_fields` (core).
- Produces (Rust) : `pub fn api_key_exists(kf: &KeePassFile, key: &str) -> bool` ; `pub fn service_name(url: &str) -> String` ; `pub fn handle_save_api_key(kf: &mut KeePassFile, key: &str, url: &str) -> Result<bool, String>` (`Ok(true)` = créée, `Ok(false)` = existait déjà, `Err` si `key` vide après `trim`).
- Produces (protocole, messages chiffrés comme `get-identities`) :
  - `check-api-key` `{key}` → `{success:"true", exists: bool}`
  - `save-api-key` `{key, url}` → `{success:"true", created: bool}` ; puis `db.save()` et `notify_vault_changed()` si créée. Coffre fermé → `error_response(req, 2, "No open database")`.

- [ ] **Step 1: Write the failing tests** (`browser.rs`)

```rust
const MISTRAL: &str = "Xq7bR2kLm9TzP4wV8nC1sD6fG3hJ5aYe";

#[test]
fn api_key_exists_finds_it_in_any_field_or_notes() {
    let mut kf = KeePassFile::new("test");
    assert!(!api_key_exists(&kf, MISTRAL));
    kf.root.group.entries.push(Entry::new("Old", "u", MISTRAL, "")); // rangée comme mot de passe
    assert!(api_key_exists(&kf, MISTRAL));
    let mut kf2 = KeePassFile::new("test");
    let mut e = Entry::new("Notes", "", "", "");
    set_string_field(&mut e, "Notes", &format!("clé prod : {MISTRAL}"));
    kf2.root.group.entries.push(e);
    assert!(api_key_exists(&kf2, MISTRAL));
}

#[test]
fn save_api_key_creates_typed_protected_entry_once() {
    let mut kf = KeePassFile::new("test");
    assert_eq!(handle_save_api_key(&mut kf, MISTRAL, "https://console.mistral.ai/api-keys"), Ok(true));
    assert_eq!(handle_save_api_key(&mut kf, MISTRAL, "https://console.mistral.ai/api-keys"), Ok(false));
    assert_eq!(kf.root.group.entries.len(), 1);
    let e = &kf.root.group.entries[0];
    assert_eq!(e.title(), "Mistral");
    assert_eq!(e.get_field("MyPass_Type"), Some("api_key"));
    assert_eq!(e.get_field("API_URL"), Some("https://console.mistral.ai/api-keys"));
    let key = e.strings.iter().find(|s| s.key == "API_Key").unwrap();
    assert_eq!(key.value.content, MISTRAL);
    assert_eq!(key.value.protected.as_deref(), Some("True"));
    assert!(handle_save_api_key(&mut kf, "  ", "https://x.io").is_err());
}

#[test]
fn service_name_is_the_registrable_label() {
    assert_eq!(service_name("https://console.mistral.ai/api-keys"), "Mistral");
    assert_eq!(service_name("https://platform.openai.com"), "Openai");
    assert_eq!(service_name("https://www.github.com/settings/tokens"), "Github");
    assert_eq!(service_name("http://localhost:5180/apikey-dialog.html"), "Localhost");
}
```

- [ ] **Step 2: Run** — `cd src-tauri && cargo test --lib commands::browser` → FAIL (fonctions absentes).
- [ ] **Step 3: Implement** les trois fonctions. `service_name` : hôte via `extract_domain`, avant-dernier label s'il y en a ≥ 2 sinon l'hôte, première lettre en majuscule. `handle_save_api_key` : si `api_key_exists` → `Ok(false)` ; sinon `Entry::new(&service_name(url), "", "", "")` + `apply_custom_fields` avec `MyPass_Type=api_key`, `API_Key`, `API_URL`, push à la racine, `Ok(true)`.
- [ ] **Step 4: Implement** `handle_check_api_key` / `handle_save_api_key` dans `native_messaging.rs` et les deux routes.
- [ ] **Step 5: Run** — `cd src-tauri && cargo test && cargo clippy -- -D warnings` → PASS.
- [ ] **Step 6: Commit**

```bash
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas
git add src-tauri/src/commands/browser.rs src-tauri/src/native_messaging.rs
git commit -m "feat(browser): check-api-key and save-api-key native actions"
```

---

### Task 5: Extension — détecteurs purs et leur vérification Node

**Files:**
- Create: `keepassxc-browser/keepassxc-browser/content/detect.js`
- Create: `keepassxc-browser/detect.check.mjs` (hors du dossier de l'extension)
- Modify: `keepassxc-browser/keepassxc-browser/manifest.json` (ajouter `content/detect.js` en tête des scripts de contenu propres à MyPass, avant `content/pwgen.js`)

**Interfaces:**
- Produces (global `kpxcDetect`, script classique sans DOM ni `browser.*`) :
  - `kpxcDetect.isApiKeyToken(token: string, context: string) => boolean`
  - `kpxcDetect.findApiKey(text: string, context: string) => string | null` — découpe `text` sur les blancs et guillemets, renvoie le premier jeton qui passe `isApiKeyToken`.
  - `kpxcDetect.isSignupPasswordField({ autocomplete, passwordCount, index, buttonText, text, path }) => boolean`
  - `kpxcDetect.isSubmitText(text: string) => boolean`

Règles (décidées ; seuils exacts au constructeur tant que les tests passent) :
- **Préfixe connu** (`sk-`, `sk-proj-`, `sk-ant-`, `ghp_`, `gho_`, `github_pat_`, `glpat-`, `hf_`, `gsk_`, `xoxb-`, `xoxp-`, `AIza`, `AKIA`, `sk_live_`, `rk_live_`, `sk_test_`) suivi d'au moins 20 caractères `[A-Za-z0-9_\-]` → candidat, contexte non requis.
- **Sans préfixe** : 24 à 200 caractères `[A-Za-z0-9_\-.]`, au moins une lettre et un chiffre, entropie de Shannon ≥ 3,5 bits/caractère, ni UUID, ni hex pur de 40 ou 64 caractères, **et** `context` contient (insensible à la casse) l'un de : `api key`, `api-key`, `apikey`, `clé api`, `clé d'api`, `secret`, `token`, `jeton`, `access key`, `copy it now`, `won't be able to see`, `ne sera plus affichée`.
- **Champ d'inscription**, dans cet ordre : `autocomplete` `current-password` → faux, `new-password` → vrai ; `passwordCount ≥ 2` → vrai sauf `index 0` quand `passwordCount ≥ 3` ; `buttonText` mot d'inscription → vrai, mot de connexion → faux ; `path` `/signup|/sign-up|/register|/join|/inscription|/create-account` → vrai, `/login|/signin|/sign-in|/connexion` → faux ; `text` : vrai seulement s'il a un mot d'inscription **et** aucun mot de connexion.
- Mots d'inscription : `sign up`, `signup`, `register`, `create account`, `create an account`, `create your account`, `join`, `inscription`, `s'inscrire`, `créer un compte`, `créer mon compte`. Mots de connexion : `sign in`, `log in`, `login`, `welcome back`, `connexion`, `se connecter`.
- `isSubmitText` : ≤ 40 caractères et un mot d'inscription ou `continue`, `next`, `submit`, `valider`, `continuer`, `suivant`, `envoyer`.

- [ ] **Step 1: Write the failing check** `keepassxc-browser/detect.check.mjs`

```js
// Runnable check for the extension's pure detectors: `node keepassxc-browser/detect.check.mjs`
import { readFileSync } from "node:fs";
import vm from "node:vm";
const src = readFileSync(new URL("./keepassxc-browser/content/detect.js", import.meta.url), "utf8");
const d = vm.runInNewContext(`${src}\n;kpxcDetect`, {});
let failed = 0;
const check = (ok, msg) => { console.log(`${ok ? "ok  " : "FAIL"} - ${msg}`); if (!ok) failed++; };

const MISTRAL = "Xq7bR2kLm9TzP4wV8nC1sD6fG3hJ5aYe";
const CTX = "Your API key. Make sure to copy it now, you won't be able to see it again.";
check(d.isApiKeyToken("sk-proj-" + "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8s9T0", ""), "OpenAI project key without context");
check(d.isApiKeyToken("ghp_" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8", ""), "GitHub token without context");
check(d.isApiKeyToken(MISTRAL, CTX), "prefix-less Mistral key inside an API key dialog");
check(!d.isApiKeyToken(MISTRAL, "Order reference"), "same string without key context");
check(!d.isApiKeyToken("3f786850e387550fdab836ed7e6dc881de23001b", "token"), "40-hex commit SHA");
check(!d.isApiKeyToken("123e4567-e89b-12d3-a456-426614174000", "API key"), "UUID");
check(!d.isApiKeyToken("averyveryverylongwordwithoutanydigits", "API key"), "no digit");
check(!d.isApiKeyToken("sk-abc12", ""), "prefix too short");
check(d.findApiKey(`Your new key: ${MISTRAL} Copy`, CTX) === MISTRAL, "findApiKey extracts the key from text");
check(d.findApiKey("Welcome to the console", CTX) === null, "findApiKey on plain text");

const f = (o) => d.isSignupPasswordField({ autocomplete: "", passwordCount: 1, index: 0, buttonText: "", text: "", path: "/", ...o });
check(f({ autocomplete: "new-password" }), "new-password");
check(!f({ autocomplete: "current-password", passwordCount: 2 }), "current-password wins");
check(f({ passwordCount: 2, index: 0 }) && f({ passwordCount: 2, index: 1 }), "password + confirm");
check(!f({ passwordCount: 3, index: 0 }) && f({ passwordCount: 3, index: 1 }), "change form: current excluded");
check(!f({ buttonText: "Sign in", text: "Don't have an account? Sign up" }), "login page with a Sign up link");
check(!f({ buttonText: "Continue", text: "Welcome back. Don't have an account? Sign up" }), "ambiguous login page");
check(f({ buttonText: "Créer un compte" }), "French signup button");
check(f({ path: "/signup" }) && !f({ path: "/login" }), "signup / login paths");
check(f({ text: "Create your account" }), "signup heading only");

check(d.isSubmitText("Create account") && d.isSubmitText("Continuer"), "submit words");
check(!d.isSubmitText("Forgot password?"), "non-submit link");

console.log(failed ? `\n${failed} check(s) failed` : "\nall checks passed");
process.exit(failed ? 1 : 0);
```

- [ ] **Step 2: Run** — `node keepassxc-browser/detect.check.mjs` → échoue (`detect.js` absent).
- [ ] **Step 3: Implement `content/detect.js`** (`'use strict'; const kpxcDetect = {};` + les quatre fonctions) et l'ajouter au manifeste.
- [ ] **Step 4: Run** — `node keepassxc-browser/detect.check.mjs` → `all checks passed`.
- [ ] **Step 5: Commit**

```bash
git add keepassxc-browser/detect.check.mjs keepassxc-browser/keepassxc-browser/content/detect.js keepassxc-browser/keepassxc-browser/manifest.json
git commit -m "feat(extension): pure signup-field and API-key detectors with a Node check"
```

---

### Task 6: Extension — suggestion de mot de passe sous le champ d'inscription

**Files:**
- Create: `keepassxc-browser/keepassxc-browser/content/password-suggestion.js` (après `content/autocomplete.js` et `content/detect.js` dans le manifeste)
- Modify: `keepassxc-browser/keepassxc-browser/content/icon-handler.js` (`addPasswordIcons` l. 67-75)
- Modify: `keepassxc-browser/keepassxc-browser/manifest.json`
- Modify: `_locales/en/messages.json`, `_locales/fr/messages.json`

**Interfaces:**
- Consumes: `kpxcDetect.isSignupPasswordField` (Task 5) ; `Autocomplete` (`content/autocomplete.js`) ; `kpxcPasswordGenerator.fill` (`content/pwgen.js`) ; message `generate_password`.
- Produces: `kpxcSignup.isSignupField(field: HTMLInputElement, combination) => boolean` (extrait du DOM : `autocomplete`, nombre et rang des champs mot de passe de `combination.passwordInputs`, texte du bouton d'envoi du formulaire, `textContent` du formulaire ou du conteneur limité à 2000 caractères, `location.pathname`) ; `kpxcPasswordSuggestion.attach(field)`.

- [ ] **Step 1: Implement** `class PasswordSuggestion extends Autocomplete` :
  - `attach(field)` n'appelle **pas** `Autocomplete.create` (qui force `autocomplete="off"` et casserait la détection) : il pose ses propres écouteurs `focus`, `click`, `keydown`, `keyup` une seule fois par champ.
  - Au focus/clic, si `kpxc.databaseState === DatabaseState.UNLOCKED` : `sendMessage('generate_password')`, `this.elements = [{ title: tr('passwordSuggestionTitle'), value: password, group: tr('passwordSuggestionHint'), uuid: '' }]`, `showList(field)`. Sinon rien.
  - `itemClick` / `itemEnter` : `kpxcPasswordGenerator.fill(this.input, password)`, `closeList()`. Échap et clic ailleurs : comportement de la classe de base.
  - `Autocomplete.updateList` trie et filtre des identifiants (`kpxc.credentials`, saisie du champ) : la surcharger pour afficher l'unique élément tel quel.
- [ ] **Step 2: Hook** — dans `addPasswordIcons`, avant le test de `usePasswordGeneratorIcons` : pour chaque `input` de `c.passwordInputs`, `if (kpxcSignup.isSignupField(input, c)) kpxcPasswordSuggestion.attach(input)`. Le réglage de l'icône reste tel quel (désactivé par défaut).
- [ ] **Step 3: Locales** — `passwordSuggestionTitle` : EN « Use a strong password » / FR « Utiliser un mot de passe fort » ; `passwordSuggestionHint` : EN « MyPass · 20 characters · fills both fields » / FR « MyPass · 20 caractères · remplit les deux champs ».
- [ ] **Step 4: Verify** — recharger l'extension (`chrome://extensions`), page `signup-form.html` de Task 10 servie par `npx vite keepassxc-browser/test-pages --port 5180` : le menu s'ouvre au focus du mot de passe, un clic remplit mot de passe et confirmation ; `login.html` : rien ne s'ouvre. (Peut se faire avec l'app installée, `generate-password` existe déjà.)
- [ ] **Step 5: Commit**

```bash
git add keepassxc-browser/keepassxc-browser
git commit -m "feat(extension): strong password suggestion under sign-up password fields"
```

---

### Task 7: Extension — enregistrement fiable après inscription

**Files:**
- Modify: `keepassxc-browser/keepassxc-browser/background/page.js` (l. 66-67, 178-190, 254-266)
- Modify: `keepassxc-browser/keepassxc-browser/content/pwgen.js` (`kpxcPasswordGenerator.fill` l. 116-156)
- Modify: `keepassxc-browser/keepassxc-browser/content/keepassxc-browser.js` (`initContentScript` l. 869-887, `retrieveCredentialsCallback` l. 682-687)
- Modify: `keepassxc-browser/keepassxc-browser/content/banner.js` (gestionnaires des boutons l. 108-140, choix d'une entrée à mettre à jour)
- Modify: `keepassxc-browser/keepassxc-browser/content/form.js` (`initForm` l. 163-181)

**Interfaces:**
- Consumes: `kpxcDetect.isSubmitText` (Task 5) ; `page.getBaseDomainFromUrl(hostname, url)` (`background/page.js:449`).
- Produces: messages inchangés de nom, sémantique étendue :
  - `page_set_submitted` `[submitted, username, password, url, oldCredentials, generated = false]` → écrit `{submitted, username, password, url, oldCredentials, generated, time}` dans `browser.storage.session` sous `pending_<tabId>` ; si une attente `generated` existe déjà avec le même mot de passe, `generated` reste vrai.
  - `page_get_submitted` → l'attente de l'onglet, ou `{}` (et suppression) si plus vieille que 15 min ou si le domaine de base de `tab.url` diffère de celui de `url`.
  - `page_clear_submitted` → supprime `pending_<tabId>`.
  - `page.submitted` / `page.submittedCredentials` en mémoire disparaissent.

- [ ] **Step 1: Implement** le stockage de session dans `page.js`.
- [ ] **Step 2: Implement** dans `kpxcPasswordGenerator.fill`, après remplissage : `sendMessage('page_set_submitted', [true, username, password, url, kpxc.credentials, true])`, `username` = valeur du champ identifiant de `kpxcFields.getCombination(elem)` (sinon `''`), `url` calculée comme `form.js:34`. Couvre suggestion, icône et menu contextuel.
- [ ] **Step 3: Implement** dans `initContentScript` : uniquement si `window.self === window.top` ; plus de `return` quand `creds.username` est vide ; pas d'effacement par le compteur de redirections si `creds.generated` ; aucun `page_clear_submitted` avant affichage ; si `rememberCredentials` renvoie `false` (déjà dans le coffre) → `page_clear_submitted`. Dans `retrieveCredentialsCallback`, retirer le `page_clear_submitted` d'avant l'affichage et ne rien faire si `kpxcBanner.created`.
- [ ] **Step 4: Implement** dans `banner.js` : `sendMessage('page_clear_submitted')` au clic sur « Nouveau », sur « Annuler » quand il ferme la bannière (pas quand il revient du dialogue), et quand une entrée est choisie pour « Mettre à jour ».
- [ ] **Step 5: Implement** dans `initForm` : `keydown` Entrée sur un `INPUT` du formulaire → `kpxcForm.onSubmit.call(form, e)` ; écouteur `click` → `kpxcForm.onSubmit` sur chaque `button, a, [role=button], input[type=submit], input[type=button]` du formulaire dont `kpxcDetect.isSubmitText(textContent || value)` est vrai (en plus du bouton déjà choisi ; `submitTriggered` évite les doublons). Champ d'inscription **hors `<form>`** : dans `kpxcPasswordSuggestion.attach`, poser le même écouteur sur les boutons de ce type du plus proche ancêtre (jusqu'à 4 niveaux) qui en contient un.
- [ ] **Step 6: Verify** — `node keepassxc-browser/detect.check.mjs` passe toujours ; vérification navigateur en Task 10 (Review Focus 2 et 3).
- [ ] **Step 7: Commit**

```bash
git add keepassxc-browser/keepassxc-browser
git commit -m "fix(extension): the save banner survives redirects, field-less pages and a stopped service worker"
```

---

### Task 8: Relevé de la vraie console Mistral (avec Luca, sans code)

**Files:**
- Modify: `docs/superpowers/specs/2026-11-03-signup-and-api-keys-design.md` (annexe « Relevé console Mistral »)
- Modify (si le relevé l'exige) : `keepassxc-browser/detect.check.mjs`, `content/detect.js`

- [ ] **Step 1:** Luca ouvre console.mistral.ai dans son Chrome et crée une clé de test nommée `mypass-test`. Claude in Chrome lit le dialogue (`read_page` / `javascript_tool` en lecture seule), **sans jamais recopier la valeur de la clé** dans la conversation, un fichier ou un log : relever seulement sa longueur et son alphabet.
- [ ] **Step 2:** Noter dans l'annexe : élément qui porte la clé (texte, `input` readonly ou non, masquée puis révélée), shadow DOM / iframe, `role` et texte du dialogue, délai d'apparition, mécanisme du bouton Copier (`writeText` ou `execCommand` — vérifier en enveloppant temporairement `navigator.clipboard.writeText` dans la console de la page sans afficher l'argument).
- [ ] **Step 3:** Si le relevé contredit Task 5 (longueur, alphabet, mot-clé absent), ajouter le cas réel (avec une clé **inventée** de même forme) à `detect.check.mjs` et ajuster `detect.js` jusqu'au vert.
- [ ] **Step 4:** Luca supprime la clé `mypass-test` sur la console.
- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-11-03-signup-and-api-keys-design.md keepassxc-browser/
git commit -m "docs: Mistral console API key dialog survey"
```

---

### Task 9: Extension — détection des clés API et bannière « Clé API détectée »

**Files:**
- Create: `keepassxc-browser/keepassxc-browser/content/apikey.js`
- Create: `keepassxc-browser/keepassxc-browser/content/clipboard-hook.js`
- Modify: `keepassxc-browser/keepassxc-browser/manifest.json`
- Modify: `keepassxc-browser/keepassxc-browser/content/banner.js` (extraire le montage de la fin de `kpxcBanner.create`, l. 150-170)
- Modify: `keepassxc-browser/keepassxc-browser/content/keepassxc-browser.js` (`initContentScript`, `detectDatabaseChange` l. 114-135)
- Modify: `keepassxc-browser/keepassxc-browser/background/keepass.js` (`kpActions` l. 32-50, nouvelles fonctions sur le modèle de `keepass.getIdentities` l. 597-622)
- Modify: `keepassxc-browser/keepassxc-browser/background/event.js` (`messageHandlers` l. 245+)
- Modify: `keepassxc-browser/keepassxc-browser/background/page.js`
- Modify: `_locales/en/messages.json`, `_locales/fr/messages.json`

**Interfaces:**
- Consumes: `kpxcDetect.findApiKey` (Task 5) ; actions `check-api-key` / `save-api-key` (Task 4) ; relevé de Task 8.
- Produces:
  - background : `kpActions.CHECK_API_KEY = 'check-api-key'`, `kpActions.SAVE_API_KEY = 'save-api-key'` ; `keepass.checkApiKey(tab, [key]) => Promise<boolean>` ; `keepass.saveApiKey(tab, [key, url]) => Promise<'created' | 'exists' | 'error'>` ; `page.ignoreApiKey(tab, [hash])`, `page.isApiKeyIgnored(tab, [hash]) => Promise<boolean>` (tableau d'empreintes dans `browser.storage.session`, clé `ignored_api_keys`).
  - messages : `check_api_key`, `save_api_key`, `apikey_ignore`, `apikey_is_ignored`.
  - contenu : `kpxcApiKey.init()`, `kpxcApiKey.onUnlocked()` ; `kpxcBanner.mount(banner)` (montage shadow DOM partagé) ; `kpxcBanner.createApiKey({ key, url, hash }, locked: boolean)`.
  - page ↔ contenu : `CustomEvent('mypass-copy', { detail: text })` sur `document`.

- [ ] **Step 1: Implement `clipboard-hook.js`** — enveloppe `navigator.clipboard.writeText` (si présent) : `document.dispatchEvent(new CustomEvent('mypass-copy', { detail: String(text) }))` puis appel de l'original. Manifeste : nouvelle entrée `content_scripts` `{ "matches": ["<all_urls>"], "js": ["content/clipboard-hook.js"], "run_at": "document_start", "all_frames": true, "world": "MAIN" }`.
- [ ] **Step 2: Implement `apikey.js`**
  - `init()` (appelé depuis `initContentScript` après le chargement des réglages, sur tous les cadres) : `MutationObserver` `{ childList: true, subtree: true, characterData: true }`, mutations regroupées toutes les 300 ms ; examen des nœuds ajoutés visibles (`kpxcFields.isVisible`) : texte, `code`, `pre`, et `input`/`textarea` lus par `.value`.
  - Champs tapés par l'utilisateur : `WeakSet` alimenté par les événements `input`/`paste` de confiance (`isTrusted`) en capture sur `document` ; ces champs sont ignorés.
  - Contexte = texte du plus proche `[role=dialog], dialog, [aria-modal=true]`, sinon de 4 ancêtres, limité à 2000 caractères. Un dialogue ajouté est réexaminé toutes les 500 ms pendant 3 s.
  - Copie : `copy` en capture (texte de la sélection ou de la sélection du champ actif) et `mypass-copy` ; contexte = élément du dernier clic de confiance.
  - `propose(key)` : ignorer si déjà vue sur cette page ; empreinte SHA-256 hex (`crypto.subtle`) ; `apikey_is_ignored` → stop ; si déverrouillé : `check_api_key` vrai → stop, sinon `kpxcBanner.createApiKey(item, false)` ; si verrouillé : `createApiKey(item, true)` et garder `kpxcApiKey.pending = item`.
  - `onUnlocked()` (appelé en fin de la branche déverrouillée de `kpxc.detectDatabaseChange`) : si `pending`, `check_api_key` → déjà là : fermer la bannière ; sinon la réafficher en mode normal.
- [ ] **Step 3: Implement la bannière** — `kpxcBanner.mount` extrait de `create` ; `createApiKey` ne fait rien si `kpxcBanner.created` ; texte `apiKeyDetected` (hôte en paramètre) ou `apiKeyLocked` ; boutons `apiKeySave` → `save_api_key` → notification `apiKeySaved` / `apiKeyAlreadySaved` / erreur, puis fermeture ; `apiKeyIgnore` → `apikey_ignore` puis fermeture. En mode verrouillé, « Enregistrer » est désactivé.
- [ ] **Step 4: Implement le service worker** — `kpActions`, `keepass.checkApiKey`, `keepass.saveApiKey` (`testAssociation(tab, [false])`, `keepassClient.sendMessage` comme `getIdentities`), `page.ignoreApiKey` / `page.isApiKeyIgnored`, entrées de `messageHandlers`.
- [ ] **Step 5: Locales**

| Clé | EN | FR |
|---|---|---|
| `apiKeyDetected` | API key detected on $HOST$ | Clé API détectée sur $HOST$ |
| `apiKeyLocked` | API key detected on $HOST$ — unlock MyPass to save it | Clé API détectée sur $HOST$ — déverrouille MyPass pour l'enregistrer |
| `apiKeySave` | Save | Enregistrer |
| `apiKeyIgnore` | Ignore | Ignorer |
| `apiKeySaved` | API key saved in MyPass | Clé API enregistrée dans MyPass |
| `apiKeyAlreadySaved` | This API key is already in MyPass | Cette clé API est déjà dans MyPass |

(`$HOST$` = placeholder Chrome i18n `{ "host": { "content": "$1" } }`, passé via `tr(key, [host])`.)

- [ ] **Step 6: Verify** — `node keepassxc-browser/detect.check.mjs` passe ; navigateur en Task 10.
- [ ] **Step 7: Commit**

```bash
git add keepassxc-browser/keepassxc-browser
git commit -m "feat(extension): detect API keys shown or copied on a page and save them in one click"
```

---

### Task 10: Pages de test, bout en bout dans Chrome, non-régression

**Files:**
- Create: `keepassxc-browser/test-pages/` : `signup-form.html`, `signup-js.html`, `signup-steps.html` + `signup-step2.html`, `signup-redirect.html` + `onboarding.html`, `dashboard.html`, `login.html`, `apikey-dialog.html`, `commit-page.html`

Contenu des pages (factices, aucune vraie clé) :
- `signup-form.html` : `<form>` e-mail + mot de passe (`autocomplete="new-password"`) + confirmation, bouton « Create account » → `dashboard.html`.
- `signup-js.html` : pas de `<form>`, bouton `<div role="button">Sign up</div>` qui fait un faux `fetch` puis `location = "dashboard.html"`.
- `signup-steps.html` : e-mail + « Continue » → `signup-step2.html` (mot de passe + confirmation, « Create account » → `dashboard.html`).
- `signup-redirect.html` : formulaire d'inscription qui envoie vers `onboarding.html`, qui redirige (`location.replace`) vers `dashboard.html` après 300 ms.
- `dashboard.html` : aucun champ.
- `login.html` : formulaire « Sign in » (identifiant + mot de passe) avec le lien « Don't have an account? Sign up ».
- `apikey-dialog.html` : bouton « Create key » qui ouvre un `<div role="dialog">` « Your API key — copy it now, you won't be able to see it again », avec un `input readonly` dont `.value` reçoit une clé factice de 32 caractères après 800 ms et un bouton « Copy » (`navigator.clipboard.writeText`) ; ailleurs, un champ éditable « Paste here ». Reproduire la forme relevée en Task 8.
- `commit-page.html` : liste de hashs de commit et d'UUID sous le titre « Access tokens ».

- [ ] **Step 1: Préparer un environnement qui ne touche pas au vrai coffre**
  1. Fermer le MyPass installé (il tient le port 25798 du relais).
  2. Lancer l'app de dev avec un `APPDATA` temporaire (pas de `sync.json`, donc aucune synchro vers le serveur) : `$env:APPDATA="$env:TEMP\mypass-e2e"; npm run tauri dev` (en arrière-plan).
  3. Créer un **coffre de test** dans un dossier temporaire, activer l'intégration navigateur dans Réglages.
  4. Chrome garde le manifeste natif du vrai `APPDATA` : son relais (l'exe installé) se connecte au port 25798, donc à l'app de dev. Vérifier dans la sortie de `tauri dev` qu'elle a bien pris ce port (sinon une instance installée tourne encore). Recharger l'extension dans `chrome://extensions`, l'associer au coffre de test.
  5. Servir les pages : `npx vite keepassxc-browser/test-pages --port 5180`.
- [ ] **Step 2: Inscription** — pour `signup-form`, `signup-js`, `signup-steps`, `signup-redirect` : la suggestion s'ouvre au focus du mot de passe, un clic remplit les deux champs ; après l'envoi, la bannière apparaît (sur `dashboard.html` au plus tard) ; recharger une fois avant de cliquer → elle revient ; « Nouveau » crée l'entrée titrée `localhost` et l'app la montre sans attendre la minute.
- [ ] **Step 3: Service worker arrêté** — sur `signup-form.html`, générer, puis `chrome://serviceworker-internals` → *Stop* sur l'extension, puis envoyer : la bannière apparaît sur `dashboard.html`.
- [ ] **Step 4: Pas de faux positifs** — `login.html` : aucune suggestion ; `commit-page.html` : aucune bannière.
- [ ] **Step 5: Clés API** — `apikey-dialog.html` : la bannière apparaît ~1 s après « Create key » ; « Enregistrer » crée l'élément « Clé API » (titre `Localhost`, clé masquée et protégée) ; rouvrir le dialogue → plus de bannière ; « Ignorer » sur une autre clé → elle ne revient pas de la session ; coller une clé dans « Paste here » → aucune bannière ; verrouiller le coffre de test puis « Create key » → bannière « déverrouille MyPass… », déverrouiller → elle passe à « Enregistrer ».
- [ ] **Step 6: Remettre en place** — arrêter l'app de dev et Vite **par PID** (pas tous les `node`/`mypass.exe`), supprimer `%TEMP%\mypass-e2e` et le coffre de test, relancer le MyPass installé.
- [ ] **Step 7: Non-régression** — `cd src-tauri && cargo test && cargo clippy -- -D warnings` ; `cd crates/mypass-core && cargo test` ; `cd server && cargo test` ; `npm run lint && npm run format:check && npm run build` ; `node src/lib/dedup.check.ts && node src/lib/groups.check.ts && node keepassxc-browser/detect.check.mjs` → tout passe.
- [ ] **Step 8: Commit**

```bash
git checkout -- src-tauri/Cargo.toml src-tauri/gen/schemas
git add keepassxc-browser/test-pages
git commit -m "test(extension): local pages for sign-up and API key end-to-end checks"
```

- [ ] **Step 9: Livraison (seulement sur feu vert explicite de Luca)** — release desktop selon « Release desktop » du CLAUDE.md (version suivante dans `src-tauri/Cargo.toml`, `npm run release`, dépôt sur le conteneur), puis Luca recharge l'extension et fait l'essai réel sur console.mistral.ai.
