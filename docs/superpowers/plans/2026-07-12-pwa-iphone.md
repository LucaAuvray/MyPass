# PWA iPhone (sous-projet 4d) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendre l'app web MyPass installable comme PWA sur l'écran d'accueil iPhone (HTTPS via `tailscale serve`) et solder le backlog UI web hérité du sous-projet 4c.

**Architecture:** Rien de nouveau ne se construit — le manifest, le service worker (vite-plugin-pwa, `registerType: autoUpdate`, `injectRegister` auto) et le mode web existent déjà. 4d = (1) durcir le moteur web (`src/lib/web.ts`) et l'UI pour l'usage mobile réel (suspension iOS, listes stale, contrôles inertes), (2) un fix serveur (les `/api/*` inconnus doivent rester 404 sous statiques), (3) l'infra HTTPS : `tailscale serve` proxifie `https://<host>.<tailnet>.ts.net` → `127.0.0.1:8787` avec un certificat tailnet automatique — prérequis absolu du service worker sur iPhone.

**Tech Stack:** React 19 + TanStack Query (front), vite-plugin-pwa (déjà configuré), Axum 0.8 + tower-http (serveur), tailscale serve (LXC).

## Global Constraints

- Toute chaîne UI passe par i18n (`src/i18n/en.json` + `fr.json`) — jamais de texte en dur.
- Le token n'apparaît JAMAIS dans un log, un message d'erreur ou une URL.
- Le mot de passe maître ne quitte jamais le navigateur (jamais en localStorage).
- Parité desktop des formes JSON : ne rien changer aux shapes émises par `web.ts` (SyncStatus camelCase, `lastSync` = secondes epoch en string).
- Gates front : `npx tsc -b` sans erreur + `npm run lint` (max-warnings 0). Pas de test runner front — la vérification comportementale est faite par le contrôleur (Playwright) après les tâches front.
- Gates serveur : `cargo test` dans `server/` (20 tests existants + les nouveaux), `cargo clippy` sans nouveau warning.
- Le desktop sync continue d'utiliser `http://<container-ip>:8787` — ne pas changer le bind du serveur (`0.0.0.0:8787` reste).
- Commits fréquents, messages `feat(web):`/`fix(web):`/`fix(server):`/`docs:` comme l'historique.

---

### Task 1: Moteur web — parité 5xx, statut « synced » honnête, beforeunload legacy

Trois fixes localisés dans `src/lib/web.ts`, hérités de la revue finale 4c.

**Files:**
- Modify: `src/lib/web.ts`

**Interfaces:**
- Consomme : rien de nouveau.
- Produit : mêmes exports (`createWebInvoke`), comportement de `push()`/`syncNow()` durci. Aucun changement de shape.

**Contexte pour l'implémenteur :** `web.ts` est le backend « mode web » de l'app (le coffre vit dans une session wasm, la persistance est un serveur HTTP GET/PUT `/api/vault` avec ETag/If-Match). Le desktop a le même moteur en Rust (`src-tauri/src/commands/sync.rs`) — la référence de parité. Trois défauts connus :

1. **5xx PUT fatal** : le desktop fait `409 | 500..=599 => continue` (re-GET + merge + re-PUT, max 3) ; le web jette au premier 5xx.
2. **Statut « synced » menteur** : quand une mutation arrive pendant qu'un PUT est en vol (`mutGen !== seen`), `push()` garde `dirty = true` (correct) mais appelle quand même `markSynced()` — l'UI affiche « synchronisé » alors qu'un push est encore dû. Même souci dans `syncNow()` : sa garde `if (sync.status.state === "syncing") markSynced()` ignore `dirty`.
3. **beforeunload vieux Chromium** : `e.preventDefault()` seul ne déclenche pas la confirmation sur les Chromium anciens — il faut aussi `e.returnValue = ""`.

- [ ] **Step 1: Fix 5xx → re-GET dans `push()`**

Dans `src/lib/web.ts`, la boucle de `push()` contient :

```ts
    if (res.status !== 409) throw new Error(`PUT /api/vault: HTTP ${res.status}`);
```

Remplacer par :

```ts
    // Parité desktop (sync.rs : `409 | 500..=599 => continue`) : un 5xx se
    // retente après re-GET + merge ; seuls les 4xx (hors 409) sont fatals.
    if (res.status !== 409 && res.status < 500) {
      throw new Error(`PUT /api/vault: HTTP ${res.status}`);
    }
```

