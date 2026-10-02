/**
 * Backend « web » : le coffre déchiffré vit dans la session wasm
 * (crates/mypass-wasm), la persistance est le serveur de sync
 * (GET/PUT /api/vault, blob KDBX chiffré, ETag/If-Match/409).
 *
 * Parité de sortie stricte avec l'IPC Tauri desktop : le JSON du wasm est
 * retourné tel quel (EntryInfo snake_case sauf customFields, etc.) — aucune
 * conversion de forme ici.
 *
 * Décision de PUT : dirty flag posé à chaque mutation, effacé après un PUT
 * réussi — PAS le `changed` de merge_remote (qui ne voit que le sens pull).
 */
import { clearWebToken, getWebToken, setWebToken, type TauriInvokeFn } from "./tauri";

type WasmModule = typeof import("@wasm/mypass_wasm");

let wasmModule: WasmModule | null = null;

async function loadWasm(): Promise<WasmModule> {
  if (wasmModule) return wasmModule;
  const mod = await import("@wasm/mypass_wasm");
  await mod.default(); // init : Vite résout l'URL du .wasm via import.meta.url
  wasmModule = mod;
  return mod;
}

// ---------------------------------------------------------------------------
// État sync (miroir de src-tauri/src/commands/sync.rs)
// ---------------------------------------------------------------------------

type SyncStatus = {
  state: string; // idle | syncing | synced | offline | error
  detail: string | null;
  lastSync: string | null;
  serverVersion: string | null;
};

const sync = {
  etag: null as string | null, // renvoyé VERBATIM en If-Match
  dirty: false, // mutations locales pas encore poussées
  status: { state: "idle", detail: null, lastSync: null, serverVersion: null } as SyncStatus,
};

// Fermer l'onglet avec un push en attente perdrait la mutation :
// confirmation native tant que dirty.
if (typeof window !== "undefined") {
  window.addEventListener("beforeunload", (e) => {
    if (sync.dirty) {
      e.preventDefault();
      // Chromium ancien : sans returnValue, pas de dialogue de confirmation.
      e.returnValue = "";
    }
  });
}

