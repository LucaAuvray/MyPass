# TOTP — Design (sous-projet 3)

> Sous-projet 3 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-03.

## Intention

Luca voit et copie ses codes 2FA dans MyPass, sur desktop comme sur PWA, et l'extension Chrome
les remplit. Il ajoute un secret en **collant la clé** donnée par le site (« Impossible de
scanner ? Saisir la clé ») ou un lien `otpauth://`.

**Succès :** pour une clé de test, MyPass affiche le même code qu'un calcul de référence
indépendant (même code que Google Authenticator), sur desktop et PWA ; l'extension Chrome remplit
un champ 2FA.

## Constat de départ

- Le calcul existe (`crates/mypass-core/src/totp.rs`, exposé par les commandes
  `generate_totp_code(secret, algorithm, digits, period)` et `generate_totp_secret`, desktop et
  wasm) mais **aucune UI ne l'appelle**.
- `generate_totp_code` passe par `TOTP::new`, qui refuse les clés de moins de 128 bits : une clé
  de 80 bits (16 caractères base32, ex. `JBSWY3DPEHPK3PXP`, courante chez Google et beaucoup de
  sites) donnerait une erreur.
- Depuis le sous-projet 5, l'import range le secret au format KeePassXC : champ `otp` protégé
  contenant un lien `otpauth://` (`normalize_totp` dans `ops/transfer.rs`). `has_totp` est exposé,
  mais `otp` / `TOTP Seed` sont filtrés des champs renvoyés au front (`ops/entries.rs:189`).
- L'export lit `otp` sinon `TOTP Seed` brut : un `TOTP Settings` (`60;8`) est perdu (mineur
  reporté du sous-projet 5).
- Extension : `get-totp` répond « TOTP not yet implemented » (`src-tauri/src/native_messaging.rs:687`),
  et `get-logins` n'envoie pas de champ `totp`. Or l'extension KeePassXC-Browser ne demande un code
  que si l'identifiant a un `totp` non vide (`content/fill.js:147`).
- `src/types/entry.ts` (`TotpConfig`…) n'est importé par aucun composant ; le type réellement
  utilisé est `Entry` de `src/stores/entriesStore.ts`.

## Décisions

| Sujet | Décision |
|---|---|
| Où se calcule le code | En Rust, à partir de l'uuid de l'entrée. Un seul lecteur de lien (`totp-rs`) pour l'UI, la PWA et l'extension. |
| Saisie | Un champ texte « Clé 2FA » : clé base32 nue ou lien `otpauth://`. Pas de QR, pas de caméra, pas de migration Google Authenticator. |
| Stockage | Format KeePassXC : champ `otp` protégé = lien `otpauth://`. L'ancien format KeePass2 (`TOTP Seed` + `TOTP Settings`) est lu, et converti en `otp` quand la clé 2FA est modifiée. |
| Clé invalide | Refusée à l'enregistrement (`TOTP_INVALID`), jamais stockée. |
| Affichage | Dans la fiche d'une entrée identifiant seulement (pas dans la liste). |
| Commandes mortes | `generate_totp_code(secret…)`, `generate_totp_secret` et le base32 fait main sont supprimés : aucune UI ne s'en sert, et le secret vient toujours du site. |

## Design

### 1. Noyau : `crates/mypass-core/src/totp.rs`

- `TotpCode { code: String, period: u64, seconds_remaining: u64 }`, sérialisé en camelCase :
  `{ code, period, secondsRemaining }`.
- `entry_otp_uri(entry: &Entry) -> Option<String>` — le lien de l'entrée :
  - champ `otp` non vide → sa valeur telle quelle ;
  - sinon `TOTP Seed` non vide → lien reconstruit par `normalize_totp` (clé nettoyée), avec
    `period` et `digits` tirés de `TOTP Settings` (`"<période>;<chiffres>"`, ex. `60;8`) quand
    ce champ existe ; un `TOTP Settings` illisible ou Steam (`30;S`) est ignoré (valeurs par
    défaut) ;
  - sinon `None`.
