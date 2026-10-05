import { create } from "zustand";
import type { ItemKind } from "@/lib/items";

export interface Entry {
  uuid: string;
  /** Uuid of the folder holding the entry (the root for unfiled entries). */
  group_uuid: string;
  title: string;
  url: string;
  username: string;
  password: string;
  notes: string;
  icon: number | string;
  tags: string[];
  customFields: Record<string, string>;
  /** 2FA `otpauth://` link, or "". */
  totp: string;
  created: string;
  modified: string;
}

interface EntriesState {
  entries: Entry[];
  selectedEntryId: string | null;
  searchQuery: string;
  kindFilter: ItemKind | null;
  /** Selected folder; exclusive with kindFilter. */
  groupFilter: string | null;

  setEntries: (entries: Entry[]) => void;
  addEntry: (entry: Entry) => void;
  updateEntry: (uuid: string, entry: Partial<Entry>) => void;
  removeEntry: (uuid: string) => void;
  selectEntry: (uuid: string | null) => void;
  setSearchQuery: (query: string) => void;
  setKindFilter: (kind: ItemKind | null) => void;
  setGroupFilter: (uuid: string | null) => void;
}

export const useEntriesStore = create<EntriesState>((set) => ({
  entries: [],
  selectedEntryId: null,
  searchQuery: "",
  kindFilter: null,
  groupFilter: null,

  setEntries: (entries) => set({ entries }),
  addEntry: (entry) => set((s) => ({ entries: [...s.entries, entry] })),
  updateEntry: (uuid, update) =>
    set((s) => ({
      entries: s.entries.map((e) => (e.uuid === uuid ? { ...e, ...update } : e)),
    })),
  removeEntry: (uuid) => set((s) => ({ entries: s.entries.filter((e) => e.uuid !== uuid) })),
  selectEntry: (uuid) => set({ selectedEntryId: uuid }),
  setSearchQuery: (query) => set({ searchQuery: query }),
  setKindFilter: (kind) => set({ kindFilter: kind, groupFilter: null, selectedEntryId: null }),
  setGroupFilter: (uuid) => set({ groupFilter: uuid, kindFilter: null, selectedEntryId: null }),
}));