function markSynced(): void {
  sync.status = {
    state: "synced",
    detail: null,
    // Parité desktop : SyncView interprète lastSync comme des secondes epoch
    lastSync: String(Math.floor(Date.now() / 1000)),
    serverVersion: sync.etag ? sync.etag.replace(/"/g, "") : null,
  };
}

function markFailed(e: unknown): void {
  // fetch qui échoue au niveau réseau = TypeError → offline ; sinon error.
  sync.status = {
    ...sync.status,
    state: e instanceof TypeError ? "offline" : "error",
    detail: e instanceof Error ? e.message : String(e),
  };
}

function authHeaders(): Record<string, string> {
  const token = getWebToken();
  return token ? { authorization: `Bearer ${token}` } : {};
}

function rejectUnauthorized(res: Response): void {
  if (res.status === 401) {
    clearWebToken();
    throw new Error("Token serveur invalide — ressaisissez-le");
  }
}

async function fetchVault(): Promise<{ bytes: Uint8Array; etag: string } | null> {
  const res = await fetch("/api/vault", { headers: authHeaders() });
  if (res.status === 404) return null; // serveur vide, pas une erreur
  rejectUnauthorized(res);
  if (!res.ok) throw new Error(`GET /api/vault: HTTP ${res.status}`);
  const etag = res.headers.get("etag") ?? "";
  return { bytes: new Uint8Array(await res.arrayBuffer()), etag };
}

async function putVault(bytes: Uint8Array, ifMatch: string | null): Promise<Response> {
  return fetch("/api/vault", {
    method: "PUT",
    headers: {
      ...authHeaders(),
      "content-type": "application/octet-stream",
      ...(ifMatch ? { "if-match": ifMatch } : {}),
    },
    body: bytes as unknown as BodyInit,
  });
}

/** Pousse l'état courant ; sur 409 : re-GET + merge + re-PUT (max 3). */
async function push(): Promise<void> {
  const wasm = await loadWasm();
  for (let attempt = 0; attempt < 3; attempt++) {
    const seen = mutGen;
    const res = await putVault(wasm.save_vault(), sync.etag);
    rejectUnauthorized(res);
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
    // Parité desktop (sync.rs : `409 | 500..=599 => continue`) : un 5xx se
    // retente après re-GET + merge ; seuls les 4xx (hors 409) sont fatals.
    if (res.status !== 409 && res.status < 500) {
      throw new Error(`PUT /api/vault: HTTP ${res.status}`);
    }
    const remote = await fetchVault();
    if (remote) {
      wasm.merge_remote(remote.bytes);
      sync.etag = remote.etag;
    }
  }
  throw new Error("Conflit de synchronisation persistant (3 tentatives)");
}

// Les PUT passent TOUS par une même chaîne de promesses : jamais deux boucles
// push concurrentes depuis cet onglet (deux retries parallèles se voleraient
// l'ETag et épuiseraient leurs 3 tentatives sur un faux conflit).
let pushChain: Promise<void> = Promise.resolve();

// Compteur de générations de mutations : un push qui se termine ne doit
// effacer dirty QUE si aucune mutation n'est arrivée pendant son vol —
// sinon la mutation en vol ne serait jamais poussée (perte silencieuse).
let mutGen = 0;

function markDirty(): void {
  mutGen++;
  sync.dirty = true;
}

/** Ajoute un push à la chaîne et retourne ce maillon (rejette si CE push échoue). */
function queuedPush(): Promise<void> {
  const link = pushChain.then(() => (sync.dirty ? push() : undefined));
  pushChain = link.catch(() => {}); // la chaîne survit aux échecs
  return link;
}

function schedulePush(): void {
  markDirty();
  void queuedPush().catch(markFailed);
}

/** The PWA's "save as": a browser download of in-memory content. */
function downloadFile(content: string, fileName: string, type: string): void {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  a.click();
  // Revoked later: iOS Safari drops the download if the URL dies right away.
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** Cycle complet : pull + merge, puis push si des mutations locales attendent. */
async function syncNow(): Promise<SyncStatus> {
  const wasm = await loadWasm();
  if (!wasm.is_unlocked()) return sync.status;
  sync.status = { ...sync.status, state: "syncing" };
  try {
    const remote = await fetchVault();
    if (remote && remote.etag !== sync.etag) {
      // Pull : le distant avait du neuf → notre session l'absorbe. On ne pose
      // PAS dirty ici : pousser un état qui ne fait que refléter le distant
      // créerait une boucle de versions entre deux clients ouverts. Seules
      // les mutations locales (schedulePush) justifient un PUT.
      wasm.merge_remote(remote.bytes);
      sync.etag = remote.etag;
    }
    if (!remote) markDirty(); // serveur vide → premier PUT sans If-Match
    if (sync.dirty) await queuedPush();
    if (sync.status.state === "syncing" && !sync.dirty) markSynced();
  } catch (e) {
    markFailed(e);
  }
  return sync.status;
}

// ---------------------------------------------------------------------------
// DatabaseInfo minimal (personne ne lit ces champs côté front aujourd'hui ;
// parité de forme avec la sérialisation desktop, valeurs neutres)
// ---------------------------------------------------------------------------

function databaseInfo(name: string) {
  return {
    file_path: "serveur",
    name,
    description: "",
    encryption: "Aes256",
    kdf: "Argon2id",
    groups: 0,
    entries: 0,
    created: "",
    modified: "",
  };
}

// ---------------------------------------------------------------------------
// Invoke web
// ---------------------------------------------------------------------------

export function createWebInvoke(): TauriInvokeFn {
  return async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    const wasm = await loadWasm();

    switch (cmd) {
      // --- base de données ---
      case "open_database": {
        const remote = await fetchVault();
        if (!remote) {
          throw new Error("Aucun coffre sur le serveur — utilisez « créer » d'abord");
        }
        wasm.open_vault(remote.bytes, String(args?.password ?? ""), undefined);
        sync.etag = remote.etag;
        sync.dirty = false;
        markSynced();
        return databaseInfo("MyPass") as T;
      }

      case "create_database": {
        const name = String(args?.name ?? "MyPass");
        const bytes = wasm.create_vault(name, String(args?.password ?? ""));
        wasm.open_vault(bytes, String(args?.password ?? ""), undefined);
        sync.etag = null;
        markDirty();
        await queuedPush(); // premier PUT sans If-Match ; 409 si un coffre existe déjà
        return databaseInfo(name) as T;
      }

      case "save_database":
        markDirty();
        await queuedPush();
        return undefined as T;

      case "lock_database":
        // Vider la file de push AVANT de fermer : sans ça, éditer puis
        // verrouiller (ou l'auto-lock) perdrait la mutation en attente — le
        // pendant web du db.save() synchrone du desktop. En cas d'échec
        // (hors-ligne), on rejette : le coffre reste ouvert et l'UI montre
        // l'erreur plutôt qu'une perte silencieuse.
        if (sync.dirty) await queuedPush();
        wasm.close_vault();
        sync.etag = null;
        sync.dirty = false;
        sync.status = { state: "idle", detail: null, lastSync: null, serverVersion: null };
        return undefined as T;

      case "get_database_info":
        return (wasm.is_unlocked() ? databaseInfo("MyPass") : null) as T;

      // --- entrées (parité IPC : JSON wasm tel quel) ---
      case "get_entries":
        return JSON.parse(wasm.get_entries((args?.groupUuid as string) ?? null)) as T;

      case "get_entry":
        return JSON.parse(wasm.get_entry(String(args?.uuid))) as T;

      case "create_entry": {
        const created = JSON.parse(wasm.create_entry(JSON.stringify(args?.entry ?? {})));
        schedulePush();
        return created as T;
      }

      case "update_entry": {
        const updated = JSON.parse(
          wasm.update_entry(String(args?.uuid), JSON.stringify(args?.update ?? {})),
        );
        schedulePush();
        return updated as T;
      }

      case "delete_entry":
        wasm.delete_entry(String(args?.uuid));
        schedulePush();
        return undefined as T;

      case "duplicate_entry": {
        const dup = JSON.parse(wasm.duplicate_entry(String(args?.uuid)));
        schedulePush();
        return dup as T;
      }

      // --- générateur / TOTP (purs) ---
      case "generate_password":
        return wasm.generate_password(JSON.stringify(args?.config ?? {})) as T;

      case "generate_passphrase":
        return wasm.generate_passphrase(JSON.stringify(args?.config ?? {})) as T;

      case "evaluate_strength":
        return JSON.parse(wasm.evaluate_strength(String(args?.password ?? ""))) as T;

      case "generate_totp_code":
        return JSON.parse(
          wasm.generate_totp_code(
            String(args?.secret ?? ""),
            (args?.algorithm as string) ?? undefined,
            (args?.digits as number) ?? undefined,
            (args?.period as number) ?? undefined,
          ),
        ) as T;

      case "generate_totp_secret":
        return wasm.generate_totp_secret() as T;

      // --- sync ---
      case "get_sync_config":
        return {
          serverUrl: window.location.origin,
          enabled: true,
          hasToken: !!getWebToken(),
        } as T;

      case "set_sync_config": {
        // En mode web, seule la mise à jour du token a un sens (l'URL est l'origine).
        if (typeof args?.token === "string" && args.token) setWebToken(args.token);
        return null as T;
      }

      case "get_sync_status":
        return sync.status as T;

      case "sync_now":
        return (await syncNow()) as T;

      // --- intégrations desktop : états inertes pour que les vues s'affichent ---
      case "get_browser_status":
        return { connected: false, browsers: [], extensionVersion: "", protocolVersion: "" } as T;
      case "is_browser_integration_enabled":
      case "is_ssh_agent_enabled":
      case "toggle_browser_integration":
      case "toggle_ssh_agent":
        return false as T;
      case "get_ssh_agent_status":
        return { enabled: false, listening: false, serviceRunning: false } as T;

      case "parse_import":
        return JSON.parse(wasm.parse_import(String(args?.content ?? ""))) as T;

      case "import_entries": {
        const result = JSON.parse(wasm.import_entries(JSON.stringify(args?.entries ?? [])));
        schedulePush();
        return result as T;
      }

      case "export_entries": {
        const format = String(args?.format);
        const content = wasm.export_entries(format, JSON.stringify(args?.uuids ?? []));
        downloadFile(content, String(args?.fileName), format === "csv" ? "text/csv" : "application/json");
        return { saved: true } as T;
      }

      default:
        throw new Error(`Commande indisponible en mode web: ${cmd}`);
    }
  };
}
