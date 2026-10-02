# CLAUDE.md

Guide pour Claude Code sur ce dépôt. Réponds en français ; code, commits et identifiants en anglais comme dans le reste du projet.

MyPass : gestionnaire de mots de passe auto-hébergé. App desktop Tauri v2 (React 19 / TypeScript + Rust), PWA web (Rust compilé en wasm) et serveur de sync Axum. Source de vérité = un fichier **KDBX4** (compatible KeePass), pas de SQL.

- Dépôt : https://github.com/LucaAuvray/MyPass (privé, branche `main`, import initial depuis le conteneur le 2026-10-02).
- Poste de dev : Windows 11, PowerShell + Git Bash. Dossier de travail : `C:\Users\lucaa\Documents\Projet\MyPass`.

## Outils disponibles

- **MCP context7** : à utiliser pour la doc à jour de toute lib/framework (Tauri v2, React 19, Tailwind v4, shadcn/Base UI, TanStack Query, Zustand, axum 0.8, wasm-bindgen, react-i18next…). Appeler `resolve-library-id` puis `query-docs` plutôt que se fier à la mémoire : les versions du projet sont récentes (Tauri 2, axum 0.8, Tailwind 4).
- `gh` (déjà authentifié comme LucaAuvray, protocole SSH mais **remote en HTTPS**), `ssh`, `scp`. Préférer les CLI à un MCP.
- `.roo/rules-architect/rules.md` est un reste de Roo/ZooCode (autre assistant), non contraignant.

## Infrastructure : le conteneur LXC (prod / sync)

Tout ce qui est « serveur » vit sur un conteneur Proxmox (CT 107, Ubuntu 24.04), **hostname `mypass-luca`, IP Tailscale `100.64.46.117`**, accès `ssh root@100.64.46.117` (clé `id_ed25519`). URL publique tailnet uniquement : `https://mypass-luca.tail7687c9.ts.net` (`tailscale serve` → `http://127.0.0.1:8787`).

| Chemin sur le conteneur | Contenu |
|---|---|
| `/var/lib/mypass/vaults/` | **LES COFFRES** : `vault.v<N>.kdbx` (historique versionné, rétention limitée) + `index.json` (`{"current":N}`). Chiffrés, appartiennent à l'utilisateur `mypass`. |
| `/var/lib/mypass/token.hash` | Hash du jeton Bearer d'API. Le jeton en clair n'est nulle part dans ce dépôt. |
| `/usr/local/bin/mypass-server` | Binaire du serveur déployé (service `mypass-server.service`, user `mypass`, port 8787) |
| `/opt/mypass-web/` | Build de la PWA servie (`MYPASS_STATIC_DIR`) ; `/opt/mypass-web.old/` = version précédente |
| `/opt/mypass-downloads/` | `MyPass_0.1.0_x64_en-US.msi` + `latest.json`, servis sous `/download/<nom>` (`MYPASS_DOWNLOAD_DIR`) |
| `/opt/mypass-src/server-build/` | Copie du projet utilisée pour compiler le serveur (cargo installé dans `/root/.cargo`) |
| `/opt/mypass-src/server/` | Ancienne copie du serveur, **obsolète** (sans /download ni politique de cache) — à supprimer |

Règles pour toute opération sur le conteneur :
- **Les coffres ne sont jamais dans le dépôt** (`*.kdbx` est ignoré). Ne jamais les rapatrier, afficher ou committer sans demande explicite ; ce sont les vraies données de Luca.
- Lire/diagnostiquer avant de modifier. Copier un fichier en `.bak-<date>` avant de l'éditer. Vérifier après chaque changement (`systemctl status mypass-server`, `journalctl -u mypass-server`, `curl http://127.0.0.1:8787/api/health`) et montrer la sortie.
- Le conteneur n'a **ni node ni wasm-pack** : la PWA se construit sur le PC (`npm run build:web`) puis `dist/` est copié dans `/opt/mypass-web` (sauvegarder l'ancien en `mypass-web.old`). Le serveur Rust se compile sur le conteneur (`cargo build --release` dans `server/`), puis remplacer `/usr/local/bin/mypass-server` et `systemctl restart mypass-server`.
- Le `.msi` est construit sur le PC Windows (`npm run tauri build`), jamais sur le conteneur. `latest.json` est servi, mais **aucun plugin updater n'est configuré** dans `src-tauri/tauri.conf.json` à ce jour.
- Pas de sauvegarde automatique de `/var/lib/mypass` aujourd'hui (aucun cron) : à mettre en place.

## Commandes

```bash
npm install
npm run dev             # Vite seul (port 1420), backend MOCK navigateur
npm run tauri dev       # app complète (Rust + hot reload)
npm run build           # build:wasm + tsc -b + vite build
npm run build:wasm      # wasm-pack build crates/mypass-wasm (wasm-pack REQUIS)
npm run dev:web         # Vite mode web : coffre wasm + sync serveur, sans mock
npm run build:web       # build PWA déployée sur le serveur de sync
npm run smoke:wasm      # smoke test Node du module wasm
npm run lint            # eslint, --max-warnings 0
npm run format[:check]  # prettier sur src/
npm run tauri build     # binaire natif / .msi
```

Rust :
```bash
cd src-tauri && cargo test && cargo clippy      # desktop
cd crates/mypass-core && cargo test             # noyau KDBX/crypto/ops
cd server && cargo test                         # serveur (tests/api.rs, tests/static_files.rs)
```

