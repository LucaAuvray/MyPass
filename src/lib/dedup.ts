/**
 * Deduplication logic for password imports.
 * Detects duplicate entries by matching URL + username (with fallback).
 */
import type {
  ParsedEntry,
  DuplicateEntry,
  DuplicateGroup,
  ResolvedEntry,
  DedupResolution,
  ImportFormat,
} from "@/types/import";

// ── URL normalization ────────────────────────────────────────────

/** Extracts a normalized hostname from a URL string. */
export function normalizeUrl(url: string): string {
  if (!url) return "";
  let cleaned = url.trim().toLowerCase();
  // Remove protocol
  cleaned = cleaned.replace(/^https?:\/\//, "");
  // Remove path/query/hash
  const slashIdx = cleaned.indexOf("/");
  if (slashIdx >= 0) cleaned = cleaned.substring(0, slashIdx);
  // Remove port
  const colonIdx = cleaned.lastIndexOf(":");
  if (colonIdx >= 0 && /^\d+$/.test(cleaned.substring(colonIdx + 1))) {
    cleaned = cleaned.substring(0, colonIdx);
  }
  // Remove www. prefix
  if (cleaned.startsWith("www.")) cleaned = cleaned.substring(4);
  return cleaned;
}

// ── Match key ────────────────────────────────────────────────────

export type MatchType = "url+username" | "url" | "username";

export function makeMatchKey(
  url: string,
  username: string,
): { key: string; matchType: MatchType } {
  const normUrl = normalizeUrl(url);
  const normUser = username.trim().toLowerCase();

  if (normUrl && normUser) {
    return { key: `${normUrl}::${normUser}`, matchType: "url+username" };
  }
  if (normUrl) {
    return { key: `${normUrl}::__empty__`, matchType: "url" };
  }
  if (normUser) {
    return { key: `__empty__::${normUser}`, matchType: "username" };
  }
  // Both empty — not a match
  return { key: "", matchType: "url+username" };
}

// ── CSV parsing ──────────────────────────────────────────────────

/** Simple CSV line parser that handles quoted fields. */
function parseCsvLine(line: string): string[] {
  const cols: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQuotes) {
      if (ch === '"') {
        if (i + 1 < line.length && line[i + 1] === '"') {
          current += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        current += ch;
      }
    } else if (ch === '"') {
      inQuotes = true;
    } else if (ch === ",") {
      cols.push(current.trim());
      current = "";
    } else {
      current += ch;
    }
  }
  cols.push(current.trim());
  return cols;
}

/** Parse CSV content into ParsedEntry[]. Google format: name,url,username,password[,note]. */
export function parseCsvToEntries(
  content: string,
  _format: ImportFormat,
): ParsedEntry[] {
  if (!content || typeof content !== "string") {
    console.error("[dedup] parseCsvToEntries: content is null or not a string", content);
    return [];
  }
  const lines = content.split("\n").filter((l) => l.trim());
  if (lines.length < 2) return [];

  // Skip header
  const dataLines = lines.slice(1);
  const entries: ParsedEntry[] = [];

  for (let i = 0; i < dataLines.length; i++) {
    const cols = parseCsvLine(dataLines[i]);
    if (cols.length < 4) continue;

    // Google/Apple/CSV format: title=0, url=1, username=2, password=3, notes=4
    const title = cols[0]?.trim() || "Untitled";
    const url = cols[1]?.trim() || "";
    const username = cols[2]?.trim() || "";
    const password = cols[3]?.trim() || "";
    const notes = cols[4]?.trim() || "";

    if (!title && !username) continue;

    entries.push({
      tempId: `import-${i}`,
      title,
      username,
      password,
      url,
      notes,
      tags: [],
      customFields: {},
      totp: "",
    });
  }

  return entries;
}

/** Parse JSON content (1Password, Bitwarden, Proton Pass) into ParsedEntry[]. */
export function parseJsonToEntries(
  content: string,
  format: ImportFormat,
): ParsedEntry[] {
  const entries: ParsedEntry[] = [];

  try {
    const data = JSON.parse(content);
    let items: Array<Record<string, unknown>> = [];

    if (format === "bitwarden") {
      items = (data?.items as Array<Record<string, unknown>>) ?? [];
    } else if (format === "protonpass") {
      const vaults = data?.vaults as Record<string, Record<string, unknown>> | undefined;
      if (vaults) {
        for (const [, vault] of Object.entries(vaults)) {
          const vaultItems = (vault?.items as Array<Record<string, unknown>>) ?? [];
          items.push(...vaultItems);
        }
      }
    } else if (format === "1pux") {
      items = (data?.items as Array<Record<string, unknown>>) ?? [];
    }

    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      let title = "", username = "", password = "", url = "", notes = "";

      if (format === "bitwarden") {
        title = (item?.name as string) ?? (item?.title as string) ?? "Untitled";
        const login = (item?.login as Record<string, unknown>) ?? {};
        username = (login?.username as string) ?? (item?.username as string) ?? "";
        password = (login?.password as string) ?? (item?.password as string) ?? "";
        const uris = (login?.uris as Array<{ uri?: string }>) ?? [];
        url = uris[0]?.uri ?? (item?.url as string) ?? "";
        notes = (login?.notes as string) ?? (item?.notes as string) ?? "";
      } else if (format === "protonpass") {
        const itemData = (item?.data as Record<string, unknown>) ?? {};
        const metadata = (itemData?.metadata as Record<string, unknown>) ?? {};
        const content_ = (itemData?.content as Record<string, unknown>) ?? {};
        title = (metadata?.name as string) ?? "Untitled";
        username = (content_?.username as string) ?? "";
        password = (content_?.password as string) ?? "";
        const urls = (content_?.urls as string[]) ?? [];
        url = urls[0] ?? "";
        notes = (content_?.note as string) ?? "";
      } else {
        // 1Password
        title = (item?.title as string) ?? "Untitled";
        username = (item?.username as string) ?? "";
        password = (item?.password as string) ?? "";
        url = (item?.url as string) ?? "";
        notes = (item?.notes as string) ?? "";
      }

      entries.push({
        tempId: `import-${i}`,
        title,
        username,
        password,
        url,
        notes,
        tags: [],
        customFields: {},
        totp: "",
      });
    }
  } catch {
    // JSON parse error — return empty
  }

  return entries;
}

