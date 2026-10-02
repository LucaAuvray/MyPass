# Feuille de route — chantier restant MyPass

> Document de découpage (2026-10-02). Chaque sous-projet suit son propre cycle :
> `superpowers:brainstorming` → spec dans `docs/superpowers/specs/` → `superpowers:writing-plans`
> → plan dans `docs/superpowers/plans/` → implémentation. Mettre à jour le tableau de suivi
> en bas à chaque étape franchie.
>
> **Pour reprendre dans une nouvelle session :** lire ce fichier, prendre le premier
> sous-projet non terminé du tableau de suivi, lancer `superpowers:brainstorming` dessus.

## Objectif

Luca utilise MyPass au quotidien, à la place d'un autre gestionnaire, sur **ses deux PC
Windows (fixe + portable) et son téléphone (PWA)**, synchronisés par `mypass-server`, sans
risque de perte de données ni fonction factice à l'écran.

## État des lieux (audit du 2026-10-02)

**Vérifié, fonctionne :**
- `mypass-core` : 41/41 tests ; `server` : 16/16 tests (lancés sur le conteneur, sources
  identiques au dépôt).
- Serveur de prod : actif depuis 22 j, `/api/health` = 200, `/api/vault` sans jeton = 401,
  aucun warning journal sur 7 j. Dernière écriture de coffre : 2026-07-12 (`vault.v7`).
- Front : `tsc` OK (hors `@wasm`, non construit), `npm run lint` OK, clés i18n EN/FR complètes.
- Moteurs de sync desktop (`src-tauri/src/commands/sync.rs`) et web (`src/lib/web.ts`).
- Sauvegarde : vzdump nocturne de CT 107 par l'hôte Proxmox `Pve` (job quotidien 03:00,
  `keep-daily=3,keep-weekly=2`, stockage `sauvegardes` = `/mnt/sauvegardes`).

**Ne fonctionne pas ou est factice :** détaillé dans chaque sous-projet ci-dessous.

## Décisions actées

| Sujet | Décision |
|---|---|
| Appareils cibles | 2 PC Windows (desktop Tauri) + téléphone (PWA), tous synchronisés |
| Passkeys | **Retirées.** Pas d'implémentation WebAuthn. |
| Navigateur | **Chrome** (Edge suit gratuitement, même mécanisme). Firefox hors périmètre. |
| Sauvegardes | Pas de nouveau mécanisme : vzdump nocturne + historique versionné du serveur + copie locale sur chaque PC. |
| CLAUDE.md | Réécrit par Luca (non commité) : corrections reportées au sous-projet 8. |

## Contraintes transverses

- **Parité desktop / web / mock** : toute commande ajoutée ou retirée → `tauri::generate_handler![]`
  (`src-tauri/src/lib.rs`), cas mock (`src/lib/tauri.ts`), cas web (`src/lib/web.ts`) si la
  fonction a un sens en PWA. Formes JSON identiques à l'IPC Tauri.
- Chaînes UI uniquement via i18next (`src/i18n/en.json` + `fr.json`).
- **Les coffres de prod ne sont jamais touchés par un test automatisé** ; seuls les E2E
  manuels parlent au vrai serveur. Jamais de `.kdbx` dans le dépôt.
- Sur le conteneur : lire avant de modifier, `.bak-<date>` avant édition, vérifier après
  (`systemctl status`, `journalctl`, `curl /api/health`) et montrer la sortie.
- Non-régression avant chaque fin de sous-projet : `cargo test` (`crates/mypass-core`,
  `src-tauri`, `server`), `cargo clippy` sans nouveau warning, `npm run lint`, `npm run build`.
- Préférer la suppression à l'ajout ; tout raccourci assumé porte un commentaire `ponytail:`
  qui nomme sa limite et la voie d'évolution.

## Ordre et dépendances

```
0 Poste de dev ──► 1 Desktop 2 PC ──► 2 Retrait du factice ──┬─► 3 TOTP ──────────┐
                                                             ├─► 4 Groupes ───────┼─► 6 Durcissement ─► 7 Mise à jour auto ─► 8 Docs
                                                             └─► 5 Import/export ─┘
```

- 0 conditionne tout (rien ne compile ici sans Rust ni wasm-pack).
- 1 avant le reste : sans lui, le desktop n'est pas utilisable.
- 2 avant 3-5 : moins de surface avant d'en ajouter.
- 3, 4, 5 sont indépendants entre eux.
- 6 (CSP) après 3-5 : la CSP peut casser des appels, on la règle une fois les fonctions en place.
- 7 après 1 : il faut un `.msi` fonctionnel à distribuer.

