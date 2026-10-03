/** One entry as it travels (MyPass JSON file, IPC, wasm) — mirrors mypass-core's ImportedEntry. */
export interface ImportedEntry {
  /** Folder path from the root, segments joined by "/"; "" is the root. */
  group: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  /** otpauth:// URI, or "". */
  totp: string;
  customFields: Record<string, string>;
}

export interface ImportResult {
  imported: number;
  updated: number;
  skipped: number;
}

export type ExportFormat = "json" | "csv";

// ── Deduplication types ──────────────────────────────────────────

/** A parsed entry from the import file, before any dedup. */
export type ParsedEntry = ImportedEntry & { tempId: string };

/** One side of a duplicate comparison. */
export interface DuplicateEntry {
  source: "import" | "vault";
  uuid?: string;
  tempId?: string;
  group: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
  /** The vault side never carries the secret itself, only whether it has one. */
  hasTotp: boolean;
  created: string;
  modified: string;
}

/** A group of entries that match on the same key. */
export interface DuplicateGroup {
  id: string;
  matchKey: string;
  matchType: "url+username" | "url" | "username" | "title";
  entries: DuplicateEntry[];
  selectedIndex: number;
}

/** An entry resolved after dedup, ready for import_entries. */
export type ResolvedEntry = ImportedEntry & {
  /** The vault entry this one overwrites in place. */
  replaceUuid?: string;
};

/** Result of the dedup resolution. */
export interface DedupResolution {
  groups: DuplicateGroup[];
  resolvedEntries: ResolvedEntry[];
  totalKept: number;
  totalDiscarded: number;
}
