# Mise à jour automatique — Design (sous-projet 7)

> Sous-projet 7 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-05.

## Intention

Publier une version une fois et voir les deux PC se mettre à jour seuls : l'app trouve la nouvelle
version au démarrage, la propose, l'installe et redémarre dessus, sans téléchargement manuel.

**Succès :** après la publication d'une 0.1.2, ce PC et le portable proposent la mise à jour et
redémarrent en 0.1.2. (La roadmap disait 0.1.1 : le 0.1.0 installé n'a pas d'updater, donc la
version qui l'apporte s'installe encore à la main, et seule la suivante prouve l'auto-update.)

## Constat de départ (2026-10-05)

- `src-tauri/tauri.conf.json` : `"version": "0.1.0"`, `"plugins": {}`, aucun updater ; la même
  version est répétée dans `src-tauri/Cargo.toml` et `package.json`.
- `/opt/mypass-downloads/` (servi sous `/download/`) contient `MyPass_0.1.0_x64_en-US.msi` et un
  `latest.json` du 2026-07-13 signé par une clé Tauri restée sur l'ancien PC : aucune clé de
  signature n'existe sur ce PC (`~/.tauri` absent).
- Smart App Control est désactivé sur ce PC et sur le portable : pas besoin d'Authenticode.
- Le portable n'a pas encore MyPass (reste du sous-projet 1).
- Sous Windows, l'updater lance msiexec puis appelle `std::process::exit(0)` : l'app est coupée net.
  L'écriture du coffre est atomique (fichier temporaire puis renommage, `database.rs` /
  `writer.rs`) et une modification non synchronisée est fusionnée à la sync suivante.
- Le relais de l'extension (`run_native_messaging_proxy`, `native_messaging.rs`) reste vivant
  quand l'app se ferme : son fil principal attend sur l'entrée standard de Chrome. Il garde
  `mypass.exe` ouvert et empêcherait le `.msi` de remplacer l'exe sans redémarrage.

## Décisions

| Sujet | Décision |
|---|---|
| Distribution | `latest.json` statique dans `/download/`, à côté du `.msi`. Pas d'endpoint dynamique (un seul OS), pas de GitHub Releases (dépôt privé, conteneur sur le tailnet seulement). |
| Interface | Boîte de dialogue native côté Rust, en français. Aucun code front, aucune permission updater donnée à la webview. |
| Clé de signature | Sans mot de passe, dans `%USERPROFILE%\.tauri\mypass.key`, hors dépôt. Luca en garde une copie dans son coffre MyPass. |
| Version | Une seule source : `src-tauri/Cargo.toml`. Retirée de `tauri.conf.json` et de `package.json`. |
| Authenticode | Hors périmètre (Smart App Control coupé sur les deux PC). |
| Moment de la vérification | Au démarrage seulement, builds release seulement. |

## 1. L'app (`src-tauri/`)

**Plugin :** crate `tauri-plugin-updater`, enregistrée dans `lib.rs`. Pas de paquet npm, aucune
permission `updater:*` dans `capabilities/default.json`.

**`tauri.conf.json` :**
- `bundle.createUpdaterArtifacts: true` (génère `MyPass_<v>_x64_en-US.msi.sig`) ;
- `plugins.updater` : `pubkey` (clé publique, committée), `endpoints:
  ["https://<host>.<tailnet>.ts.net/download/latest.json"]`, `windows.installMode:
  "passive"` ;
- `version` retiré (Tauri prend alors celle de `Cargo.toml`).

