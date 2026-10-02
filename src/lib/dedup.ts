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

export type MatchType = "url+username" | "url" | "username" | "title";

export function makeMatchKey(
  url: string,
  username: string,
  title: string,
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
  // Cards, identities, documents, SSH keys: no URL nor username, the title is all there is.
  const normTitle = title.trim().toLowerCase();
  if (normTitle) {
    return { key: `__title__::${normTitle}`, matchType: "title" };
  }
  return { key: "", matchType: "url+username" };
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
    group: "",
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
    group: e.group,
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
    const { key, matchType } = makeMatchKey(e.url, e.username, e.title);
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
    const { key, matchType } = makeMatchKey(e.url, e.username, e.title);
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
    // Only what this import could duplicate; twins already in the vault are not its business.
    if (allEntries.length < 2 || bucket.import.length === 0) continue;

    // Build match key display
    const representative = allEntries[0];
    const urlPart = normalizeUrl(representative.url) || "?";
    const userPart = representative.username || "?";
    const matchKey = bucket.matchType === "title" ? representative.title : `${urlPart} — ${userPart}`;

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

/** The fields import_entries writes, taken from either side of a comparison. */
function toResolved(e: ParsedEntry | DuplicateEntry, replaceUuid?: string): ResolvedEntry {
  return {
    group: e.group,
    title: e.title,
    username: e.username,
    password: e.password,
    url: e.url,
    notes: e.notes,
    tags: e.tags,
    customFields: e.customFields,
    totp: e.totp,
    ...(replaceUuid ? { replaceUuid } : {}),
  };
}

/**
 * Given resolved duplicate groups and non-duplicate entries, produce what to
 * send to import_entries. One entry stays per group: keeping the vault's
 * sends nothing; keeping an imported one overwrites the group's first vault
 * entry in place (replaceUuid), or adds it when the group has none.
 */
export function resolveDuplicates(
  groups: DuplicateGroup[],
  nonDuplicates: ParsedEntry[],
): DedupResolution {
  const resolvedEntries: ResolvedEntry[] = nonDuplicates.map((e) => toResolved(e));
  let totalKept = nonDuplicates.length;
  let totalDiscarded = 0;

  for (const group of groups) {
    const selected = group.entries[group.selectedIndex];
    if (!selected) continue;
    totalKept++;
    totalDiscarded += group.entries.length - 1;
    if (selected.source === "vault") continue;
    const vaultTwin = group.entries.find((e) => e.source === "vault");
    resolvedEntries.push(toResolved(selected, vaultTwin?.uuid));
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
