import { create } from "zustand";

interface DatabaseMeta {
  name: string;
  description: string;
  kdf: "argon2id" | "argon2d";
  encryption: "aes256" | "chacha20" | "twofish";
  compression: "gzip" | "none";
  version: "4.0" | "4.1";
  created: string;
  modified: string;
}

interface DatabaseState {
  isOpen: boolean;
  filePath: string | null;
  meta: DatabaseMeta | null;
  isLoading: boolean;
  error: string | null;

  open: (path: string, meta: DatabaseMeta) => void;
  close: () => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
}

export const useDatabaseStore = create<DatabaseState>((set) => ({
  isOpen: false,
  filePath: null,
  meta: null,
  isLoading: false,
  error: null,

  open: (path, meta) => set({ isOpen: true, filePath: path, meta, error: null }),
  close: () => set({ isOpen: false, filePath: null, meta: null, error: null }),
  setLoading: (loading) => set({ isLoading: loading }),
  setError: (error) => set({ error }),
}));