**`src-tauri/src/updater.rs`** — `check_on_startup(app)`, appelé dans `setup`, ne fait rien si
`cfg!(debug_assertions)` (`tauri dev` ne propose jamais d'installer le `.msi` publié) :

| Cas | Comportement |
|---|---|
| À jour, hors ligne, serveur injoignable, manifeste invalide | Rien à l'écran ; la raison va dans le log. |
| Nouvelle version | Boîte native modale sur la fenêtre principale : « MyPass {nouvelle} est disponible (installée : {actuelle}). L'app va se fermer pour l'installer. », boutons **Installer** / **Plus tard**. |
| Installer | Téléchargement + vérification de signature, puis msiexec en mode passif (UAC, installation par machine) ; l'app se ferme, le MSI la relance. |
| Échec du téléchargement ou de la signature | Boîte d'erreur avec le message ; l'app continue. |
| Plus tard | Rien ; la question revient au prochain lancement. |

La relance après installation est faite par le MSI de Tauri : à vérifier dans le code de la crate
(arguments passés à msiexec) au moment de l'implémentation. Si le MSI ne relance pas l'app, pas de
contournement maison : Luca la relance à la main, et le critère de succès devient « propose la
mise à jour et tourne en 0.1.2 après relance ».

**Relais de l'extension :** `run_native_messaging_proxy` termine le processus quand la connexion
au bridge de l'app se ferme. L'extension relance le relais au prochain usage.

## 2. Signature et release

**Paire de clés** (une fois) : `tauri signer generate`, sans mot de passe, écrite dans
`%USERPROFILE%\.tauri\mypass.key` (+ `.pub`). La clé privée n'est jamais affichée ni copiée dans le
dépôt.

**`npm run release`** (`scripts/release.mjs`, Node) :
1. lit `version` dans `src-tauri/Cargo.toml` ;
2. lance `npm run tauri build` avec `TAURI_SIGNING_PRIVATE_KEY` = chemin de la clé et
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` vide ;
3. écrit `src-tauri/target/release/bundle/msi/latest.json` :
   `{ version, pub_date, platforms: { "windows-x86_64": { signature, url } } }`, `signature` =
   contenu du `.msi.sig`, `url` =
   `https://<host>.<tailnet>.ts.net/download/MyPass_<v>_x64_en-US.msi` ;
4. s'arrête avec un message clair si la clé ou le `.sig` manque.

**Procédure de release** (écrite dans `CLAUDE.md`, qui remplace la ligne « aucun plugin updater
n'est configuré ») :
1. incrémenter `version` dans `src-tauri/Cargo.toml`, committer ;
2. `npm run release` ;
3. sur le conteneur : `cp latest.json latest.json.bak-<date>` ;
4. copier le `.msi` dans `/opt/mypass-downloads/`, puis `latest.json` en `latest.json.new` et
   `mv` par-dessus : le manifeste arrive en dernier, aucun PC ne voit une version sans son `.msi` ;
5. vérifier via le tailnet : `latest.json` annonce la version, le `.msi` téléchargé a le même
   sha256 qu'en local ;
6. garder le `.msi` courant et le précédent, supprimer les plus anciens.

Aucun changement du serveur.

## 3. Vérification

- **Non-régression :** `cargo test` et `cargo clippy --all-targets -- -D warnings` dans
  `src-tauri`, `npm run lint`, `npm run build`.
- **Pas de nouveau test unitaire :** la logique est l'appel au plugin et une boîte de dialogue, le
  relais finit par `process::exit` ; le contrôle est l'E2E.
- **E2E local**, rien d'installé, jamais la prod : build release avec `--config` (identifiant
  `com.mypass.e2e`, endpoint `http://127.0.0.1:<port>/download/latest.json`,
  `dangerousInsecureTransportProtocol: true` dans cette surcharge seulement), `APPDATA` temporaire,
  serveur local avec `MYPASS_DOWNLOAD_DIR` temporaire ; boîte pilotée par UI Automation :
  - serveur éteint → pas de boîte, app utilisable ;
  - `latest.json` en 9.9.9 → boîte avec les bonnes versions ; « Plus tard » → app utilisable ;
  - `.msi` altéré → « Installer » → erreur de signature, msiexec jamais lancé, app vivante ;
  - relais : `mypass.exe --native-messaging` lancé entrée ouverte, app fermée → le relais se
    termine en moins d'une seconde.
- **Déploiement réel :**
  1. release 0.1.1 ; Luca l'installe à la main sur ce PC et sur le portable (qui récupère le coffre
     depuis le serveur : fin du sous-projet 1) ;
  2. release 0.1.2 (seule la version change) ; sur chaque PC : boîte → Installer → UAC → l'app
     redémarre en 0.1.2 ; version de l'exe vérifiée sur ce PC, confirmée par Luca sur le portable ;
  3. en cas d'échec : installation manuelle depuis `/download/` ; le coffre (`%APPDATA%`) n'est pas
     touché par le MSI.

## Hors périmètre

- Signature Authenticode.
- Vérification périodique pendant que l'app tourne.
- Notes de version dans la boîte, version affichée dans l'app.
- CI.
- PWA (mise à jour par son service worker).
