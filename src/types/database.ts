export interface DatabaseMeta {
  name: string;
  description: string;
  kdf: "argon2id" | "argon2d";
  encryption: "aes256" | "chacha20" | "twofish";
  compression: "gzip" | "none";
  version: "4.0" | "4.1";
  created: string;
  modified: string;
  recycleBinEnabled: boolean;
  historyMaxItems: number;
  historyMaxSize: number;
}

/** Where the desktop keeps this PC's single vault (decided by Rust). */
export interface VaultLocation {
  path: string;
  exists: boolean;
}

export interface DatabaseInfo {
  filePath: string;
  meta: DatabaseMeta;
  groups: number;
  entries: number;
}