(Le re-GET qui suit est inchangé : sur 5xx l'ETag distant n'a en général pas bougé, `merge_remote` est alors un no-op inoffensif.)

- [ ] **Step 2: `markSynced()` seulement quand tout est à bord**

Dans `push()`, le bloc succès est actuellement :

```ts
    if (res.ok) {
      sync.etag = res.headers.get("etag");
      // Une mutation arrivée pendant ce PUT n'est pas dans les octets envoyés :
      // on ne touche pas à son dirty — son maillon en file re-poussera.
      if (mutGen === seen) sync.dirty = false;
      markSynced();
      return;
    }
```

Remplacer par :

```ts
    if (res.ok) {
      sync.etag = res.headers.get("etag");
      // Une mutation arrivée pendant ce PUT n'est pas dans les octets envoyés :
      // on ne touche ni à dirty ni au statut — son maillon en file re-poussera
      // et posera « synced » quand tout sera réellement à bord.
      if (mutGen === seen) {
        sync.dirty = false;
        markSynced();
      }
      return;
    }
```

Et dans `syncNow()`, remplacer :

```ts
    if (sync.status.state === "syncing") markSynced();
```

par :

```ts
    if (sync.status.state === "syncing" && !sync.dirty) markSynced();
```

(Si `dirty` est encore vrai, un maillon de push est en file : c'est LUI qui posera « synced » à sa fin, ou `markFailed` via son catch.)

- [ ] **Step 3: beforeunload compatible vieux Chromium**

Remplacer :

```ts
  window.addEventListener("beforeunload", (e) => {
    if (sync.dirty) e.preventDefault();
  });
```

par :

```ts
  window.addEventListener("beforeunload", (e) => {
    if (sync.dirty) {
      e.preventDefault();
      // Chromium ancien : sans returnValue, pas de dialogue de confirmation.
      e.returnValue = "";
    }
  });
```

- [ ] **Step 4: Gates**

Run : `npx tsc -b && npm run lint`
Expected : 0 erreur, 0 warning.

- [ ] **Step 5: Commit**

```bash
git add src/lib/web.ts
git commit -m "fix(web): PUT 5xx re-GET comme desktop, statut synced honnête, beforeunload legacy"
```

---

### Task 2: UI web — auto-lock survivant à la suspension iOS, listes rafraîchies après sync, SyncView adaptée au web

**Files:**
- Create: `src/hooks/useSync.ts`
- Modify: `src/components/layout/AppShell.tsx`
- Modify: `src/views/SyncView.tsx`
- Modify: `src/views/UnlockView.tsx`

**Interfaces:**
- Consomme : `tauriCommand` de `@/lib/tauri`, `isWebMode()` de `@/lib/tauri`.
- Produit : `useSyncStatus(): UseQueryResult<SyncStatus>` et `syncNowAndRefresh(qc: QueryClient): Promise<SyncStatus>` exportés de `src/hooks/useSync.ts` ; le type `SyncStatus { state: string; detail: string | null; lastSync: string | null; serverVersion: string | null }` y déménage aussi (il quitte `SyncView.tsx`).

**Contexte :** trois problèmes UI connus.

1. **iOS suspend les timers** : l'auto-lock 60 s de `AppShell` repose sur un `setTimeout` posé quand l'onglet devient masqué — sur iPhone, le JS d'un onglet/PWA en arrière-plan est gelé, le timeout ne fire jamais. Fix : mémoriser `hiddenAt` et, au retour visible, verrouiller si l'absence a dépassé 60 s.
2. **Listes stale après sync** : `sync_now` peut puller du neuf, mais personne n'invalide la query `["entries"]` — la liste reste périmée jusqu'à un changement de vue (constaté par Luca au E2E 4c, point 5). Trois sites appellent `sync_now` : l'intervalle 60 s d'`AppShell`, le bouton de `SyncView`, et les deux appels post-unlock d'`UnlockView`. Fix racine : un helper unique `syncNowAndRefresh` que les trois sites utilisent. (Bénéficie aussi au desktop — même staleness préexistant.)
3. **Contrôles SyncView inertes en web** : le Switch « activer » et le champ URL serveur ne font rien en mode web (l'URL est l'origine de la page, la sync est toujours active). Les masquer en web ; le champ token reste (il permet de remplacer le token stocké).

- [ ] **Step 1: Créer `src/hooks/useSync.ts`**

```ts
/** Statut de synchronisation + déclenchement partagés (desktop et web). */
import { useQuery, type QueryClient } from "@tanstack/react-query";
import { tauriCommand } from "@/lib/tauri";

export type SyncStatus = {
  state: string;
  detail: string | null;
  lastSync: string | null;
  serverVersion: string | null;
};

export function useSyncStatus() {
  return useQuery({
    queryKey: ["sync-status"],
    queryFn: () => tauriCommand<SyncStatus>("get_sync_status"),
    refetchInterval: 5000,
  });
}

/** sync_now + invalidation des listes : un pull peut ramener du neuf. */
export async function syncNowAndRefresh(qc: QueryClient): Promise<SyncStatus> {
  const status = await tauriCommand<SyncStatus>("sync_now");
  await qc.invalidateQueries({ queryKey: ["entries"] });
  return status;
}
```

- [ ] **Step 2: Migrer `SyncView.tsx` sur le hook partagé**

Dans `src/views/SyncView.tsx` :
- Supprimer la définition locale de `SyncStatus` (lignes `export type SyncStatus = {...}`), la définition locale de `useSyncStatus()` et son commentaire `eslint-disable react-refresh` (plus nécessaire une fois la fonction partie du fichier de composant).
- Ajouter : `import { useSyncStatus, syncNowAndRefresh } from "@/hooks/useSync";`
- Remplacer la mutation `syncNow` :

```ts
  const syncNow = useMutation({
    mutationFn: () => syncNowAndRefresh(qc),
    onSettled: () => qc.invalidateQueries({ queryKey: ["sync-status"] }),
  });
```

- [ ] **Step 3: SyncView adaptée au mode web**

Toujours dans `SyncView.tsx` : `import { tauriCommand, isWebMode } from "@/lib/tauri";` puis dans le composant `const web = isWebMode();`. Envelopper le Switch « activer » ET le bloc champ URL serveur dans `{!web && (...)}` :

```tsx
          {!web && (
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <RefreshCw className="size-4 text-primary" />
                <h3 className="text-sm font-semibold">{t("sync.enable")}</h3>
              </div>
              <Switch checked={enabled} onCheckedChange={(checked) => save.mutate(checked)} />
            </div>
          )}

          {!web && (
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">{t("sync.serverUrl")}</label>
              <Input
                value={serverUrl ?? cfg?.serverUrl ?? ""}
                onChange={(e) => setServerUrl(e.target.value)}
                placeholder="https://sync.example.com"
              />
            </div>
          )}
```

Le champ token, le bouton « mettre à jour » et le bloc statut/« synchroniser » restent affichés dans les deux modes. Aucune nouvelle chaîne i18n.

- [ ] **Step 4: AppShell — import + intervalle + hiddenAt**

Dans `src/components/layout/AppShell.tsx` :
- Remplacer `import { useSyncStatus } from "@/views/SyncView";` par `import { useSyncStatus, syncNowAndRefresh } from "@/hooks/useSync";` et ajouter `useQueryClient` :

```ts
import { useQueryClient } from "@tanstack/react-query";
```

- Remplacer l'effet d'intervalle :

```ts
  const queryClient = useQueryClient();

  useEffect(() => {
    const id = setInterval(() => {
      void syncNowAndRefresh(queryClient).catch(() => {});
    }, 60_000);
    return () => clearInterval(id);
  }, [queryClient]);
```

- Remplacer l'effet auto-lock par :

```ts
  // Mode web : verrouiller après 60 s onglet masqué (rien ne doit rester
  // déchiffré dans un onglet oublié en arrière-plan sur iPhone).
  useEffect(() => {
    if (!isWebMode()) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let hiddenAt: number | null = null;
    const doLock = () => void lockDatabase().catch(() => {});
    const onVisibility = () => {
      if (document.hidden) {
        hiddenAt = Date.now();
        timer = setTimeout(doLock, 60_000);
      } else {
        clearTimeout(timer);
        // iOS gèle les timers d'une page en arrière-plan : le timeout ne fire
        // jamais pendant la suspension — on rattrape au retour en comparant
        // les horodatages.
        if (hiddenAt !== null && Date.now() - hiddenAt >= 60_000) doLock();
        hiddenAt = null;
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [lockDatabase]);
```

- [ ] **Step 5: UnlockView — sync post-unlock avec invalidation**

Dans `src/views/UnlockView.tsx` :
- Ajouter les imports :

```ts
import { useQueryClient } from "@tanstack/react-query";
import { syncNowAndRefresh } from "@/hooks/useSync";
```

- Dans le composant : `const queryClient = useQueryClient();`
- Remplacer les DEUX occurrences de `void tauriCommand("sync_now").catch(() => {});` (fin de `handleUnlock` et fin de `handleCreate`) par :

```ts
      void syncNowAndRefresh(queryClient).catch(() => {});
```

- Si `tauriCommand` n'est alors plus utilisé dans le fichier, retirer ce nom de l'import de `@/lib/tauri` (lint `no-unused-vars` sinon).

- [ ] **Step 6: Gates**

Run : `npx tsc -b && npm run lint`
Expected : 0 erreur, 0 warning (vérifier notamment qu'aucun import mort ne reste dans SyncView/UnlockView/AppShell).

- [ ] **Step 7: Commit**

```bash
git add src/hooks/useSync.ts src/components/layout/AppShell.tsx src/views/SyncView.tsx src/views/UnlockView.tsx
git commit -m "feat(web): auto-lock résistant à la suspension iOS, listes invalidées après sync, SyncView épurée en web"
```

---

### Task 3: Serveur — les `/api/*` inconnus restent 404 sous statiques (TDD)

**Files:**
- Modify: `server/src/lib.rs`
- Test: `server/tests/static_files.rs`

**Interfaces:**
- Consomme : `app_with_static(state, Some(dir))` existant, helper `common::test_state(path)` existant.
- Produit : aucun changement d'API — comportement seulement.

**Contexte :** dans `app_with_static`, `.fallback_service(ServeDir...)` REMPLACE le `.fallback(404)` posé juste avant. Résultat : `GET /api/nimporte-quoi` (aucune route ne matche) tombe dans le fallback SPA et répond **200 index.html** au lieu de 404 — un client qui se trompe d'URL d'API reçoit du HTML avec un statut succès. Fix : une route attrape-tout `/api/{*rest}` → 404, prioritaire sur le fallback (dans axum 0.8/matchit, les routes statiques comme `/api/vault` gardent priorité sur le wildcard).

- [ ] **Step 1: Écrire le test qui échoue**

Ajouter à `server/tests/static_files.rs` :

```rust
#[tokio::test]
async fn unknown_api_route_is_404_not_spa_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let web = tmp.path().join("web");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "<html>mypass-web</html>").unwrap();

    let app = app_with_static(common::test_state(tmp.path()), Some(web));

    let res = app.clone().oneshot(request("/api/nope")).await.unwrap();
    assert_eq!(res.status(), 404);
    let res = app.oneshot(request("/api/vault/nope/deep")).await.unwrap();
    assert_eq!(res.status(), 404);
}
```

- [ ] **Step 2: Vérifier qu'il échoue**

Run : `cd server && cargo test unknown_api_route -- --nocapture`
Expected : FAIL — `assertion failed: left == right` avec `left: 200` (le fallback SPA sert index.html).

- [ ] **Step 3: Fix minimal**

Dans `server/src/lib.rs`, fonction `app_with_static`, ajouter la route attrape-tout après le `.merge(vault)` :

```rust
    let router = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .merge(vault)
        // Les /api inconnus doivent rester des 404 même quand les statiques
        // sont servies : sans cet attrape-tout, fallback_service (qui REMPLACE
        // le .fallback ci-dessous) les transformerait en index.html 200.
        .route(
            "/api/{*rest}",
            axum::routing::any(|| async { axum::http::StatusCode::NOT_FOUND }),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
        // ponytail: sans ce fallback explicite, merge() hérite du fallback
        // 404 par défaut du sous-routeur `vault`, déjà enveloppé par le
        // middleware d'auth par son .layer() — un chemin inconnu répondrait
        // 401 au lieu de 404. On force un vrai 404 pour les routes non /api.
        .fallback(|| async { axum::http::StatusCode::NOT_FOUND });
```

(Seules les deux nouvelles lignes `.route("/api/{*rest}", ...)` changent — le reste est le code existant, montré pour situer l'insertion.)

- [ ] **Step 4: Vérifier que tout passe**

Run : `cd server && cargo test`
Expected : tous les tests passent (20 existants + 1 nouveau = 21), y compris `serves_index_and_spa_fallback_and_assets` (qui prouve que `/api/vault` → 401 et `/api/health` → 200 gardent priorité sur le wildcard).

Run : `cd server && cargo clippy`
Expected : aucun nouveau warning.

- [ ] **Step 5: Commit**

```bash
git add server/src/lib.rs server/tests/static_files.rs
git commit -m "fix(server): /api/* inconnus -> 404 au lieu du fallback SPA index.html 200"
```

---

### Task 4: Metas PWA iOS + documentation CLAUDE.md

**Files:**
- Modify: `index.html`
- Modify: `CLAUDE.md`

**Interfaces:** aucune — HTML statique et doc.

**Contexte :** le manifest (`public/manifest.json`, `display: standalone`, icônes 192/512 présentes dans `public/icons/`) et l'`apple-touch-icon` existent déjà. Il manque les deux metas Apple historiques (sur iOS elles restent le signal le plus fiable pour le mode plein écran une fois installé) et la documentation du mode web dans CLAUDE.md (dette notée en 4c : un futur contributeur qui lance `npm run build` sans wasm-pack installé échoue sans comprendre).

- [ ] **Step 1: Metas Apple dans `index.html`**

Après la ligne `<link rel="apple-touch-icon" ...>`, ajouter :

```html
    <meta name="apple-mobile-web-app-capable" content="yes" />
    <meta name="apple-mobile-web-app-status-bar-style" content="default" />
```

- [ ] **Step 2: Documenter le mode web dans `CLAUDE.md`**

Dans la section `## Commands`, après la ligne `npm run format:check`, ajouter au bloc bash :

```bash
npm run build:wasm      # wasm-pack build de crates/mypass-wasm (requis : wasm-pack installé)
npm run dev:web         # Vite dev en mode web (coffre wasm + sync serveur, pas de mock)
npm run build:web       # build de l'app web (déployée sur le serveur de sync)
npm run smoke:wasm      # smoke test Node du module wasm
```

Et juste après ce bloc, ajouter la note :

```markdown
`npm run build` exécute `build:wasm` d'abord — **wasm-pack doit être installé** sinon le build frontend échoue.
```

Dans la section `## Architecture`, après la sous-section « Critical: browser dev mode uses a mock backend », ajouter :

```markdown
### Web mode & sync (self-hosted)

- `crates/mypass-core/` — the KDBX/crypto/ops core, extracted from `src-tauri` (which re-exports it as `kdbx`). Compiles to wasm32.
- `crates/mypass-wasm/` — wasm-bindgen bindings over the core (JSON strings in/out, `thread_local` session mirroring the desktop DbState). Built by `npm run build:wasm` into `crates/mypass-wasm/pkg` (aliased as `@wasm`).
- `server/` — standalone Axum sync server (GET/PUT `/api/vault`, ETag/If-Match/409, Bearer auth, versioned encrypted blobs). Also serves the web app build when `MYPASS_STATIC_DIR` is set. Deployed on a Proxmox LXC, reached over Tailscale.
- `src/lib/web.ts` — third backend of `getInvoke()` in `tauri.ts` (native Tauri → web → browser mock), selected by `vite --mode web`: the vault lives in the wasm session, every mutation is pushed to the server (dirty flag, serialized push chain, 409 → merge → retry).
- Desktop sync lives in `src-tauri/src/commands/sync.rs` + `crates/mypass-core/src/merge.rs` (LWW merge, tombstones) — the web engine in `web.ts` mirrors it and must keep JSON shape parity with the Tauri IPC.
```

- [ ] **Step 3: Gates**

Run : `npm run build:web`
Expected : build OK (prouve que index.html reste valide et que le SW se génère).

- [ ] **Step 4: Commit**

```bash
git add index.html CLAUDE.md
git commit -m "docs: metas PWA iOS + mode web/sync documenté dans CLAUDE.md"
```

---

### Task 5: Vérification locale (contrôleur) puis déploiement LXC + tailscale serve

**Exécutée par le contrôleur (pas de subagent) — accès SSH au LXC requis.**

**Files:** aucun changement de code. Infra : LXC `<container-ip>` (SSH root par clé), binaire `/usr/local/bin/mypass-server`, sources `/opt/mypass-src`, statiques `/opt/mypass-web`, service systemd `mypass-server`.

- [ ] **Step 1: E2E local Playwright (vérif des tasks 1-2)**

Serveur local avec statiques (`MYPASS_STATIC_DIR=dist`, données scratchpad) + navigateur : vérifier (a) unlock → entrée visible ; (b) créer une entrée dans l'onglet B, puis « synchroniser » dans l'onglet A → **la liste de A se met à jour sans navigation** (fix invalidation) ; (c) SyncView en web ne montre ni Switch ni champ URL, mais montre token + statut ; (d) statut reste cohérent après mutations rapides.

- [ ] **Step 2: Rebuild + redéploiement serveur (Task 3 change le binaire)**

Procédure connue : `git archive` du HEAD → LXC `/opt/mypass-src`, `cargo build --release -j 1` (~40 s), `systemctl stop mypass-server`, `cp target/release/mypass-server /usr/local/bin/`, `systemctl start`. Puis vérifier depuis le PC : `/api/health` → ok, `/api/nope` → 404.

- [ ] **Step 3: Redéploiement des statiques**

`npm run build:web`, tar de `dist/` → LXC (`tar --no-same-owner -x` dans `/opt/mypass-web`). Vérifier index 200.

- [ ] **Step 4: Activer tailscale serve (HTTPS)**

Sur le LXC : `tailscale serve --bg 8787` (proxy `https://<host>.<tailnet>.ts.net` → `127.0.0.1:8787`, cert Let's Encrypt automatique, persistant au reboot).

**Précondition possible :** MagicDNS + « HTTPS Certificates » doivent être activés sur le tailnet. Si la commande échoue avec un message le demandant → **action Luca** : console admin Tailscale (https://login.tailscale.com/admin/dns) → activer HTTPS Certificates. Reprendre ensuite.

Récupérer le nom DNS exact (`tailscale status --json` ou la sortie de serve). Vérifier depuis le PC : `curl https://<host>.<tailnet>.ts.net/api/health` → `ok` (la première requête peut prendre ~30 s, provisioning du cert). Le desktop reste sur `http://<container-ip>:8787` — rien à reconfigurer.

- [ ] **Step 5: Ledger + point d'étape**

Consigner dans `.superpowers/sdd/progress.md`, donner l'URL HTTPS à Luca pour la Task 6.

---

### Task 6: E2E iPhone (Luca — checklist guidée)

**Exécutée avec Luca sur son iPhone. Prérequis : app Tailscale iOS connectée au tailnet.**

- [ ] **Step 1: Accès Safari** — ouvrir `https://<host>.<tailnet>.ts.net` dans Safari (VPN actif) : page de déverrouillage sans avertissement de certificat.
- [ ] **Step 2: Unlock** — saisir token + mot de passe maître : le coffre s'ouvre, les entrées s'affichent.
- [ ] **Step 3: Installation** — bouton Partager → « Sur l'écran d'accueil » : l'icône MyPass apparaît.
- [ ] **Step 4: Lancement standalone** — ouvrir depuis l'icône : plein écran sans barre Safari. **Attendu : le token doit être resaisi UNE fois** (la PWA installée a son propre localStorage, séparé de Safari — comportement normal, pas un bug).
- [ ] **Step 5: Aller-retour sync** — créer une entrée sur l'iPhone → synchroniser → vérifier qu'elle apparaît sur le desktop ; modifier sur desktop → synchroniser des deux côtés → vérifier sur l'iPhone **que la liste se met à jour sans changer de vue** (fix Task 2).
- [ ] **Step 6: Auto-lock iOS** — passer la PWA en arrière-plan > 60 s, revenir : l'app doit être verrouillée (fix hiddenAt).
- [ ] **Step 7: Ledger** — consigner les résultats ; tout écart = fix avant merge.

---

## Self-Review

- **Couverture spec :** la spec 4d = « PWA : manifest + service worker → installable sur l'écran d'accueil iPhone » (déjà construits en amont ; 4d les rend atteignables via HTTPS — Task 5, et valide — Task 6) + backlog 4c intégral : 5xx re-GET (T1), synced cosmétique (T1), beforeunload returnValue (T1), invalidation ENTRIES_KEY (T2), hiddenAt iOS (T2), SyncView inerte (T2), /api/* → 404 (T3), CLAUDE.md (T4). Couvert.
- **Placeholders :** aucun — chaque étape code montre le code.
- **Cohérence des types :** `SyncStatus` déménage dans `useSync.ts` (T2 step 1) et ses deux seuls importeurs (`AppShell`, `SyncView`) sont mis à jour dans la même task ; `web.ts` garde son type local privé (inchangé, T1 n'y touche pas). `syncNowAndRefresh(qc)` a la même signature aux trois sites d'appel.
