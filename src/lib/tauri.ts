/**
 * Typed wrapper around Tauri's invoke function.
 * 
 * In Tauri runtime, uses the native IPC bridge.
 * In browser dev mode, uses a mock that simulates basic operations.
 */
import type { Entry } from "@/stores/entriesStore";
import type { Group } from "@/types/group";

export type TauriInvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

const TOKEN_KEY = "mypass.web.token";

/** Mode « web » : app servie par mypass-server, coffre en wasm (build `--mode web`). */
export function isWebMode(): boolean {
  return (
    typeof window !== "undefined" &&
    !("__TAURI_INTERNALS__" in window) &&
    import.meta.env.MODE === "web"
  );
}

export function hasWebToken(): boolean {
  return !!localStorage.getItem(TOKEN_KEY);
}

export function setWebToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token.trim());
}

export function getWebToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

/** 401 serveur : le token stocké est invalide → le purger pour que
 * l'UnlockView ré-affiche le champ (spec : « le client redemande le token »). */
export function clearWebToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

let _invoke: TauriInvokeFn;

async function getInvoke(): Promise<TauriInvokeFn> {
  if (_invoke) return _invoke;

  // Check if we're running inside a real Tauri window.
  // Tauri v2 only injects `__TAURI__` when withGlobalTauri is enabled;
  // `__TAURI_INTERNALS__` is always present in a real Tauri webview.
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  if (isTauri) {
    try {
      const mod = await import("@tauri-apps/api/core");
      if (typeof mod.invoke === "function") {
        _invoke = mod.invoke as TauriInvokeFn;
        console.info("[MyPass] Tauri runtime detected. Using native IPC.");
        return _invoke;
      }
    } catch (e) {
      console.warn("[MyPass] Tauri API import failed:", e);
    }
  }

  if (isWebMode()) {
    console.info("[MyPass] Web mode. Vault via WASM + sync server.");
    const { createWebInvoke } = await import("./web");
    _invoke = createWebInvoke();
    return _invoke;
  }

  console.info("[MyPass] Browser dev mode. Using mock backend.");
  _invoke = createMockInvoke();
  return _invoke;
}

// =============================================================================
// Mock backend for browser-only development
// =============================================================================

const mockStore = {
  isOpen: false,
  vaultExists: false,
  entries: [] as Entry[],
  groups: [] as Group[],
  browserIntegrationEnabled: false,
  sshAgentEnabled: false,
};