// ── Duplicate detection ──────────────────────────────────────────

/** Build a duplicate entry from a vault entry (for comparison). */
export function vaultEntryToDupEntry(entry: {
  uuid: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes?: string;
  tags?: string[];
  customFields?: Record<string, string>;
  created?: string;
  modified?: string;
}): DuplicateEntry {
  return {
    source: "vault",
    uuid: entry.uuid,
    title: entry.title,
    username: entry.username,
    password: entry.password,
    url: entry.url,
    notes: entry.notes ?? "",
    tags: entry.tags ?? [],
    customFields: entry.customFields ?? {},
    totp: "",
    created: entry.created ?? "",
    modified: entry.modified ?? "",
  };
}

/** Build a duplicate entry from a parsed import entry. */
export function parsedEntryToDupEntry(e: ParsedEntry): DuplicateEntry {
  return {
    source: "import",
    tempId: e.tempId,
    title: e.title,
    username: e.username,
    password: e.password,
    url: e.url,
    notes: e.notes,
    tags: e.tags,
    customFields: e.customFields,
    totp: e.totp,
    created: "",
    modified: "",
  };
}

/** Compute duplicate groups from import entries and existing vault entries. */
export function computeDuplicateGroups(
  importEntries: ParsedEntry[],
  vaultEntries: DuplicateEntry[],
): DuplicateGroup[] {
  const map = new Map<string, { import: DuplicateEntry[]; vault: DuplicateEntry[]; matchType: MatchType }>();

  // Index import entries
  for (const e of importEntries) {
    const { key, matchType } = makeMatchKey(e.url, e.username);
    if (!key) continue;
    let bucket = map.get(key);
    if (!bucket) {
      bucket = { import: [], vault: [], matchType };
      map.set(key, bucket);
    }
    bucket.import.push(parsedEntryToDupEntry(e));
  }

  // Index vault entries
  for (const e of vaultEntries) {
    const { key, matchType } = makeMatchKey(e.url, e.username);
    if (!key) continue;
    let bucket = map.get(key);
    if (!bucket) {
      bucket = { import: [], vault: [], matchType };
      map.set(key, bucket);
    }
    bucket.vault.push(e);
  }

  // Build groups where total >= 2
  const groups: DuplicateGroup[] = [];
  let groupIdx = 0;

  for (const [, bucket] of map) {
    const allEntries = [...bucket.import, ...bucket.vault];
    if (allEntries.length < 2) continue;

    // Build match key display
    const representative = allEntries[0];
    const urlPart = normalizeUrl(representative.url) || "?";
    const userPart = representative.username || "?";
    const matchKey = `${urlPart} — ${userPart}`;

    groups.push({
      id: `dup-${groupIdx++}`,
      matchKey,
      matchType: bucket.matchType,
      entries: allEntries,
      // Pre-select the first import entry if available, otherwise first entry
      selectedIndex: 0,
    });
  }

  return groups;
}

// ── Resolution ───────────────────────────────────────────────────

/**
 * Given resolved duplicate groups and non-duplicate entries,
 * produce the final list of entries to import.
 */
export function resolveDuplicates(
  groups: DuplicateGroup[],
  nonDuplicates: ParsedEntry[],
): DedupResolution {
  const resolvedEntries: ResolvedEntry[] = [];

  // Add non-duplicate entries
  for (const e of nonDuplicates) {
    resolvedEntries.push({
      title: e.title,
      username: e.username,
      password: e.password,
      url: e.url,
      notes: e.notes,
      tags: e.tags,
      customFields: e.customFields,
      totp: e.totp,
    });
  }

  // Add selected entry from each duplicate group
  let totalKept = nonDuplicates.length;
  let totalDiscarded = 0;

  for (const group of groups) {
    const selected = group.entries[group.selectedIndex];
    if (selected) {
      resolvedEntries.push({
        title: selected.title,
        username: selected.username,
        password: selected.password,
        url: selected.url,
        notes: selected.notes,
        tags: selected.tags,
        customFields: selected.customFields,
        totp: selected.totp,
      });
      totalKept++;
      totalDiscarded += group.entries.length - 1;
    }
  }

  return { groups, resolvedEntries, totalKept, totalDiscarded };
}

/**
 * Find non-duplicate import entries (those not in any duplicate group).
 */
export function findNonDuplicates(
  importEntries: ParsedEntry[],
  groups: DuplicateGroup[],
): ParsedEntry[] {
  const dupTempIds = new Set<string>();
  for (const group of groups) {
    for (const entry of group.entries) {
      if (entry.tempId) dupTempIds.add(entry.tempId);
    }
  }
  return importEntries.filter((e) => !dupTempIds.has(e.tempId));
}
