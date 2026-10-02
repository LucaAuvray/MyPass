# MyPass Sync — serveur auto-hébergé + app web (design)

Date : 2026-07-09
Statut : validé par Luca (sections approuvées une par une)

## Objectif

Transformer MyPass (app desktop Tauri, coffre KDBX4 local) en solution multi-appareils type 1Password auto-hébergée :

- Un serveur de synchronisation 24/7 dans **un conteneur LXC Proxmox** (un seul conteneur, pas de DB séparée).
- Le coffre accessible et synchronisé sur tous les PC (app Tauri existante) **et sur iPhone via une app web/PWA** (même UI React).
- App native mobile envisageable plus tard ; le serveur est conçu pour supporter les deux.

## Contraintes et décisions actées

| Décision | Choix |
|---|---|
| Utilisateurs | Un seul (Luca). Pas de comptes, pas de partage. |
| Exposition réseau | Jamais exposé sur internet : accès via VPN (WireGuard/Tailscale) uniquement. |
| Modèle de sécurité | Zero-knowledge : le serveur stocke le `.kdbx` chiffré tel quel, ne possède aucune clé. Déchiffrement toujours côté client. |
| Format | KDBX4 reste la source de vérité (compatibilité KeePass conservée — Strongbox/KeePassium utilisables sur iPhone en attendant la PWA). |
| Approche retenue | A — « blob KDBX versionné » (rejeté : B API par entrée type Bitwarden = réécriture de la persistance sans besoin ; C WebDAV seul = pas d'app web MyPass). |

## Architecture

```
┌─────────────── LXC Proxmox (1 conteneur, 2 vCPU / 1 Go RAM / 10 Go) ──────┐
│  mypass-server (binaire Rust / Axum)                                       │
│  ├── API HTTP /api/vault (GET/PUT du .kdbx versionné, If-Match)           │
│  ├── Fichiers statiques → app web React (build Vite)                       │
│  └── Stockage : /var/lib/mypass/vaults/ (versions du .kdbx + index.json)  │
│  Accessible uniquement via VPN                                             │
└─────────────────────────────────────────────────────────────────────────────┘
         ▲                        ▲                        ▲
  PC : app Tauri          iPhone : Safari/PWA        Autres PC :
  + module sync           (WASM déchiffre             Tauri ou web
                           dans le navigateur)
```

Principes :

1. **Serveur bête et aveugle** — stocke des versions du fichier chiffré ; une compromission du LXC ne donne qu'un coffre chiffré Argon2.
2. **Déchiffrement côté client uniquement** — Rust natif sur desktop, le **même code Rust compilé en WASM** dans le navigateur (pas de réimplémentation JS de la crypto).
3. **Un seul conteneur** — serveur + stockage fichiers + app web statique. Pas de base de données : le KDBX est la base de données.

## Composants

### 1. `mypass-server` (nouveau crate, `server/` dans le repo)

- Axum, 4 endpoints :
  - `GET /api/vault` → dernière version du `.kdbx` + numéro de version (header)
  - `PUT /api/vault` avec `If-Match: <version>` → nouvelle version ; version obsolète → `409 Conflict`
  - `GET /api/vault/versions` → historique
  - `GET /api/health`
- **Auth** : token statique Bearer généré à l'installation, vérifié via hash Argon2 côté serveur (le token n'est pas stocké en clair). Pas de gestion de comptes.
- **Versioning** : chaque `PUT` écrit `vault.v{n}.kdbx` + met à jour `index.json` ; rétention des 50 dernières versions (rollback/backup).
- **Écriture atomique** : fichier temporaire + rename — jamais de version corrompue.
- Sert aussi l'app web (statique).

### 2. `mypass-core` (crate partagé, extrait de `src-tauri`)

