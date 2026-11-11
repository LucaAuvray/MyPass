# Clés d'accès (passkeys) via l'extension — conception

Date : 2026-11-11. Statut : à valider par Luca.

## Pourquoi

Binance, PayPal et d'autres sites proposent de « sécuriser le compte avec une clé d'accès ». Dans
Chrome sur les deux PC, la clé part aujourd'hui dans Windows Hello : MyPass n'apparaît pas dans la
fenêtre de choix, la clé n'est pas dans le coffre et n'existe que sur le PC où elle a été créée.
Luca veut que ces clés vivent dans MyPass, comme ses mots de passe, donc synchronisées sur les deux
PC.

**Revirement assumé :** la feuille de route du 2026-10-02 avait retiré les passkeys (« pas
d'implémentation WebAuthn »). Cette conception la remplace sur ce point ; la ligne de la feuille de
route sera annotée.

## Objectifs

- Dans Chrome (et Edge) sur les PC, quand un site demande de créer une clé d'accès, MyPass la crée
  et la range dans le coffre après confirmation dans l'app.
- Quand le site demande de se connecter avec une clé d'accès, MyPass signe après confirmation.
- La clé est synchronisée par le mécanisme existant (le coffre est sauvé puis poussé).
- Si MyPass ne peut pas ou ne doit pas répondre (verrouillé, aucune clé pour ce site, annulation),
  la fenêtre Windows habituelle prend le relais : les clés déjà dans Windows Hello continuent de
  marcher.

## Hors périmètre

- Apparaître dans la fenêtre Windows « Choisir où enregistrer » (plugin COM + paquet MSIX, approche
  B ci-dessous).
- Utiliser ces clés sur l'iPhone : iOS ne laisse que les applis natives fournir des clés d'accès,
  pas une PWA.
- Firefox (déjà hors périmètre de l'intégration navigateur).
- Algorithmes autres que ES256 (RS256, EdDSA) ; attestation autre que `none` ; « related origins »
  (origines liées) ; saisie automatique conditionnelle (`mediation: "conditional"`, l'extension la
  renvoie déjà au navigateur).
- Écran de gestion des clés : une clé se supprime en supprimant son entrée (voir Rangement).
- Importer des clés depuis Windows Hello (Windows ne les exporte pas).

## Ce qui serait un échec même si « ça marche »

- Un site obtient une signature ou crée une clé pour un autre domaine (`rp.id` qui ne correspond
  pas à l'origine réelle de la page).
- Les clés existantes dans Windows Hello cessent de fonctionner.
- La clé privée apparaît dans un log, dans l'UI ou dans `customFields`.
- Le coffre reste bloqué (mutex `DbState` tenu) pendant que la fenêtre de confirmation attend.

## Approches considérées

| | A. Par l'extension (retenue) | B. Plugin Windows |
|---|---|---|
| Principe | L'extension MyPass Browser (KeePassXC-Browser 1.10.3) intercepte déjà `navigator.credentials` et envoie `passkeys-register` / `passkeys-get` à l'hôte natif ; il manque ces deux actions dans l'app. | Serveur COM `IPluginAuthenticator`, enregistré par `WebAuthNPluginAddAuthenticator` (Windows 11, nov. 2025), app empaquetée en MSIX. |
| Couvre | Chrome et Edge avec l'extension. | Tous les navigateurs et les applis Windows ; MyPass dans la fenêtre Windows. |
| Coût | Deux actions de l'hôte, une fenêtre de confirmation, un badge. Pas de nouvelle crate réelle (`p256` est déjà dans `Cargo.lock` via `ssh-key`). | Refaire la release (`.msi` + updater Tauri → MSIX signé par un certificat reconnu sur chaque PC), Windows Hello imposé, API récente. |

Luca a choisi **A** (2026-11-11) : même résultat pour Binance et PayPal dans Chrome, sans toucher à
la release. B reste possible plus tard.

## Conception

### 1. Protocole (référence : KeePassXC 2.7.10)

Reproduire le comportement de KeePassXC, que l'extension attend tel quel. Sources à lire
(`keepassxreboot/keepassxc`, tag `2.7.10`, `src/browser/`) : `BrowserAction.cpp`
(`handlePasskeysGet/Register`), `BrowserService.cpp` (`showPasskeysRegisterPrompt`,
`showPasskeysAuthenticationPrompt`, `addPasskeyToEntry`, `getPasskeyAllowedEntries`),
`BrowserPasskeys.cpp` (`buildRegisterPublicKeyCredential`, `buildGetPublicKeyCredential`,
données d'authentificateur), `BrowserPasskeysClient.cpp` (validation des options, `clientDataJSON`),
`PasskeyUtils.cpp`. Côté extension : `content/passkeys.js` (codes d'erreur → exceptions),
`content/passkeys-inject.js`, `content/passkeys-utils.js`, `background/keepass.js`
(`passkeysRegister/Get`).

- Deux nouvelles branches dans `handle_request` de `src-tauri/src/native_messaging.rs` :
  `"passkeys-register"` et `"passkeys-get"`. Message déchiffré : `publicKey` (options, tampons en
  base64url), `origin` (fourni par le script de contenu, donc fiable), `relatedOrigins` et
  `groupName` (ignorés).
- Réponse chiffrée comme les autres actions, avec `response` = l'objet `PublicKeyCredential` JSON
  de KeePassXC, ou `response = { "errorCode": n }` en cas d'erreur (codes 20–33 de
  `content/passkeys.js`).
- **Repli silencieux** (choix de Claude, écart avec KeePassXC) : « aucune clé pour ce site » et
  « annulé » répondent **sans champ `response`**. Avec un `errorCode`, `passkeys-inject.js:50-55`
  affiche une notification d'erreur avant de basculer sur Windows : chaque connexion à un site dont
  la clé est dans Windows Hello commencerait par une erreur MyPass. Sans `response`, l'extension
  bascule directement (`passkeys-utils.js` → `fallback`).
- Champs de la réponse : comme KeePassXC (`id`, `type: "public-key"`, `authenticatorAttachment:
  "platform"`, `response.{attestationObject | authenticatorData, clientDataJSON, signature,
  userHandle, publicKeyAlgorithm, clientExtensionResults: {}}`), **plus** `response.publicKey`
  (SPKI DER en base64url) à l'enregistrement, pour les sites qui appellent `getPublicKey()`.

### 2. Validation (frontière de confiance)

La page contrôle les options (l'extension les intercepte avant le navigateur), donc l'hôte vérifie
lui-même :

- `origin` en `https://` ; sinon `INVALID_URL_PROVIDED`.
- `rpId` = `rp.id` (enregistrement) ou `rpId` (connexion), par défaut l'hôte de l'origine. Il doit
  être égal à l'hôte de l'origine ou en être un suffixe après un point (`accounts.binance.com` →
  `binance.com` accepté ; `binance.com.evil.com` → `binance.com` refusé) et contenir au moins un
  point (refuse `com`). Sinon `DOMAIN_RPID_MISMATCH` (→ `SecurityError`).
- `challenge` non vide (`INVALID_CHALLENGE`) ; `user.id` de 1 à 64 octets (`INVALID_USER_ID`).
- `pubKeyCredParams` doit contenir `-7` (ES256), sinon `NO_SUPPORTED_ALGORITHMS`. La page reçoit
  alors une erreur, et l'extension bascule sur Windows si l'option de repli est active.
- Une demande d'attestation `direct`/`indirect` reçoit `none` ; `enterprise` est refusée
  (`ATTESTATION_NOT_SUPPORTED`), comme KeePassXC.

### 3. Cryptographie

- ES256 seulement : `p256` 0.13 en dépendance directe de `src-tauri` (features `ecdsa`, `pkcs8`,
  `pem`), version déjà présente dans `Cargo.lock`. `sha2`, `rand` et `base64` y sont déjà.
- Identifiant de clé : 32 octets aléatoires. Clé privée rangée en PKCS#8 PEM
  (`-----BEGIN PRIVATE KEY-----`, format de KeePassXC).
- `clientDataJSON` = `{"type":"webauthn.create"|"webauthn.get","challenge":…,"origin":…,
  "crossOrigin":…}` (comme `BrowserPasskeysClient.cpp`).
- `authenticatorData` = SHA-256(`rpId`) | drapeaux | compteur 0 sur 4 octets | (enregistrement :
  AAGUID de 16 octets nuls | longueur sur 2 octets | identifiant | clé COSE). Drapeaux : UP + UV +
  AT à l'enregistrement, UP + UV à la connexion ; BE et BS à 0 et pas d'extensions, comme KeePassXC.
  UV est posé parce que l'utilisateur a confirmé dans un coffre déverrouillé par le mot de passe
  maître (même hypothèse que KeePassXC).
- Objet d'attestation CBOR `{"fmt":"none","attStmt":{},"authData":…}` et clé COSE
  `{1:2, 3:-7, -1:1, -2:x, -3:y}`, encodés à la main en ordre canonique (quelques lignes, pas de
  crate CBOR).
- Signature : ECDSA P-256 au format DER sur `authenticatorData || SHA-256(clientDataJSON)`.
- Encodage base64url sans remplissage partout ; vérifier contre `kpxcBase64ToArrayBuffer`.

### 4. Rangement dans le coffre

Champs de KeePassXC, que le noyau conserve déjà (`passkey_fields_survive_update_and_save`) et cache
de `customFields` : `KPEX_PASSKEY_USERNAME`, `KPEX_PASSKEY_RELYING_PARTY`, et en champs protégés
`KPEX_PASSKEY_CREDENTIAL_ID`, `KPEX_PASSKEY_PRIVATE_KEY_PEM`, `KPEX_PASSKEY_USER_HANDLE`.

- **Une clé = une entrée dédiée**, comme KeePassXC par défaut. Titre `rp.name` (ou le `rpId`),
  identifiant `user.name`, URL = origine, sans mot de passe, dans le groupe racine (comme les
  entrées créées par `set-login`). L'entrée mot de passe du site n'est pas touchée, et supprimer la
  clé revient à supprimer son entrée.
- Si une entrée a déjà le même `RELYING_PARTY` et le même `USER_HANDLE`, sa clé est remplacée (la
  fenêtre de confirmation le dit) au lieu de créer un doublon.
- `excludeCredentials` contient l'identifiant d'une clé du coffre pour ce `rpId` →
  `CREDENTIAL_IS_EXCLUDED`.
- Connexion : entrées dont `RELYING_PARTY == rpId`, dans tout le coffre. Si `allowCredentials`
  n'est pas vide, filtrer par identifiant ; sinon garder celles qui ont un `USER_HANDLE`. Aucune
  entrée → repli silencieux sur Windows (§1).
- Après un enregistrement : `db.save()` puis `notify_vault_changed()`, comme `set-login`.
- `get-logins` ignore les entrées qui ont une clé d'accès et pas de mot de passe, sinon l'extension
  proposerait de remplir l'identifiant avec un mot de passe vide (choix de Claude).

### 5. Confirmation dans l'app

- Chaque enregistrement et chaque connexion passent par une confirmation, sur le modèle de l'agent
  SSH : `src-tauri/src/ssh/agent.rs` (`confirm`, événements `ssh-sign-request[-closed]`) et
  `src/components/ssh/SshSignRequestDialog.tsx`. La fenêtre principale passe au premier plan
  (`focus_main_window`).
- L'attente dure le `timeout` des options, que l'extension borne déjà entre 15 et 120 s. **Le mutex
  `DbState` n'est jamais tenu pendant l'attente** : lire et préparer, relâcher, attendre, reprendre
  le verrou pour écrire. L'attente ne doit pas bloquer les autres messages de la même connexion
  (`get-logins` d'un autre onglet) : traiter la demande de clé d'accès sur son propre fil.
- Annulation, délai dépassé ou fenêtre fermée → repli silencieux sur la fenêtre Windows (§1).
- **Coffre verrouillé** : l'extension n'envoie même pas la demande. `passkeysRegister/Get`
  (`background/keepass.js:695-764`) appellent d'abord `testAssociation(tab, [false])`, qui échoue
  sur coffre fermé et bascule en silence sur Windows. Seule retouche de l'extension (choix de
  Claude) : passer `[false, true]` (`triggerUnlock`) dans ces deux appels. MyPass passe alors au
  premier plan pour qu'on le déverrouille. La fenêtre Windows s'ouvre quand même : on l'annule,
  on déverrouille, puis on recommence. Sans ça, une clé créée pendant le verrouillage automatique
  partirait en douce dans Windows Hello.
- La charge de l'événement ne contient que le `rpId`, les noms de compte et les titres d'entrée,
  jamais de clé.
- Textes (i18n EN + FR) :
  - Enregistrement : « Créer une clé d'accès pour **binance.com** (compte luca@…) ? » + « Elle
    remplacera la clé existante » si besoin ; boutons Annuler / Créer.
  - Connexion : « Se connecter à **binance.com** avec : » + le compte, ou une liste à choix unique
    s'il y en a plusieurs ; boutons Annuler / Se connecter.

### 6. Affichage des entrées

Un badge « Clé d'accès » dans le détail de l'entrée (et sur la carte si ça tient sans bruit), à
partir de `has_passkey`, déjà présent dans `EntryInfo`. Aucun autre écran.

### 7. Extension et documentation

- Extension : seulement la retouche `triggerUnlock` du §5. Sur chaque PC, Luca recharge
  l'extension (`chrome://extensions`) et active « Clés d'accès » dans ses options (désactivée par
  défaut), en laissant le repli actif.
- **Effet de bord accepté** (choix de Claude) : avec l'option active, `content/passkeys.js:260-273`
  force `PublicKeyCredential.isConditionalMediationAvailable()` à `false` sur toutes les pages. Les
  sites ne proposent donc plus la clé d'accès dans la liste de saisie automatique du champ
  identifiant, y compris pour les clés Windows Hello : il faut cliquer sur leur bouton « Se
  connecter avec une clé d'accès », qui passe par MyPass puis bascule sur Windows au besoin.
  C'est le choix de KeePassXC : un navigateur ne sait pas proposer un authentificateur logiciel
  dans cette liste.
- `CLAUDE.md` (section Intégration navigateur) et la feuille de route (ligne Passkeys) sont mis à
  jour.
- Livraison par une release desktop normale (version suivante). Ni le serveur ni la PWA n'en ont
  besoin pour fonctionner ; le badge apparaîtra dans la PWA au prochain déploiement.

## Vérification

- Tests Rust (`src-tauri`) :
  - table de validation origine/`rpId` (cas ci-dessus, `http://` refusé) ;
  - aller-retour enregistrement → connexion : structure de l'objet d'attestation (hash du `rpId`,
    drapeaux, identifiant, x/y de la clé COSE égaux à la clé publique), signature vérifiée avec la
    clé publique `p256` ;
  - rangement : champs présents et protégés, remplacement sur même `USER_HANDLE`, recherche par
    `allowCredentials` et par `USER_HANDLE`, `excludeCredentials`.
- À la main : webauthn.io (créer puis se connecter), puis Binance ou PayPal sur le compte réel de
  Luca. Ensuite, connexion depuis l'autre PC après synchronisation. Annuler → la fenêtre Windows
  apparaît, sans notification d'erreur ; coffre verrouillé → MyPass passe au premier plan et la
  fenêtre Windows apparaît ; un site avec une clé Windows Hello existante marche toujours par son
  bouton, sans notification d'erreur.
- Non-régression habituelle : les trois `cargo test`, `cargo clippy`, `npm run lint`,
  `npm run format:check`, `npm run build`.

## Laissé au constructeur

Découpage en modules (par exemple `src-tauri/src/passkeys.rs` pour la crypto et le CBOR), forme
exacte de la commande Tauri de réponse à la confirmation, mise en page de la fenêtre, rendre la main
au navigateur après la décision (re-minimiser la fenêtre si on l'a sortie pour ça), organisation
des tests.
