/**
 * In-memory fake backend for `npm run dev` in a plain browser. Only
 * `getInvoke()` in ./tauri loads it, behind `import.meta.env.DEV`, so it is
 * never part of a production build (.msi or PWA).
 */
import type { Entry } from "@/stores/entriesStore";
import type { Group } from "@/types/group";
import type { TauriInvokeFn } from "./tauri";
import { findGroup } from "./groups";

const folder = (uuid: string, name: string, children: Group[] = []): Group => ({
  uuid, name, icon: null, children, entryCount: 0, isExpanded: true, created: null, modified: null,
});

/** A copy of the tree with entryCount recomputed from the mock entries. */
function countedTree(g: Group): Group {
  const children = g.children.map(countedTree);
  const own = mockStore.entries.filter((e) => e.group_uuid === g.uuid).length;
  return { ...g, children, entryCount: own + children.reduce((n, c) => n + c.entryCount, 0) };
}

function parentOf(g: Group, uuid: string): Group | null {
  if (g.children.some((c) => c.uuid === uuid)) return g;
  for (const c of g.children) {
    const p = parentOf(c, uuid);
    if (p) return p;
  }
  return null;
}

function mockName(name: unknown): string {
  const n = String(name ?? "").trim();
  if (!n) throw new Error("GROUP_NAME_REQUIRED");
  return n;
}

const mockStore = {
  isOpen: false,
  vaultExists: false,
  entries: [] as Entry[],
  root: folder("root", "Root", [folder("mock-g1", "Perso")]),
  browserIntegrationEnabled: false,
  sshAgentEnabled: false,
};

export function createMockInvoke(): TauriInvokeFn {
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
            { uuid: "mock-1", group_uuid: "mock-g1", title: "Google", username: "user@gmail.com", url: "https://google.com", password: "mock-pass-1", notes: "", icon: 0, tags: ["email"], customFields: {}, totp: "otpauth://totp/Google?secret=JBSWY3DPEHPK3PXP", created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-2", group_uuid: "mock-g1", title: "GitHub", username: "dev", url: "https://github.com", password: "mock-pass-2", notes: "Code repository", icon: 0, tags: ["dev"], customFields: {}, totp: "", created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-3", group_uuid: "root", title: "Twitter", username: "@handle", url: "https://twitter.com", password: "mock-pass-3", notes: "", icon: 0, tags: ["social"], customFields: {}, totp: "", created: new Date().toISOString(), modified: new Date().toISOString() },
            { uuid: "mock-4", group_uuid: "root", title: "Amazon", username: "user@example.com", url: "https://amazon.com", password: "weak", notes: "Shopping", icon: 0, tags: ["shopping"], customFields: {}, totp: "", created: "2024-01-15T00:00:00Z", modified: "2024-01-15T00:00:00Z" },
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
          group_uuid: (entry?.groupUuid as string) ?? "root",
          title: (entry?.title as string) ?? "New Entry",
          username: (entry?.username as string) ?? "",
          url: (entry?.url as string) ?? "",
          password: (entry?.password as string) ?? "",
          notes: (entry?.notes as string) ?? "",
          icon: 0,
          tags: (entry?.tags as string[]) ?? [],
          customFields: (entry?.customFields as Record<string, string>) ?? {},
          totp: (entry?.totp as string) ?? "",
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
        return [countedTree(mockStore.root)] as T;

      case "create_group": {
        const parent = findGroup(mockStore.root, (args?.parentUuid as string) ?? "root");
        if (!parent) throw new Error("Group not found");
        const created = folder(`mock-g-${Date.now()}`, mockName(args?.name));
        parent.children.push(created);
        return created as T;
      }

      case "update_group": {
        const group = findGroup(mockStore.root, String(args?.uuid));
        if (!group) throw new Error("Group not found");
        if (args?.name !== undefined) group.name = mockName(args.name);
        return group as T;
      }

      case "delete_group": {
        // Like the core: entries and subfolders move up to the parent.
        const uuid = String(args?.uuid);
        const parent = parentOf(mockStore.root, uuid);
        if (!parent) throw new Error("Group not found");
        const [gone] = parent.children.splice(parent.children.findIndex((c) => c.uuid === uuid), 1);
        parent.children.push(...gone.children);
        for (const e of mockStore.entries) if (e.group_uuid === uuid) e.group_uuid = parent.uuid;
        return undefined as T;
      }

      case "move_entry": {
        const entry = mockStore.entries.find((e) => e.uuid === args?.entryUuid);
        if (entry) entry.group_uuid = String(args?.groupUuid);
        return undefined as T;
      }

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

      case "get_totp_code": {
        const s = Math.floor(Date.now() / 1000);
        return { code: "123456", period: 30, secondsRemaining: 30 - (s % 30) } as T;
      }

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

      case "parse_import":
        return [
          { group: "Perso", title: "Mock import", username: "mock", password: "mock-pw", url: "https://mock.example", notes: "", tags: [], totp: "", customFields: {} },
          { group: "", title: "GitHub", username: "dev", password: "mock-pw-2", url: "https://github.com", notes: "", tags: [], totp: "", customFields: {} },
        ] as T;

      case "import_entries": {
        const entries = (args?.entries as Array<Record<string, unknown>>) ?? [];
        let imported = 0;
        for (const entry of entries) {
          mockStore.entries.push({
            uuid: `mock-import-${Date.now()}-${imported}`,
            group_uuid: "root",
            title: (entry.title as string) ?? "Untitled",
            username: (entry.username as string) ?? "",
            url: (entry.url as string) ?? "",
            password: (entry.password as string) ?? "",
            notes: (entry.notes as string) ?? "",
            icon: 0,
            tags: (entry.tags as string[]) ?? [],
            customFields: (entry.customFields as Record<string, string>) ?? {},
            totp: (entry.totp as string) ?? "",
            created: new Date().toISOString(),
            modified: new Date().toISOString(),
          });
          imported++;
        }
        return { imported, updated: 0, skipped: 0 } as T;
      }

      case "export_entries":
        return { saved: true } as T;

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