---

## 0 — Poste de dev opérationnel

**Objectif :** pouvoir compiler, tester et packager MyPass sur ce PC (utilisateur `lucaa`).

**Constat :** ni `cargo`, ni `rustc`, ni `rustup`, ni Visual Studio Build Tools, ni binaire
`wasm-pack`. `package.json:12` préfixe le PATH par `C:\Users\Utilisateur\.cargo\bin` (ancien PC).
`npm ci` signale des scripts d'installation bloqués par npm 11 (`npm install-scripts ls`) : le
paquet npm `wasm-pack` télécharge son binaire en postinstall.
**Smart App Control était actif** (`HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState = 1`,
politique `{0283ac0f-fff1-49ae-ada1-8a933130cad6}`) : il bloquait tout exécutable non signé produit
par cargo (os error 4551) **et le `mypass.exe` installé** (non signé). Désactivé par Luca sur le PC de
dev le 2026-10-02 (aucune exemption par dossier n'existe).

**Périmètre :**
- Visual Studio Build Tools (charge « Desktop development with C++ » : MSVC + Windows SDK).
- `rustup`, toolchain stable `x86_64-pc-windows-msvc`, cible `wasm32-unknown-unknown`.
- `wasm-pack` (autoriser le postinstall npm ou `cargo install wasm-pack`).
- `package.json` : retirer le préfixe PATH codé en dur du script `tauri` (rustup ajoute
  `%USERPROFILE%\.cargo\bin` au PATH utilisateur).

**Fini quand :** `cargo test` + `cargo clippy` dans `src-tauri`, `npm run build`,
`npm run build:web`, `npm run smoke:wasm` passent, et `npm run tauri dev` ouvre l'app.

**Résultat (2026-10-02) :** VS Build Tools 2026 (MSVC + SDK 10.0.26100), rustc 1.99.0,
cible wasm32, wasm-pack 0.15.0 (postinstall autorisé via `allowScripts` dans `package.json`).
`src-tauri` : 26/26 tests ; clippy passe avec **11 warnings de style préexistants** (état de
référence, nettoyés au sous-projet 6). Builds desktop et web, smoke wasm et `tauri dev` OK.

## 1 — Desktop utilisable sur deux PC

**Objectif :** installer le `.msi` sur le fixe et le portable, chacun avec sa copie locale du
coffre, synchronisée avec le serveur et le téléphone.

**Constat :**
- `src/views/UnlockView.tsx:17` : chemin `"mypass-vault.kdbx"` codé en dur et relatif, résolu
  depuis le dossier courant du process (probablement `C:\Program Files\MyPass`, non inscriptible).
  Aucun coffre n'existe sur ce PC alors que l'app est installée.
- Pas de sélecteur de fichier : impossible d'ouvrir un `.kdbx` existant. L'association `.kdbx`
  du `.msi` est ignorée (`src-tauri/src/main.rs` ne lit pas le chemin passé en argument).
- `src/hooks/useDatabase.ts:73-74` lit `error.message`, mais Tauri rejette avec une chaîne :
  un mauvais mot de passe n'affiche rien sur desktop.
- Deuxième PC : la sync se configure dans `SyncView`, **après** déverrouillage d'un coffre local.
  Aucun chemin simple pour « récupérer mon coffre depuis le serveur » sur un PC neuf.

**Périmètre :**
- **Un seul coffre par PC** (décision du 2026-10-02), toujours dans `%APPDATA%\MyPass\`
  (même convention que `sync.json` et la config navigateur). Pas de sélecteur de fichier.
- Premier lancement sur un PC neuf : récupérer le coffre depuis le serveur (URL + jeton +
  mot de passe maître), l'écrire en local, pré-remplir la config de sync.
- `create_database` ne doit plus jamais écraser un coffre existant.
- Erreurs : normaliser le rejet **une seule fois** dans `tauriCommand` (`src/lib/tauri.ts:461`)
  pour que tous les appelants reçoivent une `Error`.
- Association `.kdbx` retirée du `.msi`.
- Détail : `docs/superpowers/specs/2026-10-02-desktop-two-pcs-design.md`.

**Prérequis sur chaque PC cible :** le `.msi` n'est pas signé, donc bloqué si Smart App Control
est actif (vérifier `VerifiedAndReputablePolicyState`). Soit le désactiver sur ce PC, soit signer
l'app avec un certificat reconnu (voir sous-projet 7).

**Hors périmètre :** interface de fichier clé (supporté par le backend, aucun besoin exprimé).

**Références :** `docs/superpowers/specs/2026-07-09-sync-backend-design.md`,
`docs/superpowers/plans/2026-07-10-desktop-sync.md`.

**Fini quand :** le `.msi` est installé sur les deux PC ; une entrée créée sur le fixe apparaît
sur le portable et le téléphone en ≤ 60 s ; un mauvais mot de passe affiche une erreur.

**Résultat (2026-10-02) :** livré sur la branche `chantier-restant` (plan
`docs/superpowers/plans/2026-10-02-desktop-two-pcs.md`, revue indépendante + corrections).
E2E validé sur ce PC + téléphone : récupération du vrai coffre (octets identiques à `vault.v7`),
erreurs jeton / mot de passe, création sur PC → téléphone (v8) et suppression téléphone → PC (v9)
sans clic. **Reste : installer et récupérer le coffre sur le deuxième PC.**
Découvertes en route :
- Le jeton serveur était perdu : rotation faite (ancien hash dans
  `/var/lib/mypass/token.hash.bak-2026-10-02`). Procédure : renommer `token.hash`, `systemctl
  restart mypass-server`, lire le nouveau jeton dans `journalctl -u mypass-server` (en root).
- Le service worker PWA était aussi inclus dans la build desktop : une ancienne install le gardait
  dans `%LOCALAPPDATA%\com.mypass.app\EBWebView\Default\Service Worker` et servait l'interface de
  juillet (avec le mock). Le premier correctif (`sw.js` auto-destructeur) n'a pas suffi : le 2ᵉ PC
  a encore affiché la maquette, et la panne a été reproduite sur un profil WebView2 empoisonné
  (3 lancements, ancien index toujours servi). Correctif final (`17d46d6`) : le desktop est servi
  depuis `https://tauri.localhost` (`useHttpsScheme`), hors de portée de l'ancien SW lié à
  `http://`, et n'enregistre plus aucun SW. Coût : le délai de verrouillage auto et la langue
  (localStorage) reviennent une fois à leur valeur par défaut.
- Le `.msi` servi sous `/download/` est à jour ; celui de juillet est dans
  `/root/MyPass_0.1.0_x64_en-US.msi.bak-2026-10-02`.
- Mineurs reportés : échec d'écriture de `sync.json` après récupération → écran bloqué jusqu'au
  redémarrage ; `SERVER_ERROR` sans code HTTP ; ancienne erreur réaffichée après « Retour ».

## 2 — Retrait du factice et du code mort

**Objectif :** plus aucune donnée inventée ni bouton inerte dans l'app réelle.

**Périmètre :**
- **Passkeys** : `src/views/PasskeysView.tsx` (`MOCK_PASSKEYS` affichées même en desktop),
  `src/components/passkeys/`, route dans `src/App.tsx`, entrées de `AppSidebar.tsx`, raccourci
  dans `src/hooks/useKeyboardShortcuts.ts`, mocks dans `src/lib/tauri.ts` et `src/lib/web.ts`,
  types (`src/types/entry.ts`, `src/types/index.ts`), `src-tauri/src/commands/passkeys.rs` +
  `commands/mod.rs` + 5 lignes de `generate_handler!`, clés i18n `passkeys.*`, ligne du README.
  Vérifier les références dans `src-tauri/src/native_messaging.rs`.
  **Conserver** la lecture des champs `KPEX_PASSKEY_*` existants dans `crates/mypass-core`
  (`has_passkey`, filtrage des champs personnalisés) : on ne détruit aucune donnée d'un coffre.
- **Code mort** : `checkHibp` (`src/lib/crypto.ts:58`, commande inexistante côté Rust),
  `src-tauri/src/security/zxcvbn.rs` (bouchons), `security/hibp.rs` si inutilisé (le front
  appelle HIBP directement), commande `greet` (`src-tauri/src/lib.rs:15`).
- **Boutons sans action** : « Mettre à jour » de `src/views/SecurityView.tsx:88` (ouvrir
  l'entrée concernée ou retirer le bouton). Les items « Renommer » / « Supprimer » de
  `GroupTree.tsx` sont traités au sous-projet 4.
- **Firefox** : section Firefox de `register-nhm.ps1` et entrée `allowed_extensions` de
  `src-tauri/src/commands/browser.rs:87`.
- **Dossier `extension/` obsolète** : seul un manifest divergent et des icônes. La vraie
  extension est `keepassxc-browser/keepassxc-browser/` (fork modifié : `content/identity-fields.js`,
  `background/keepass.js:595`). ⚠️ `extension/dev-key.pem` (ignoré par git) fixe l'ID de
  l'extension : **le déplacer hors du dépôt avant toute suppression**.

**Fini quand :** plus de `MOCK_PASSKEYS` ni de route `/passkeys` ; lint, clippy et tests verts ;
un coffre contenant des champs `KPEX_PASSKEY_*` s'ouvre et se resauvegarde sans perte.

## 3 — TOTP

**Objectif :** voir et copier les codes 2FA dans MyPass, et laisser l'extension les remplir.

**Constat :** le calcul existe (`src-tauri/src/commands/totp.rs`, `generate_totp_code` du wasm) ;
`has_totp` est exposé (`crates/mypass-core/src/ops/entries.rs:174`), mais le secret (`otp` /
`TOTP Seed`) est filtré des champs renvoyés au front (ligne 183) et **aucune UI n'affiche de code**.
L'extension reçoit « TOTP not yet implemented » (`src-tauri/src/native_messaging.rs:692-711`).

**Périmètre :** affichage du code + compte à rebours + copie dans `EntryDetail` ; saisie et
modification du secret dans `EntryForm` ; `get-totp` pour l'extension. Format de stockage
compatible KeePassXC (champ `otp` = URI `otpauth://`), à confirmer dans la spec. À trancher :
calculer le code côté Rust à partir de l'uuid (le secret ne quitte pas le backend) ou exposer
le secret au front.

**Fini quand :** pour un secret de test, MyPass affiche le même code que Google Authenticator
ou KeePassXC, sur desktop et PWA ; l'extension Chrome remplit un champ 2FA.

## 4 — Groupes

**Objectif :** ranger les entrées en dossiers, partout.

**Constat :** commandes Rust enregistrées (`commands/groups.rs`) et exports wasm complets
(`get_groups`, `create_group`, `update_group`, `delete_group`, `move_entry`), mais :
`AppSidebar.tsx:116` ne passe jamais `groups` à `GroupTree` et `onSelectGroup` ne fait rien ;
« Renommer » / « Supprimer » sans action (`GroupTree.tsx:133-134`) ; aucun cas groupe dans
`src/lib/web.ts`. `useEntries(groupUuid)` sait déjà filtrer.
⚠️ La fusion ne supprime jamais un groupe (`crates/mypass-core/src/merge.rs:75`, `ponytail:`) :
supprimer un groupe sur un PC ne se propagerait pas. La spec doit trancher (pierres tombales
de groupes, ou limite assumée et documentée).

**Périmètre :** arbre affiché, filtrage de la liste, créer / renommer / supprimer, déplacer une
entrée (par menu ; glisser-déposer seulement si demandé), parité web.

**Fini quand :** un groupe créé ou renommé sur un PC apparaît sur l'autre et sur le téléphone ;
le comportement de la suppression correspond à ce que la spec a décidé.

## 5 — Import / export

**Objectif :** sortir et rentrer ses données de façon fiable, sur desktop comme sur PWA.

**Constat :** `ExportDialog.tsx` ne propose que CSV et JSON, et écrit à un chemin relatif sans
dialogue (ligne 36). En mode web, `import_entries`, `export_csv` et `export_json` tombent dans
« Commande indisponible en mode web » (`src/lib/web.ts:351`) ; le wasm n'expose aucune fonction
d'import ou d'export. L'import passe déjà par un `<input type="file">` et le dédoublonnage TS
(`src/lib/dedup.ts`), donc il est réutilisable côté web. **Perte de données à l'import**
(relevée au sous-projet 2) : la colonne TOTP du CSV (`CsvColumnMapping.totp`) est ignorée, et
`import_entries` n'écrit ni `tags`, ni `customFields`, ni `totp` des entrées résolues.

**Périmètre :** boîte « Enregistrer sous » sur desktop (`plugin-dialog`) ; export en PWA
(téléchargement de fichier) ; import en PWA. À trancher : une seule génération CSV/JSON
partagée (TS ou core Rust) plutôt que deux implémentations. XML / HTML (promis par le README)
retirés du README, sauf besoin exprimé.

**Fini quand :** export puis réimport dans un coffre vide sans perte de champ, sur desktop et PWA.

## 6 — Durcissement

**Objectif :** qu'un gestionnaire de mots de passe ait les protections qu'il annonce.

**Périmètre :**
- CSP réelle dans `src-tauri/tauri.conf.json` (aujourd'hui `"csp": null`). La sync passe par
  Rust (`reqwest`) ; côté front, seul HIBP (`api.pwnedpasswords.com`) appelle l'extérieur.
- Mot de passe maître : `DbState.password_hash` contient en réalité le mot de passe en clair
  (`src-tauri/src/commands/database.rs:117`). Le renommer, le stocker en `Zeroizing`
  (`zeroize` est déjà une dépendance) et l'effacer au verrouillage (`lock_database`, ligne 192),
  ainsi que `keyfile_data`.
- Retirer les `#![allow(dead_code)]` de `src-tauri/src/lib.rs` et corriger ce que clippy remonte.
- Serveur : coffres en `644` → `UMask=0077` dans l'unité systemd (ou à l'écriture) ; ménage du
  conteneur (`/opt/mypass-src/server` obsolète, fichier parasite `$null` dans
  `/opt/mypass-src/server-build`).
- Accepté tel quel (déjà marqué `ponytail:` dans `sync.rs:15`) : jeton de sync en clair dans
  `%APPDATA%`. À réévaluer seulement si un PC est partagé.

**Fini quand :** clippy propre sans `allow` global ; l'app desktop fonctionne avec la CSP
(déverrouillage, sync, HIBP, copie, extension) ; les coffres serveur sont en `600` et le service
répond.

## 7 — Mise à jour automatique

**Objectif :** déployer une version une fois et voir les deux PC se mettre à jour seuls.

**Constat :** `/opt/mypass-downloads/latest.json` et le `.msi` sont servis sous `/download/`,
mais aucun plugin updater n'est configuré dans `src-tauri/tauri.conf.json`.

**Périmètre :** `tauri-plugin-updater`, paire de clés de signature (clé privée **hors dépôt**),
artefacts de mise à jour générés au build, endpoint
`https://mypass-luca.tail7687c9.ts.net/download/latest.json`, vérification au démarrage, et
procédure de release écrite (bump de version → build → copie sur le conteneur).
À trancher : signature Authenticode (ex. Azure Trusted Signing) seulement si un PC cible doit
garder Smart App Control actif ; la signature updater Tauri, elle, est gratuite et indépendante.

**Fini quand :** après le déploiement d'une 0.1.1, le portable propose la mise à jour et
redémarre en 0.1.1.

## 8 — Docs

**Périmètre :**
- `CLAUDE.md` : vraie extension = `keepassxc-browser/keepassxc-browser/` ; sauvegardes vzdump
  par `Pve` ; passkeys retirées ; `/opt/mypass-src/server` supprimé ; tout ce que les
  sous-projets précédents ont changé.
- `README.md` : fonctions réelles, structure (`crates/mypass-core`, `server/`), nombre de
  commandes, mention `LICENSE` (ajouter le fichier ou retirer la mention).
- Passe Prettier (98 fichiers non conformes) dans un commit dédié, sans autre changement.

**Fini quand :** un nouveau lecteur peut installer, builder et déployer en suivant uniquement
`README.md` et `CLAUDE.md`.

---

## Suivi

| # | Sous-projet | Spec | Plan | Fait |
|---|---|---|---|---|
| 0 | Poste de dev | — | — | [x] 2026-10-02 |
| 1 | Desktop sur 2 PC | [x] | [x] | [~] reste le 2ᵉ PC |
| 2 | Retrait du factice | [x] | [x] | [ ] |
| 3 | TOTP | [ ] | [ ] | [ ] |
| 4 | Groupes | [ ] | [ ] | [ ] |
| 5 | Import / export | [ ] | [ ] | [ ] |
| 6 | Durcissement | [ ] | [ ] | [ ] |
| 7 | Mise à jour auto | [ ] | [ ] | [ ] |
| 8 | Docs | — | — | [ ] |

Le sous-projet 0 est de l'installation d'outils : il n'a ni spec ni plan, seulement les
vérifications de « Fini quand ». Le 8 est de la rédaction pure.
