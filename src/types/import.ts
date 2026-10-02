export type ImportFormat =
  | "csv"
  | "1pux"
  | "opvault"
  | "bitwarden"
  | "protonpass"
  | "google"
  | "apple"
  | "keepass1";

export interface ImportPreview {
  entries: ImportPreviewEntry[];
  groups: string[];
  totalCount: number;
  duplicateCount: number;
}

export interface ImportPreviewEntry {
  title: string;
  username: string;
  url: string;
  group: string;
  hasPassword: boolean;
  hasTotp: boolean;
  isDuplicate: boolean;
}

export interface ImportResult {
  imported: number;
  skipped: number;
  duplicates: number;
  errors: string[];
}

export interface ColumnMapping {
  title: number;
  username: number;
  password: number;
  url: number;
  notes: number;
  totp: number;
  group: number;
  delimiter: string;
  hasHeader: boolean;
}

export type ExportFormat = "csv" | "json" | "xml" | "html";

// ── Deduplication types ──────────────────────────────────────────

/** A parsed entry from the import file, before any dedup. */
export interface ParsedEntry {
  tempId: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
}

/** One side of a duplicate comparison. */
export interface DuplicateEntry {
  source: "import" | "vault";
  uuid?: string;
  tempId?: string;
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
  created: string;
  modified: string;
}

/** A group of entries that match on the same key. */
export interface DuplicateGroup {
  id: string;
  matchKey: string;
  matchType: "url+username" | "url" | "username";
  entries: DuplicateEntry[];
  selectedIndex: number;
}

/** An entry resolved after dedup, ready for import. */
export interface ResolvedEntry {
  title: string;
  username: string;
  password: string;
  url: string;
  notes: string;
  tags: string[];
  customFields: Record<string, string>;
  totp: string;
}

/** Result of the dedup resolution. */
export interface DedupResolution {
  groups: DuplicateGroup[];
  resolvedEntries: ResolvedEntry[];
  totalKept: number;
  totalDiscarded: number;
}