function createMockInvoke(): TauriInvokeFn {
  return async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    await new Promise((r) => setTimeout(r, 300)); // Simulate IPC latency

    switch (cmd) {
      case "get_database_info":
        if (!mockStore.isOpen) return null as T;
        return {
          filePath: "mock-vault.kdbx",
          name: "Mock Vault",
          description: "Browser dev mode",
          encryption: "Aes256",
          kdf: "Argon2id",
          groups: 1,
          entries: mockStore.entries.length,
          created: new Date().toISOString(),
          modified: new Date().toISOString(),
        } as T;

      case "get_vault_location":
        return { path: "mock-vault.kdbx", exists: mockStore.vaultExists } as T;

      case "create_database":
      case "open_database":
      case "fetch_vault_from_server":
        mockStore.isOpen = true;
        mockStore.vaultExists = true;
        // Mock some entries for demo
        if (mockStore.entries.length === 0) {
          mockStore.entries = [
            { uuid: "mock-1", group: "root", title: "Google", username: "user@gmail.com", url: "https://google.com", password: "mock-pass-1", notes: "", icon: 0, tags: ["email"], customFields: {}, created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-2", group: "root", title: "GitHub", username: "dev", url: "https://github.com", password: "mock-pass-2", notes: "Code repository", icon: 0, tags: ["dev"], customFields: {}, created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-3", group: "root", title: "Twitter", username: "@handle", url: "https://twitter.com", password: "mock-pass-3", notes: "", icon: 0, tags: ["social"], customFields: {}, created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-4", group: "root", title: "Amazon", username: "user@example.com", url: "https://amazon.com", password: "weak", notes: "Shopping", icon: 0, tags: ["shopping"], customFields: {}, created: "2024-01-15T00:00:00Z", modified: "2024-01-15T00:00:00Z" },
          ];
        }
        return {
          filePath: "mock-vault.kdbx",
          name: (args?.name as string) ?? "Mock Vault",
          description: "",
          encryption: "Aes256",
          kdf: "Argon2id",
          groups: 1,
          entries: mockStore.entries.length,
          created: new Date().toISOString(),
          modified: new Date().toISOString(),
        } as T;

      case "save_database":
        return undefined as T;

      case "lock_database":
        mockStore.isOpen = false;
        return undefined as T;

      case "get_entries":
        return mockStore.entries as T;

      case "get_entry":
        return mockStore.entries.find((e) => e.uuid === args?.uuid) as T;

      case "create_entry": {
        const entry = args?.entry as Record<string, unknown>;
        const newEntry: Entry = {
          uuid: `mock-${Date.now()}`,
          group: (entry?.groupUuid as string) ?? "root",
          title: (entry?.title as string) ?? "New Entry",
          username: (entry?.username as string) ?? "",
          url: (entry?.url as string) ?? "",
          password: (entry?.password as string) ?? "",
          notes: (entry?.notes as string) ?? "",
          icon: 0,
          tags: (entry?.tags as string[]) ?? [],
          customFields: (entry?.customFields as Record<string, string>) ?? {},
          created: new Date().toISOString(),
          modified: new Date().toISOString(),
        };
        mockStore.entries.push(newEntry);
        return newEntry as T;
      }

      case "update_entry": {
        const idx = mockStore.entries.findIndex((e) => e.uuid === args?.uuid);
        if (idx >= 0) {
          const update = args?.update as Partial<Entry>;
          mockStore.entries[idx] = { ...mockStore.entries[idx], ...update };
          return mockStore.entries[idx] as T;
        }
        return null as T;
      }

      case "delete_entry":
        mockStore.entries = mockStore.entries.filter((e) => e.uuid !== args?.uuid);
        return undefined as T;

      case "search_entries": {
        const q = ((args?.query as string) ?? "").toLowerCase();
        return mockStore.entries.filter(
          (e) => e.title.toLowerCase().includes(q) || e.username.toLowerCase().includes(q),
        ) as T;
      }

      case "duplicate_entry": {
        const orig = mockStore.entries.find((e) => e.uuid === args?.uuid);
        if (orig) {
          const clone = { ...orig, uuid: `mock-${Date.now()}`, title: `${orig.title} (copy)` };
          mockStore.entries.push(clone);
          return clone as T;
        }
        return null as T;
      }

      case "get_groups":
        return [
          { uuid: "root", name: "Root", icon: 0, children: [], entries: [], isExpanded: true, created: "", modified: "" },
        ] as T;

      case "generate_password":
        return "M0ck-P4ssw0rd-G3n3r4t3d!" as T;

      case "generate_passphrase":
        return "correct-horse-battery-staple" as T;

      case "evaluate_strength":
        return {
          score: 4,
          label: "Strong",
          color: "#2563EB",
          feedback: "Good password!",
          crackTimeSeconds: 315360000,
          crackTimeDisplay: "10 years",
        } as T;

      case "generate_totp_code":
        return { code: "123456", secondsRemaining: 25 } as T;

      case "generate_totp_secret":
        return "JBSWY3DPEHPK3PXP" as T;

      case "parse_ssh_key": {
        if (String(args?.content ?? "").includes("ENCRYPTED") && !args?.passphrase)
          throw new Error("PASSPHRASE_REQUIRED");
        return {
          privateKey: "-----BEGIN OPENSSH PRIVATE KEY-----\nmock\n-----END OPENSSH PRIVATE KEY-----",
          publicKey: "ssh-ed25519 AAAAC3mock dev@mock",
          fingerprint: "SHA256:mockfingerprint",
          algorithm: "ssh-ed25519",
          comment: "dev@mock",
        } as T;
      }

      case "generate_ssh_key":
        return {
          privateKey: "-----BEGIN OPENSSH PRIVATE KEY-----\nmock\n-----END OPENSSH PRIVATE KEY-----",
          publicKey: `ssh-ed25519 AAAAC3mock ${args?.comment ?? "mypass"}`,
          fingerprint: "SHA256:mockfingerprint",
          algorithm: "ssh-ed25519",
          comment: String(args?.comment ?? "mypass"),
        } as T;

      case "get_browser_status":
        return {
          connected: false,
          browsers: [],
          extensionVersion: "1.0.0",
          protocolVersion: "2.0",
        } as T;

      case "is_browser_integration_enabled":
        return mockStore.browserIntegrationEnabled as T;

      case "toggle_browser_integration":
        mockStore.browserIntegrationEnabled = Boolean(args?.enabled);
        return mockStore.browserIntegrationEnabled as T;

      case "is_ssh_agent_enabled":
        return mockStore.sshAgentEnabled as T;

      case "toggle_ssh_agent":
        mockStore.sshAgentEnabled = Boolean(args?.enabled);
        return mockStore.sshAgentEnabled as T;

      case "get_ssh_agent_status":
        return {
          enabled: mockStore.sshAgentEnabled,
          listening: mockStore.sshAgentEnabled,
          serviceRunning: false,
        } as T;

      case "respond_ssh_sign":
      case "disable_windows_ssh_agent_service":
        return undefined as T;

      case "list_passkeys":
        return [
          { entry_uuid: "pk-1", credential_id: "cred-mock-1", relying_party: "github.com", username: "dev", created: "2024-03-15", counter: 42 },
          { entry_uuid: "pk-2", credential_id: "cred-mock-2", relying_party: "google.com", username: "user@gmail.com", created: "2024-02-10", counter: 127 },
        ] as T;

      case "import_csv":
      case "import_google":
      case "import_apple": {
        const content = (args?.content as string) ?? "";
        const lines = content.split("\n").filter((l) => l.trim());
        if (lines.length < 2) {
          return { imported: 0, skipped: 0, duplicates: 0, errors: ["Fichier vide"] } as T;
        }
        // Skip header
        const dataLines = lines.slice(1);
        let imported = 0;
        for (const line of dataLines) {
          const cols = parseCsvLine(line);
          if (cols.length < 4) continue;
          const title = cols[0]?.trim() || "Untitled";
          const url = cols[1]?.trim() || "";
          const username = cols[2]?.trim() || "";
          const password = cols[3]?.trim() || "";
          if (!title && !username) continue;
          mockStore.entries.push({
            uuid: `mock-import-${Date.now()}-${imported}`,
            group: "root",
            title,
            username,
            url,
            password,
            notes: cols[4]?.trim() ?? "",
            icon: 0,
            tags: [],
            customFields: {},
            created: new Date().toISOString(),
            modified: new Date().toISOString(),
          });
          imported++;
        }
        return { imported, skipped: 0, duplicates: 0, errors: [] } as T;
      }

      case "import_1password":
      case "import_bitwarden":
      case "import_protonpass": {
        const content = (args?.content as string) ?? "";
        if (!content.trim()) {
          return { imported: 0, skipped: 0, duplicates: 0, errors: ["Fichier vide"] } as T;
        }
        try {
          const data = JSON.parse(content);
          const items: Array<Record<string, unknown>> =
            data?.items ?? [];
          let imported = 0;
          for (const item of items) {
            const title = (item.name as string) ?? (item.title as string) ?? "Untitled";
            const login = (item.login as Record<string, unknown>) ?? {};
            const username = (login.username as string) ?? (item.username as string) ?? "";
            const password = (login.password as string) ?? (item.password as string) ?? "";
            const uris = login.uris as Array<{ uri?: string }> | undefined;
            const url = uris?.[0]?.uri ?? (item.url as string) ?? "";
            mockStore.entries.push({
              uuid: `mock-import-${Date.now()}-${imported}`,
              group: "root",
              title: String(title),
              username: String(username),
              url: String(url),
              password: String(password),
              notes: "",
              icon: 0,
              tags: [],
              customFields: {},
              created: new Date().toISOString(),
              modified: new Date().toISOString(),
            });
            imported++;
          }
          return { imported, skipped: 0, duplicates: 0, errors: [] } as T;
        } catch {
          return { imported: 0, skipped: 0, duplicates: 0, errors: ["JSON invalide"] } as T;
        }
      }

      case "preview_csv_import": {
        const content = (args?.content as string) ?? "";
        const lines = content.split("\n").filter((l) => l.trim());
        if (lines.length < 2) return [] as T;
        const dataLines = lines.slice(1);
        const preview = [];
        for (const line of dataLines.slice(0, 20)) {
          const cols = parseCsvLine(line);
          if (cols.length < 4) continue;
          preview.push({
            title: cols[0]?.trim() || "Untitled",
            username: cols[2]?.trim() || "",
            url: cols[1]?.trim() || "",
            group: "Imported",
            hasPassword: !!(cols[3]?.trim()),
            hasTotp: false,
            isDuplicate: false,
          });
        }
        return preview as T;
      }

      case "import_entries": {
        const entries = (args?.entries as Array<Record<string, unknown>>) ?? [];
        let imported = 0;
        for (const entry of entries) {
          mockStore.entries.push({
            uuid: `mock-import-${Date.now()}-${imported}`,
            group: "root",
            title: (entry.title as string) ?? "Untitled",
            username: (entry.username as string) ?? "",
            url: (entry.url as string) ?? "",
            password: (entry.password as string) ?? "",
            notes: (entry.notes as string) ?? "",
            icon: 0,
            tags: (entry.tags as string[]) ?? [],
            customFields: (entry.customFields as Record<string, string>) ?? {},
            created: new Date().toISOString(),
            modified: new Date().toISOString(),
          });
          imported++;
        }
        return { imported, skipped: 0, duplicates: 0, errors: [] } as T;
      }

      case "get_entries_for_dedup":
        return mockStore.entries.map((e) => ({
          uuid: e.uuid,
          title: e.title,
          username: e.username,
          password: e.password,
          url: e.url,
          notes: e.notes ?? "",
          tags: e.tags ?? [],
          customFields: e.customFields ?? {},
          created: e.created ?? "",
          modified: e.modified ?? "",
        })) as T;

      case "import_passkeys":
        return { imported: 0, skipped: 0, duplicates: 0, errors: [] } as T;

      case "get_sync_config":
        return { serverUrl: "", enabled: false, hasToken: false } as T;
      case "set_sync_config":
        return null as T;
      case "get_sync_status":
      case "sync_now":
        return { state: "not_configured", detail: null, lastSync: null, serverVersion: null } as T;

      default:
        console.warn(`[MyPass Mock] Unknown command: ${cmd}`);
        return null as T;
    }
  };
}

/** Simple CSV line parser that handles quoted fields. */
function parseCsvLine(line: string): string[] {
  const result: string[] = [];
  let current = "";
  let inQuotes = false;
  for (const ch of line) {
    if (ch === '"') {
      inQuotes = !inQuotes;
    } else if (ch === "," && !inQuotes) {
      result.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  result.push(current);
  return result;
}

// =============================================================================
// Public API
// =============================================================================

export async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = await getInvoke();
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    // Tauri rejects with the command's raw error (a string): give every
    // caller a real Error so `.message` works on desktop as in web mode.
    throw e instanceof Error ? e : new Error(String(e));
  }
}