- `code_at(uri: &str, now: u64) -> Result<TotpCode, String>` — lit le lien avec
  `TOTP::from_url_unchecked` (accepte les clés de 80 bits) puis calcule le code à l'instant `now`.
  Erreur `TOTP_INVALID` si le lien ne se lit pas, si la clé est vide, si `digits` n'est pas 6, 7
  ou 8, ou si `period` vaut 0 (jamais de panique sur une division par zéro ou un débordement).
- `code_for_entry(entry: &Entry, now: u64) -> Result<TotpCode, String>` — `entry_otp_uri` puis
  `code_at` ; pas de lien → `TOTP_NONE`.
- `normalize_totp` (aujourd'hui dans `ops/transfer.rs`) devient `totp::normalize`, partagé.
  Avec un lien, il remet en forme le paramètre `secret` (majuscules, sans espaces ni `=`) pour qu'un
  lien collé avec une clé en minuscules se lise ; le reste du lien est conservé.
- Supprimés : `generate_totp_code`, `generate_totp_secret`, `TotpSetup`, `base32_encode`,
  `base32_decode`.

### 2. Noyau : `crates/mypass-core/src/ops/entries.rs`

- `EntryInfo` gagne `totp: String` = `entry_otp_uri(entry)` ou `""` (pour préremplir le
  formulaire). `has_totp` reste (utilisé par le dédoublonnage).
- `NewEntry` et `UpdateEntry` gagnent `totp: Option<String>` :
  - `None`, ou le lien actuel renvoyé tel quel par le formulaire → rien ne change (une 2FA illisible, Steam ou KeeOtp, ne bloque pas les autres modifications et n'est pas réécrite) ;
  - `Some("")` (après trim) → suppression de `otp`, `TOTP Seed` et `TOTP Settings` ;
  - sinon → `totp::normalize(valeur, titre)`, vérifié par `code_at` (sinon erreur `TOTP_INVALID`
    et l'entrée n'est pas modifiée), écrit dans `otp` protégé ; `TOTP Seed` et `TOTP Settings`
    sont retirés.
- `get_totp_code(kf, uuid, now) -> Result<TotpCode, String>` — `find_entry` puis
  `code_for_entry`.
- L'export (`ops/transfer.rs`) lit le TOTP via `entry_otp_uri` (corrige la perte de
  `TOTP Settings`).

### 3. Commandes et parité

| Commande | Desktop (`src-tauri`) | Web (`web.ts` + wasm) | Mock |
|---|---|---|---|
| `get_totp_code(uuid) → TotpCode` | verrou → `ops::entries::get_totp_code(kf, uuid, unix_now())` | binding wasm `get_totp_code(uuid)` | `{ code: "123456", period: 30, secondsRemaining: 30 - (s % 30) }` |
| `create_entry` / `update_entry` | `totp` transmis au noyau | idem (forme JSON identique) | `totp` gardé dans l'entrée en mémoire |

- `generate_totp_code` et `generate_totp_secret` disparaissent de `generate_handler!`, du wasm, de
  `web.ts`, de `mock.ts` et du smoke wasm (remplacés par un contrôle de `get_totp_code`).
- **Extension** (même comportement que KeePassXC) :
  - `get-logins` : chaque identifiant dont l'entrée a une 2FA lisible reçoit
    `"totp": "<code actuel>"` ; sinon pas de clé `totp`.
  - `get-totp` (`uuid`) : `{ "totp": "<code>", "success": "true" }` ; entrée introuvable, sans
    2FA ou clé illisible → `"totp": ""` (l'extension affiche alors « aucun TOTP trouvé »).

### 4. Interface

- **Fiche** (`EntryDetail.tsx`, entrée identifiant dont `totp` est non vide) : ligne « Code 2FA »
  sous le mot de passe :
  - code en chasse fixe, groupé par 3 (6 chiffres : `123 456`) ou par 4 (8 chiffres :
    `1234 5678`) ; le bouton copier copie le code **sans espace** ;
  - barre de temps restant (`secondsRemaining / period`), rouge sous 5 secondes ;
  - hook `useTotpCode(uuid)` : TanStack Query, `refetchInterval: 1000`, actif seulement quand la
    ligne est affichée ;
  - erreur `TOTP_INVALID` → « Clé 2FA invalide » à la place du code.
- **Formulaire** (`EntryForm.tsx`) : champ facultatif « Clé 2FA », chasse fixe, placeholder
  `JBSW Y3DP… ou otpauth://…`, prérempli avec `totp` en modification ; envoyé à la création s'il
  est rempli, toujours envoyé en modification (vide = retirer la 2FA). Erreur `TOTP_INVALID` du
  backend → message sous le champ, formulaire laissé ouvert.
- Type `Entry` (`entriesStore.ts`) : `totp: string`. `src/types/entry.ts` / `TotpConfig`, morts,
  sont supprimés (et leur ré-export dans `src/types/index.ts`).
- i18n EN + FR : libellés du champ et de la ligne, placeholder, `TOTP_INVALID`.

## Tests et vérification

**Tests Rust automatisés (`mypass-core`) :**
- Vecteurs RFC 6238 (annexe B, clés ASCII `12345678901234567890…` en base32, 8 chiffres) en SHA1,
  SHA256 et SHA512 aux instants 59, 1111111109 et 2000000000.
- Clé de 80 bits `JBSWY3DPEHPK3PXP` acceptée ; `digits=8`, `period=60`, `algorithm=SHA256` du lien
  respectés ; `secondsRemaining` = `period - now % period`.
- Lien collé avec clé en minuscules et espaces → lu ; `digits=12`, `period=0`, clé vide, lien
  `otpauth://hotp/…` → `TOTP_INVALID`, sans panique.
- Ancien format : `TOTP Seed` + `TOTP Settings = 60;8` → lien avec `period=60&digits=8` ; `otp`
  prioritaire quand les deux existent.
- `update` : clé valide → `otp` protégé, anciens champs retirés ; `""` → les trois champs retirés ;
  clé invalide → `TOTP_INVALID` et entrée inchangée ; `None` → `otp` intact. `create` avec `totp`.
- Export d'une entrée `TOTP Seed` + `TOTP Settings` → lien complet.

**Tests desktop (`src-tauri`) :** `get-logins` contient `totp` pour une entrée avec 2FA et pas pour
les autres ; réponse de `get-totp` (code de 6 chiffres, puis `""` pour une entrée sans 2FA).

**Non-régression :** `cargo test` (`crates/mypass-core`, `src-tauri`, `server`), `cargo clippy` sans
nouveau warning, `npm run lint`, `npm run build`, `npm run build:web`, `npm run smoke:wasm`.

**E2E manuel, jamais sur la prod :**
- Référence : petit script Node (`crypto.createHmac`) qui calcule le code de `JBSWY3DPEHPK3PXP` à
  l'instant présent.
- Desktop isolé (`npm run tauri dev`, `APPDATA` temporaire, identifiant d'app distinct) : créer une
  entrée avec cette clé → le code affiché égale la référence ; il change au passage de la période ;
  copier colle le code sans espace ; une clé invalide est refusée avec le message.
- PWA (`npm run build:web` + `mypass-server` local, comme au sous-projet 5) : même vérification.
- Extension : par Luca après installation du nouveau `.msi` (l'extension doit être associée à
  l'app installée) — un site avec 2FA, le champ du code est rempli.

## Risques

- **Horloge de l'appareil décalée** : le code est faux comme dans toute appli TOTP ; hors périmètre.
- **Formats non lus** : Steam (`otpauth://steam`, `TOTP Settings = 30;S`), KeeOtp
  (`otp = key=…&step=…`), champs natifs KeePass 2.47+ (`TimeOtp-*`) → « Clé 2FA invalide » ou pas
  de 2FA. Ajoutable plus tard si un vrai coffre en contient.
- **Secret visible dans le formulaire** : la clé arrive au front pour la modification, comme le mot
  de passe aujourd'hui ; pas de nouvelle exposition.
- **Conversion de l'ancien format** : changer la clé 2FA d'une entrée `TOTP Seed` la passe en `otp` ; un
  vieux KeePass2 sans support `otp` ne verrait plus la 2FA. Acceptable : le coffre est lu par
  MyPass et KeePassXC, qui lisent `otp`.