- Pas de runner de tests frontend (ni vitest ni jest) : seul Rust est testé (modules `#[cfg(test)]` + `server/tests`).
- Le script `tauri` de `package.json` préfixe le PATH avec `C:\Users\Utilisateur\.cargo\bin` (ancien PC). **Sur ce PC l'utilisateur est `lucaa`** : vérifier que cargo est installé et corriger ce chemin si `npm run tauri` ne trouve pas `cargo`.

## Architecture

### Découpage
- `src/` — UI React 19. État : Zustand (`src/stores/{app,database,entries}Store.ts`) + TanStack Query. **Tous les appels vers Rust passent par `src/lib/tauri.ts`.**
- `crates/mypass-core/` — noyau KDBX (lecture/écriture, crypto AES-GCM/ChaCha20, Argon2, XML), générateur, TOTP, ops sur coffre, `merge.rs` (fusion LWW + tombstones). Compile aussi en wasm32. `src-tauri` le ré-exporte comme `kdbx`.
- `crates/mypass-wasm/` — bindings wasm-bindgen (JSON in/out, session `thread_local` qui reflète `DbState`). Sortie dans `crates/mypass-wasm/pkg`, alias `@wasm`.
- `src-tauri/src/` — backend desktop. `commands/` : un fichier par domaine (`database`, `entries`, `groups`, `generator`, `totp`, `import_export`, `passkeys`, `browser`, `sync`, `ssh`). **Toute commande doit être ajoutée à `tauri::generate_handler![]` dans `lib.rs`**, sinon elle n'existe pas. `security/` (NaCl box, HIBP k-anonymity, zxcvbn), `ssh/` (agent SSH + clés), `native_messaging.rs`.
- `server/` — serveur de sync Axum : `GET/PUT /api/vault` (ETag / If-Match / 409), `/api/vault/versions[/{n}]`, `/api/health`, auth Bearer, blobs chiffrés versionnés. Sert aussi la PWA (`MYPASS_STATIC_DIR`) et `/download` ; `/api/*` inconnu = 404 (jamais l'index SPA).
- Env du serveur : `MYPASS_DATA_DIR`, `MYPASS_BIND` (défaut `0.0.0.0:8787`), `MYPASS_STATIC_DIR`, `MYPASS_DOWNLOAD_DIR`.

### Critique : le mode navigateur utilise un backend mock
`src/lib/tauri.ts` choisit le backend de `getInvoke()` : Tauri natif (`"__TAURI__" in window`) → web (`--mode web`, `src/lib/web.ts`) → **mock en mémoire**. `npm run dev` seul ne teste donc ni KDBX ni crypto : toute modif touchant la logique coffre/crypto se vérifie avec `npm run tauri dev` (ou `dev:web` pour le wasm).

### Sync (auto-hébergée)
- Desktop : `src-tauri/src/commands/sync.rs` + `mypass-core/src/merge.rs`.
- Web : `src/lib/web.ts` reproduit le moteur (drapeau dirty, chaîne de push sérialisée, 409 → merge → retry). **Les formes JSON doivent rester identiques à l'IPC Tauri** (parité desktop/web).
- Le serveur ne voit que des blobs KDBX chiffrés ; il écrit blob puis index (atomique) et élague l'historique.

### Intégration navigateur
Même protocole que KeePassXC-Browser (NaCl box + Native Messaging) pour réutiliser son extension.
- Le même binaire sert d'hôte : lancé avec `--native-messaging` il exécute `run_native_messaging_host()` (stdin/stdout JSON) au lieu d'ouvrir une fenêtre (`src-tauri/src/main.rs`).
- `register-nhm.ps1` / `mypass-nhm.bat` (Windows) enregistrent l'hôte pour Chrome/Edge/Firefox.
- `keepassxc-browser/keepassxc-browser/` = clone vendored de l'extension amont, en **référence uniquement** (non buildé). `extension/` = notre extension MV3 minimale ; sa clé `extension/dev-key.pem` est **ignorée par git** (garder une copie hors dépôt, elle fixe l'ID de l'extension).

### Import / export
`commands/import_export.rs` + `src/lib/dedup.ts` : import CSV, 1PUX, Bitwarden, Google, Apple, Proton Pass avec dédoublonnage ; export CSV/JSON/XML/HTML.

### Docs de conception
`plans/` (architecture, intégration navigateur, dedup) et `docs/superpowers/{specs,plans}/` (agent SSH, cartes d'identité/documents, backend de sync, PWA iPhone, extraction du core/vault-ops, bindings wasm). Les lire avant de remettre en cause un choix non évident.

## Conventions frontend
- Alias `@/` → `src/` (`vite.config.ts` + `tsconfig.json`).
- shadcn/ui (`style: base-nova`, Base UI, Tailwind v4, `src/components/ui/`) : ajouter les primitives via la CLI shadcn, pas à la main (voir `components.json`).
- i18n `react-i18next`, traductions EN + FR dans `src/i18n/` : aucune chaîne utilisateur en dur.

## Sécurité — à ne pas faire
- Ne jamais committer : `*.kdbx`, `*.pem`, jetons, clé privée de signature Tauri, exports CSV de mots de passe (`Google Passwords*.csv`). Voir `.gitignore`.
- Ne pas logger ni afficher de secrets (mots de passe, clés maîtresses, jeton Bearer, contenu de coffre).
- Les `-----BEGIN OPENSSH PRIVATE KEY-----` présents dans le dépôt sont des fixtures de test/mock, pas de vraies clés.

## Git
- Push via HTTPS (le remote est `https://github.com/LucaAuvray/MyPass.git`) ; la clé d'hôte SSH de GitHub n'est pas connue sur ce PC.
- Commits en français ou anglais, courts ; ne pas pousser sans demande de Luca.
