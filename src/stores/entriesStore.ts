import { create } from "zustand";
import type { ItemKind } from "@/lib/items";

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
  customFields: Record<string, string>;
  created: string;
  modified: string;
}

interface EntriesState {
  entries: Entry[];
  selectedEntryId: string | null;
  searchQuery: string;
  kindFilter: ItemKind | null;

  setEntries: (entries: Entry[]) => void;
  addEntry: (entry: Entry) => void;
  updateEntry: (uuid: string, entry: Partial<Entry>) => void;
  removeEntry: (uuid: string) => void;
  selectEntry: (uuid: string | null) => void;
  setSearchQuery: (query: string) => void;
  setKindFilter: (kind: ItemKind | null) => void;
}

export const useEntriesStore = create<EntriesState>((set) => ({
  entries: [],
  selectedEntryId: null,
  searchQuery: "",
  kindFilter: null,

  setEntries: (entries) => set({ entries }),
  addEntry: (entry) => set((s) => ({ entries: [...s.entries, entry] })),
  updateEntry: (uuid, update) =>
    set((s) => ({
      entries: s.entries.map((e) => (e.uuid === uuid ? { ...e, ...update } : e)),
    })),
  removeEntry: (uuid) =>
    set((s) => ({ entries: s.entries.filter((e) => e.uuid !== uuid) })),
  selectEntry: (uuid) => set({ selectedEntryId: uuid }),
  setSearchQuery: (query) => set({ searchQuery: query }),
  setKindFilter: (kind) => set({ kindFilter: kind, selectedEntryId: null }),
}));
