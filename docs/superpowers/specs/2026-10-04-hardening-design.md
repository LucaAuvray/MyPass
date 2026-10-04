# Durcissement — Design (sous-projet 6)

> Sous-projet 6 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-04.

## Intention

Qu'un gestionnaire de mots de passe ait les protections qu'il annonce : une page piégée ne peut
rien charger ni exfiltrer, l'app ne parle à aucun tiers en dehors de HIBP, le mot de passe maître
ne traîne pas en mémoire après verrouillage, et les coffres du serveur ne sont lisibles que par
leur service.

**Succès :** le desktop et la PWA fonctionnent sous une CSP stricte sans aucune violation en
console ; plus aucun appel à Google ; `lock` efface réellement mot de passe et fichier-clé ;
clippy propre sur les quatre crates ; en prod, la PWA est servie avec ses en-têtes de sécurité et
les coffres sont en `600`.

## Constat de départ (2026-10-04)

- `src-tauri/tauri.conf.json` : `"csp": null`.
- `index.html` charge Inter, DM Sans et JetBrains Mono depuis `fonts.googleapis.com` /
  `fonts.gstatic.com` à chaque lancement (desktop et PWA). Aucune classe `italic` dans `src/`.
- Plugins Tauri : `fs` et `shell` sont initialisés et autorisés (`fs:default`, `shell:default`)
  mais rien ne s'en sert. `dialog` n'est utilisé que depuis Rust (`import_export.rs`,
  `DialogExt`) ; le paquet npm `@tauri-apps/plugin-dialog` n'est importé nulle part. Le front
  n'appelle que `writeText` et `clear` du presse-papiers, jamais `readText`.
- `shell` et `opener` injectent chacun un script qui intercepte les clics sur `<a target="_blank">`
  et ouvre le lien : un clic sur l'URL d'une entrée ouvre vraisemblablement deux onglets.
- `DbState.password_hash` contient le mot de passe maître **en clair** (`password.as_bytes()`).
  `lock_database` remet `password_hash` et `keepass_file` à `None` sans rien mettre à zéro, et
  oublie `keyfile_data` ; son commentaire « Zero out sensitive data » est faux.
- Le format du coffre dérive la clé Argon2 du mot de passe avec un sel tiré à chaque écriture, et
  la sync déchiffre le coffre distant avec son propre sel : le mot de passe doit rester en mémoire
  tant que le coffre est ouvert ; on ne peut pas le remplacer par une clé dérivée.
- `#![allow(dead_code)]` déjà retiré au sous-projet 2. Clippy : 8 avertissements dans
  `src-tauri`, 4 dans `mypass-core`, 0 dans `mypass-wasm` et `server`.
- Serveur : aucune CSP ni en-tête de sécurité sur la PWA. Le middleware `static_cache_policy`
  (`server/src/lib.rs`) enveloppe déjà toutes les réponses quand `MYPASS_STATIC_DIR` est défini.
- Conteneur : coffres et `index.json` en `644`, `token.hash` en `644`, `token.hash.bak-2026-10-02`
  en `664`, `vaults/` en `775` — dans `/var/lib/mypass` en `700`, donc pas lisibles par un autre
  utilisateur aujourd'hui, mais sans défense en profondeur. Unité sans `UMask`.
  `/opt/mypass-src/server` obsolète et fichier parasite `/opt/mypass-src/server-build/$null`.

## Décisions

