# Retrait du factice et du code mort — Design (sous-projet 2)

> Sous-projet 2 de `docs/superpowers/specs/2026-10-02-roadmap-design.md`.
> Design validé avec Luca le 2026-10-02.

## Intention

L'app réelle (`.msi` et PWA) n'affiche plus aucune donnée inventée ni bouton qui ne fait rien, et
le code qui ne sert pas disparaît avant d'ajouter TOTP, groupes et import/export (sous-projets 3-5).

**Succès :** plus de `MOCK_PASSKEYS` ni de route `/passkeys` ; aucune build de production ne contient
le backend mock ; le bouton « Mettre à jour » de l'écran Sécurité ouvre l'entrée ; lint, clippy et
tests verts ; un coffre contenant des champs `KPEX_PASSKEY_*` s'ouvre et se resauvegarde sans perte.

## Décisions

| Sujet | Décision |
|---|---|
| Passkeys | Retirées de l'UI, de l'IPC et du protocole navigateur. La lecture des champs `KPEX_PASSKEY_*` dans `mypass-core` est **conservée** (aucune donnée détruite). |
| Bouton « Mettre à jour » (Sécurité) | Ouvre l'entrée concernée. |
| Backend mock | Réservé à `npm run dev` : fichier à part, chargé seulement si `import.meta.env.DEV`. |
| `extension/dev-key.pem` | Déplacé hors dépôt dans `C:\Users\lucaa\Documents\Projet\MyPass-hors-depot\extension-dev-key.pem`, puis `extension/` supprimé. |
| Firefox | Retiré (décision de la feuille de route : Chrome seul). |

## Design

### 1. Passkeys

**Front, supprimés :** `src/views/PasskeysView.tsx`, `src/components/passkeys/` (dossier entier),
clés i18n `passkeys.*` et `nav.passkeys` (EN + FR).

**Front, nettoyés :**
- `src/App.tsx` : route `/passkeys` et son import.
- `src/components/layout/AppSidebar.tsx` : item étendu (l. 132) et icône repliée (l. 168).
- `src/hooks/useKeyboardShortcuts.ts` : raccourci `Mod+2` (l. 31). Aucun renumérotage des autres.
- `src/lib/tauri.ts` et `src/lib/web.ts` : cas des commandes passkeys.
- `src/types/entry.ts` (`passkey?: PasskeyData`, interface `PasskeyData`) et `src/types/index.ts`
  (export `PasskeyData`).

**Rust, supprimés :** `src-tauri/src/commands/passkeys.rs`, sa ligne dans `commands/mod.rs`, les
5 commandes de `generate_handler!` (`lib.rs`). Dans `native_messaging.rs` : `handle_passkeys_register`,
`handle_passkeys_get`, leurs deux bras de `match` et la ligne de doc qui les cite. L'extension reçoit
alors la réponse générique `Unknown action` (erreur, code 0), comme elle recevait déjà une erreur.
`ADVERTISED_VERSION` ne change pas (il conditionne aussi l'association et le générateur).

**Conservé :** `has_passkey` et le filtrage `KPEX_PASSKEY_*` de `crates/mypass-core/src/ops/entries.rs`.
`apply_custom_fields` fait un upsert des seules clés reçues : une modification depuis l'UI ne
touche pas les champs `KPEX_PASSKEY_*`, qu'elle ne voit pas.

### 2. Code mort Rust

- Supprimés : `greet` (`lib.rs`), `src-tauri/src/security/hibp.rs`, `src-tauri/src/security/zxcvbn.rs`
  et leurs lignes dans `security/mod.rs`. Les dépendances Cargo devenues inutiles sont retirées.
- Les trois `allow(dead_code)` globaux de `lib.rs` (`#![allow(dead_code)]`, sur `mod commands` et
  sur `pub mod security`) sont retirés pour que le compilateur signale de nouveau le code mort.
  Ce qu'il signale alors :
  - `id` de `NativeRequest` (`native_messaging.rs:115`) jamais lu : champ supprimé (serde ignore
    les champs inconnus).
  - Champs d'import reçus puis ignorés : la colonne TOTP du CSV (`CsvColumnMapping.totp`,
    `import_export.rs:33`) et `tags`, `custom_fields`, `totp` des entrées résolues
    (`ResolvedEntry`, `:441`) que `import_entries` n'écrit pas. C'est une perte de données à
    l'import, traitée au sous-projet 5. Ici, `allow(dead_code)`
    ciblé sur ces champs avec un commentaire qui renvoie au sous-projet 5 ; une ligne est ajoutée
    au périmètre du sous-projet 5 dans la feuille de route.
  - Tout autre avertissement révélé est traité de la même façon : suppression si inutilisé,
    sinon `allow` ciblé et commenté.

