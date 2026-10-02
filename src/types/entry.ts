export interface Entry {
  uuid: string;
  group: string;
  title: string;
  url: string;
  username: string;
  password: string;
  notes: string;
  icon: number | string;
  tags: string[];
  totp?: TotpConfig;
  passkey?: PasskeyData;
  customFields: Record<string, string>;
  attachments: Attachment[];
  expiry?: string;
  created: string;
  modified: string;
  history: EntryHistory[];
  autoType: AutoTypeConfig;
  browserSettings: BrowserSettings;
}

export interface TotpConfig {
  secret: string;
  algorithm: "SHA1" | "SHA256" | "SHA512";
  digits: 6 | 8;
  period: number;
  url: string;
}

export interface PasskeyData {
  credentialId: string;
  relyingParty: string;
  userId: string;
  publicKey: string;
  counter: number;
  created: string;
}

export interface Attachment {
  uuid: string;
  name: string;
  mimeType: string;
  size: number;
  data: Uint8Array;
}

export interface EntryHistory {
  timestamp: string;
  entry: Omit<Entry, "history">;
}

export interface AutoTypeConfig {
  enabled: boolean;
  sequence: string;
  window: string;
}

export interface BrowserSettings {
  matchUrl: string;
  matchPattern: "exact" | "hostname" | "startswith" | "regex";
  excludeFromBrowser: boolean;
}