| Sujet | Décision |
|---|---|
| Périmètre CSP | Desktop **et** PWA (la PWA déchiffre le coffre dans un navigateur). |
| Où vit la CSP | Desktop : `tauri.conf.json`. PWA : en-tête HTTP posé par le serveur. Pas de `<meta>` dans `index.html` (il s'appliquerait aussi au desktop et au mock, et ne permet pas `frame-ancestors`). |
| Polices | Embarquées via `@fontsource-variable/{inter,dm-sans,jetbrains-mono}`, graisses droites seulement. |
| Mot de passe maître | `Zeroizing`, effacé au verrouillage. Pas de clé dérivée (impossible avec le format, voir constat). |
| PWA / wasm | Pas de `Zeroizing` : le JS garde sa propre copie du mot de passe, non effaçable ; zéroïser la seule copie wasm ne réduit pas l'exposition. |
| Entrées déchiffrées | Hors périmètre : libérées mais pas mises à zéro au verrouillage (commentaire `ponytail:`). |
| Sauvegardes `.bak` du conteneur | Conservées. |

## 1. Desktop (`src-tauri/`, `index.html`, `src/`)

**CSP** (`app.security.csp`, forme objet) :

| Directive | Valeur |
|---|---|
| `default-src` | `'self'` |
| `script-src` | `'self'` |
| `style-src` | `'self' 'unsafe-inline'` |
| `img-src` | `'self' data:` |
| `font-src` | `'self'` |
| `connect-src` | `ipc: http://ipc.localhost https://api.pwnedpasswords.com` |
| `object-src`, `base-uri`, `form-action`, `frame-ancestors` | `'none'` |

- Tauri ajoute lui-même les hash de ses scripts d'initialisation à `script-src`.
- `'unsafe-inline'` en `style-src` : `sonner` injecte une balise `<style>` à l'exécution. Tauri
  ajoute un nonce à `style-src`, ce qui fait ignorer `'unsafe-inline'` par le navigateur ; si les
  toasts sont bloqués en E2E, ajouter `"dangerousDisableAssetCspModification": ["style-src"]`.
- Si l'IPC passe par `https://ipc.localhost` (fenêtre en `useHttpsScheme`), l'ajouter à
  `connect-src`. Les deux points se tranchent sur la console, pas a priori.

**Polices :** supprimer les trois balises Google de `index.html` ; importer
`@fontsource-variable/inter`, `@fontsource-variable/dm-sans` et
`@fontsource-variable/jetbrains-mono`. Les noms de famille de `src/index.css` (`--font-sans`,
`--font-display`, `--font-mono`) deviennent ceux exposés par ces paquets (`"Inter Variable"`,
`"DM Sans Variable"`, `"JetBrains Mono Variable"`), avec les mêmes replis.

**Permissions :**
- retirer `tauri-plugin-fs` et `tauri-plugin-shell` (crates, `.plugin(...)` dans `lib.rs`,
  `fs:default` et `shell:default` dans `capabilities/default.json`) ;
- retirer les paquets npm `@tauri-apps/plugin-fs`, `@tauri-apps/plugin-shell` et
  `@tauri-apps/plugin-dialog` ; garder la crate `tauri-plugin-dialog` (Rust) et retirer la
  permission `dialog:default` (aucun appel depuis le front) ;
- retirer `clipboard-manager:allow-read-text`.
- `opener` reste : c'est lui qui ouvre les liens d'entrée dans le navigateur.

## 2. Secrets en mémoire et clippy

**`DbState`** (`src-tauri/src/commands/database.rs`) :
- `password_hash: Option<Vec<u8>>` → `master_password: Option<Zeroizing<String>>` ;
- `keyfile_data: Option<Vec<u8>>` → `Option<Zeroizing<Vec<u8>>>` ;
- `DbState::lock_vault(&mut self)` : met `master_password`, `keyfile_data` et `keepass_file` à `None`
  et `is_open` à `false` ; `lock_database` l'appelle (après `clear_session_approvals`) ;
- `save()` et `commands/sync.rs` lisent `master_password` directement (fin du
  `String::from_utf8(clone)`) ; toute copie reste un `Zeroizing` ;
- le commentaire de `lock` dit ce qui est effacé, et un `ponytail:` nomme la limite : les entrées
  déchiffrées (`KeePassFile`) sont libérées sans mise à zéro ; évolution = `Zeroize` sur les
  types XML.

**Clippy :** corriger les 12 avertissements. Critère : `cargo clippy --all-targets -- -D warnings`
passe dans `crates/mypass-core`, `crates/mypass-wasm`, `src-tauri` et `server`.

## 3. Serveur (`server/`, conteneur)

**En-têtes** ajoutés par `static_cache_policy` à toutes les réponses :
- `Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval';
  style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self';
  connect-src 'self' https://api.pwnedpasswords.com; worker-src 'self'; manifest-src 'self';
  object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: no-referrer`

Sans `MYPASS_STATIC_DIR`, le middleware n'est pas monté et rien ne change (API seule).

**Conteneur** (lire avant, `.bak-<date>` avant toute édition, vérifier après) :
- compiler le serveur dans `/opt/mypass-src/server-build`, sauvegarder puis remplacer
  `/usr/local/bin/mypass-server`, redémarrer ;
- drop-in `/etc/systemd/system/mypass-server.service.d/umask.conf` : `[Service]` `UMask=0077` ;
- une fois : `chmod 600` sur les fichiers de `/var/lib/mypass/vaults/` et sur
  `/var/lib/mypass/token.hash*`, `chmod 700` sur `vaults/` ;
- supprimer `/opt/mypass-src/server` et `/opt/mypass-src/server-build/$null`.

## 4. Vérification

- **Tests automatiques :**
  - `DbState::lock_vault` efface mot de passe, fichier-clé et coffre ;
  - `server/tests/static_files.rs` : `index.html` et une réponse `/api` portent la CSP (avec
    `'wasm-unsafe-eval'` et `frame-ancestors 'none'`), `nosniff` et `no-referrer`.
- **Non-régression :** `cargo test` (core, src-tauri, server, wasm), `clippy -D warnings` sur les
  quatre crates, `npm run lint`, `npm run build`, `npm run build:web`, `npm run smoke:wasm`.
- **E2E desktop**, jamais sur la prod ni sur un vrai coffre : build release sous un identifiant de
  test, coffre et serveur de sync locaux, piloté par CDP. Vérifier :
  - déverrouillage, sync, HIBP, copie d'un mot de passe, toast ;
  - un clic sur l'URL d'une entrée ouvre un seul onglet ;
  - les polices viennent de l'app ;
  - aucune violation CSP dans la console.
- **E2E PWA :** même parcours sur le serveur local (`MYPASS_STATIC_DIR=../dist`), plus le
  service worker installé et le wasm chargé sous CSP.
- **Extension :** processus séparé sans webview, non concerné par la CSP. Vérifiée par Luca après
  réinstallation du `.msi`.
- **Prod :**
  - `curl -I` montre les en-têtes ;
  - `systemctl show -p UMask` vaut `0077` ;
  - les coffres sont en `600` ;
  - `status`, `journalctl` et `/api/health` sont OK ;
  - le premier `vault.v<N>` écrit après déploiement est en `600`.

## Hors périmètre

- Mise à zéro des entrées déchiffrées au verrouillage.
- Jeton de sync en clair dans `%APPDATA%` (déjà marqué `ponytail:` dans `sync.rs`).
- Signature du binaire et mise à jour automatique (sous-projet 7).
- Ménage des anciennes sauvegardes `.bak` du conteneur.