### 3. Code mort front et mock

- Supprimés : `checkHibp` (`src/lib/crypto.ts`, commande `check_hibp` inexistante) et
  `src/lib/index.ts` (importé nulle part). Le contrôle HIBP réel (`HibpCheck.tsx`, appel direct
  à `api.pwnedpasswords.com`) ne change pas.
- Le mock (`mockStore`, `createMockInvoke` et ses données) passe de `src/lib/tauri.ts` dans
  `src/lib/mock.ts`, qui exporte `createMockInvoke(): TauriInvokeFn`. `getInvoke()` devient :
  Tauri natif → mode web → `if (import.meta.env.DEV)` `await import("./mock")` → sinon
  `throw new Error("NO_BACKEND")`. Vite remplace `import.meta.env.DEV` par `false` en production
  et n'émet pas le chunk du mock.
- `NO_BACKEND` est traduit comme les autres codes de l'écran de déverrouillage :
  `unlock.errors.NO_BACKEND` = « MyPass backend not found — reinstall the app » /
  « Backend MyPass introuvable — réinstallez l'application ».

### 4. Bouton « Mettre à jour » (écran Sécurité)

`PasswordList` (`src/views/SecurityView.tsx`) : au clic, `setKindFilter(null)`, puis
`selectEntry(uuid)`, puis `navigate("/")`. L'ordre compte : `setKindFilter` remet
`selectedEntryId` à `null`.

### 5. Firefox et `extension/`

- `register-nhm.ps1` : section Firefox et mention dans le message final.
- `src-tauri/src/commands/browser.rs` : clé `allowed_extensions` du manifest Native Messaging.
- `README.md` : lignes Passkeys et Firefox (le reste de la doc relève du sous-projet 8).
- `extension/dev-key.pem` déplacé (cf. Décisions), puis dossier `extension/` supprimé. L'ID de
  l'extension ne change pas : le manifest du fork `keepassxc-browser/keepassxc-browser/` porte la
  même clé publique (`"key"`).

## Tests et vérification

**Tests Rust automatisés :**
- `crates/mypass-core` : une entrée portant des champs `KPEX_PASSKEY_*` est modifiée via
  `ops::entries::update` (titre + un champ personnalisé), écrite en octets, relue : les champs
  `KPEX_PASSKEY_*` sont intacts et `has_passkey` reste vrai. Test de caractérisation : il passe
  dès son écriture (aucun code de production ne change dans le noyau) et verrouille le critère.

**Vérifications automatiques :**
- `npm run build` puis `npm run build:web` : la chaîne `mock-vault.kdbx` (propre au mock) est
  absente de `dist/` ; elle est présente dans `src/lib/mock.ts`.
- `grep -ri passkey src src-tauri/src` ne renvoie plus que des commentaires ou rien ;
  `grep -ri firefox register-nhm.ps1 src-tauri/src` ne renvoie rien.

**Non-régression :** `cargo test` (`crates/mypass-core`, `src-tauri`, `server`), `cargo clippy`
sans nouveau warning (référence : 15 lignes `^warning`), `npm run lint`, `npm run build`,
`npm run build:web`.

**Manuel :**
- `npm run tauri dev` : plus d'entrée Passkeys dans la barre latérale (étendue et repliée),
  `Ctrl+2` ne fait rien, `/passkeys` redirige vers `/` ; écran Sécurité → « Mettre à jour » ouvre
  l'entrée.
- `npm run dev` (navigateur) : le mock fonctionne toujours (création de coffre factice, entrées).

## Risques

- **Extension et passkeys :** si l'option passkeys est activée dans l'extension, elle reçoit
  `Unknown action` au lieu de `Passkeys not yet implemented`. Dans les deux cas c'est une erreur ;
  rien ne fonctionnait avant.
- **Mock absent en production :** une build de production lancée hors Tauri et hors mode web
  affiche `NO_BACKEND` au lieu de la maquette. C'est voulu.
- **Clé `.pem` :** elle est ignorée par git, donc sa perte ne se rattrape pas depuis le dépôt.
  Elle est déplacée (pas copiée puis supprimée en deux temps) et sa présence à destination est
  vérifiée avant de supprimer `extension/`.