- Extraction de `kdbx/` (reader, writer, crypto, keys, xml) dans un crate compilable en deux cibles : natif (utilisé par Tauri comme aujourd'hui) et `wasm32-unknown-unknown` (navigateur, via `wasm-bindgen`).
- Contient aussi la **fusion KDBX** (voir Flux) — partagée desktop/web.
- Point de vigilance : Argon2 en WASM ≈ 2-3× plus lent que natif → déverrouillage 1-2 s au lieu de ~0,5 s. Acceptable ; paramètres KDF inchangés.

### 3. Client web (3ᵉ implémentation du backend dans `src/lib/`)

- `src/lib/tauri.ts` détecte déjà l'environnement (Tauri réel vs mock navigateur). Ajout d'un mode « web » : les commandes (`get_entries`, `create_entry`, `save_database`…) sont implémentées en TypeScript sur le coffre déchiffré en mémoire par le WASM ; `save_database` fait le `PUT`.
- PWA : manifest + service worker → installable sur l'écran d'accueil iPhone.
- Verrouillage auto quand l'onglet passe en arrière-plan ; rien n'est persisté en clair dans le navigateur.

### 4. Module de sync desktop (`src-tauri/src/commands/sync.rs`)

- Au déverrouillage : `GET`, comparaison avec la copie locale, fusion si besoin.
- À chaque sauvegarde : `PUT` avec `If-Match` ; sur `409` → fusion puis re-`PUT`.
- Hors-ligne : comportement actuel inchangé (copie locale), sync à la reconnexion.

## Flux de données

**Déverrouillage** : `GET /api/vault` → `.kdbx` chiffré + version `n` (hors-ligne desktop : copie locale). Mot de passe maître → Argon2 → déchiffrement local. Le mot de passe ne quitte jamais l'appareil.

**Sauvegarde** : chiffrement local → `PUT` avec `If-Match: n` → `200` = version `n+1` (desktop met aussi à jour sa copie locale).

**Conflit** : `PUT` → `409` + version distante → déchiffrement de la distante → **fusion entrée par entrée** (la plus récente gagne, par `LastModificationTime` ; les versions perdantes vont dans l'historique d'entrée KDBX — rien n'est perdu ; sémantique alignée sur KeePassXC, code vendored en référence) → re-chiffrement → re-`PUT`. Automatique, sans dialogue utilisateur sauf cas impossible.

**iPhone (PWA)** : VPN actif → URL → téléchargement du coffre → déchiffrement WASM → travail en mémoire → sauvegarde = `PUT`.

## Gestion d'erreurs

- Serveur injoignable → desktop : mode hors-ligne silencieux + bandeau « non synchronisé » ; web : message clair (vérifier le VPN).
- `PUT` interrompu → atomicité côté serveur (temp + rename).
- Token invalide → `401`, le client redemande le token.
- Rollback : toute version parmi les 50 conservées est restaurable.

## Tests

- **Fusion KDBX** (code critique) : tests unitaires Rust dans `mypass-core` — ajout des deux côtés, modif concurrente d'une même entrée, suppression vs modif.
- **Serveur** : tests d'intégration Axum — GET/PUT/409/atomicité.
- **WASM ↔ natif** : un coffre chiffré en natif se déchiffre en WASM et inversement (vecteurs de test partagés).
- **E2E manuel** : deux clients modifient en parallèle → convergence vérifiée.

## Découpage en sous-projets (ordre d'implémentation)

1. **Extraction `mypass-core`** — sortir `kdbx/` de `src-tauri` en crate partagé, sans changement de comportement (les tests Rust existants passent).
2. **`mypass-server`** — API vault versionnée + auth token + tests d'intégration.
3. **Sync desktop** — module `sync.rs` + fusion KDBX dans `mypass-core` + UI (bandeau état sync, réglages serveur/token).
4. **App web/PWA** — build WASM de `mypass-core`, mode « web » dans `src/lib/`, PWA, servie par le serveur.
5. **Déploiement LXC** — conteneur Debian minimal (2 vCPU, 1 Go RAM, 10 Go), binaire `mypass-server` en service systemd, accès VPN.

Chaque sous-projet aura son propre plan d'implémentation ; hors scope ici : app native iOS, multi-utilisateur, partage, exposition internet.
