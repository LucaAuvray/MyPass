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

Tout ce qui est « serveur » vit sur un conteneur Proxmox (CT 107, Ubuntu 24.04), **hostname `mypass-luca`, IP Tailscale `<container-ip>`**, accès `ssh root@<container-ip>` (clé `id_ed25519`). URL publique tailnet uniquement : `https://<host>.<tailnet>.ts.net` (`tailscale serve` → `http://127.0.0.1:8787`). Le dépôt est public : les vraies valeurs n'y figurent pas, elles sont dans la mémoire locale de Claude (`container-address`) et dans `src-tauri/tauri.conf.json` (endpoint de l'updater).

| Chemin sur le conteneur | Contenu |
|---|---|
| `/var/lib/mypass/vaults/` | **LES COFFRES** : `vault.v<N>.kdbx` (historique versionné, rétention limitée) + `index.json` (`{"current":N}`). Chiffrés, appartiennent à l'utilisateur `mypass`. |
| `/var/lib/mypass/token.hash` | Hash du jeton Bearer d'API. Le jeton en clair n'est nulle part dans ce dépôt. |
| `/usr/local/bin/mypass-server` | Binaire du serveur déployé (service `mypass-server.service`, user `mypass`, port 8787) |
| `/opt/mypass-web/` | Build de la PWA servie (`MYPASS_STATIC_DIR`) ; `/opt/mypass-web.bak-<date>[-sp<N>]/` = versions précédentes |
| `/opt/mypass-downloads/` | `MyPass_<version>_x64_en-US.msi` (le courant et le précédent) + `latest.json` (manifeste de l'updater), servis sous `/download/<nom>` (`MYPASS_DOWNLOAD_DIR`) |
| `/opt/mypass-src/server-build/` | Copie du projet utilisée pour compiler le serveur (cargo installé dans `/root/.cargo`) ; seul `server/` y sert, il ne dépend d'aucun autre crate du dépôt |
| `/etc/systemd/system/mypass-server.service` (+ `.d/`) | Unité du service : user `mypass`, `MYPASS_DATA_DIR=/var/lib/mypass`, `MYPASS_STATIC_DIR`, `MYPASS_DOWNLOAD_DIR`, `UMask=0077`, `ProtectSystem=strict` |

Règles pour toute opération sur le conteneur :
- **Les coffres ne sont jamais dans le dépôt** (`*.kdbx` est ignoré). Ne jamais les rapatrier, afficher ou committer sans demande explicite ; ce sont les vraies données de Luca.
- Lire/diagnostiquer avant de modifier. Copier un fichier en `.bak-<date>` avant de l'éditer. Vérifier après chaque changement (`systemctl status mypass-server`, `journalctl -u mypass-server`, `curl http://127.0.0.1:8787/api/health`) et montrer la sortie.
- Le conteneur n'a **ni node ni wasm-pack** : la PWA se construit sur le PC.
  1. `npm run build:web` (avant tout `npm run tauri build`/`release`, qui écrase `dist/` avec le build desktop), puis `tar czf mypass-web.tgz -C dist .` et `scp` dans `/root/`.
  2. Sur le conteneur : `mkdir /opt/mypass-web.new && tar xzf /root/mypass-web.tgz --no-same-owner -C /opt/mypass-web.new && chown -R root:root /opt/mypass-web.new` (sans `--no-same-owner`, tar échoue sur l'uid Windows).
  3. `mv /opt/mypass-web /opt/mypass-web.bak-<date>` puis `mv /opt/mypass-web.new /opt/mypass-web`. Les PWA ouvertes se rechargent seules sur le nouveau build (service worker `autoUpdate` + `controllerchange` dans `src/main.tsx`).
- Le serveur Rust se compile sur le conteneur : copier les fichiers modifiés de `server/` dans `/opt/mypass-src/server-build/server/` (`.bak-<date>` d'abord), `cargo build --release`, copier `target/release/mypass-server` sur `/usr/local/bin/mypass-server`, `systemctl restart mypass-server`.
- Le `.msi` est construit sur le PC Windows avec `npm run release`, jamais sur le conteneur : voir « Release desktop ».
- Sauvegardes : vzdump nocturne de CT 107 par l'hôte Proxmox `Pve` (03:00, `keep-daily=3,keep-weekly=2`, stockage `sauvegardes`), plus l'historique versionné du serveur et la copie locale du coffre sur chaque PC. Pas d'autre mécanisme (choix de la feuille de route).

## Release desktop

Au démarrage, l'app (build release) lit `https://<host>.<tailnet>.ts.net/download/latest.json` ; si la version annoncée est plus récente, elle propose de l'installer (boîte native), vérifie la signature du `.msi`, se ferme, et le MSI la relance.

1. Incrémenter `version` dans `src-tauri/Cargo.toml` (seule source de la version), committer.
2. `npm run release` : construit le `.msi` signé et écrit `src-tauri/target/release/bundle/msi/latest.json`.
3. Sur le conteneur : `cp -a /opt/mypass-downloads/latest.json /opt/mypass-downloads/latest.json.bak-<date>`.
4. `scp` le `.msi` dans `/opt/mypass-downloads/`, puis `latest.json` en `latest.json.new`, puis `mv latest.json.new latest.json` : le manifeste arrive en dernier, aucun PC ne voit une version sans son `.msi`.
5. Vérifier via le tailnet : `latest.json` annonce la version, et le `.msi` téléchargé a le même sha256 qu'en local.
6. Garder le `.msi` courant et le précédent, supprimer les plus anciens.

**Clé de signature :** `%USERPROFILE%\.tauri\mypass.key` (+ `.pub`), sans mot de passe, jamais dans le dépôt ; copie de secours dans le coffre de Luca. La clé publique est dans `tauri.conf.json` (`plugins.updater.pubkey`). Clé perdue = nouvelle paire, puis réinstallation manuelle du `.msi` sur chaque PC. Un simple `npm run tauri build` échoue sans la clé : utiliser `npm run release` (ou `--no-bundle`).

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
npm run tauri build     # binaire natif (avec --no-bundle ; le .msi signé passe par release)
npm run release         # .msi signé + latest.json (voir « Release desktop »)
```

Rust :
```bash
cd src-tauri && cargo test && cargo clippy      # desktop
cd crates/mypass-core && cargo test             # noyau KDBX/crypto/ops
cd server && cargo test                         # serveur (tests/api.rs, tests/static_files.rs)
```

- Pas de runner de tests frontend (ni vitest ni jest) : Rust est testé (modules `#[cfg(test)]` + `server/tests`), et la logique front pure a des vérifications Node sans dépendance : `node src/lib/dedup.check.ts`, `node src/lib/groups.check.ts`.
- Non-régression avant de livrer : les trois `cargo test`, `cargo clippy` sans nouveau warning, `npm run lint`, `npm run format:check`, `npm run build`.

## Architecture

### Découpage
- `src/` — UI React 19. État : Zustand (`src/stores/{app,database,entries}Store.ts`) + TanStack Query. **Tous les appels vers Rust passent par `src/lib/tauri.ts`.**
- `crates/mypass-core/` — noyau KDBX (lecture/écriture, crypto AES-GCM/ChaCha20, Argon2, XML), générateur, TOTP, ops sur coffre, `merge.rs` (fusion LWW + tombstones). Compile aussi en wasm32. `src-tauri` le ré-exporte comme `kdbx`.
- `crates/mypass-wasm/` — bindings wasm-bindgen (JSON in/out, session `thread_local` qui reflète `DbState`). Sortie dans `crates/mypass-wasm/pkg`, alias `@wasm`.
- `src-tauri/src/` — backend desktop. `commands/` : un fichier par domaine (`database`, `entries`, `groups`, `generator`, `totp`, `import_export`, `browser`, `sync`, `ssh`). **Toute commande doit être ajoutée à `tauri::generate_handler![]` dans `lib.rs`**, sinon elle n'existe pas. `security/` (NaCl box, HIBP k-anonymity, zxcvbn), `ssh/` (agent SSH + clés), `native_messaging.rs`, `updater.rs` (vérification de mise à jour au démarrage).
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
- À chaque démarrage, l'app écrit `%APPDATA%\MyPass\mypass_browser_manifest.json` et `mypass-nhm.bat` (qui lance l'exe courant avec `--native-messaging`) ; `register-nhm.ps1`, lancé une fois à la main, crée les clés `HKCU` de l'hôte `com.mypass.mypass_browser` pour Chrome et Edge (l'app ne touche pas au registre). L'hôte refuse de servir tant que l'intégration n'est pas activée dans Réglages (opt-in, `%APPDATA%\MyPass\browser_settings.json`). Firefox hors périmètre.
- L'extension est `keepassxc-browser/keepassxc-browser/` : KeePassXC-Browser 1.10.3 adaptée (« MyPass Browser », hôte `com.mypass.mypass_browser`), chargée **non empaquetée** dans Chrome (`chrome://extensions` → mode développeur). La `key` (publique) de son `manifest.json` fixe son ID (`bmeobbbilliigohcbhomfphecoocnnda`) quel que soit le dossier, et `allowed_origins` du manifeste natif (`commands/browser.rs`) l'autorise : ne pas changer l'un sans l'autre.
- Le relais natif quitte dès que l'app se ferme, pour ne pas bloquer le remplacement de l'exe par une mise à jour ; l'extension le relance à son prochain usage.

### Import / export
`commands/import_export.rs` → `mypass-core/src/ops/transfer.rs` (partagé avec la PWA) : import JSON MyPass (sans perte) ou CSV lu par ses en-têtes (Google, Apple, KeePassXC, Bitwarden…), dédoublonnage côté front (`src/lib/dedup.ts`) ; export JSON MyPass ou CSV (boîte « Enregistrer sous » native sur desktop).

### Docs de conception
`plans/` (architecture, intégration navigateur, dedup, d'origine) et `docs/superpowers/{specs,plans}/` : agent SSH, cartes d'identité, backend de sync, PWA iPhone, extraction du core, bindings wasm, puis la feuille de route du 2026-10-02 (`specs/2026-10-02-roadmap-design.md`) et ses sous-projets (desktop sur 2 PC, retrait du factice, TOTP, groupes, import/export, durcissement, mise à jour auto). Les lire avant de remettre en cause un choix non évident.

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
