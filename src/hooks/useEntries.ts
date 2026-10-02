import { useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { tauriCommand } from "@/lib/tauri";
import { useEntriesStore, type Entry } from "@/stores/entriesStore";

const ENTRIES_KEY = ["entries"] as const;

export function useEntries(groupUuid?: string) {
  const queryClient = useQueryClient();
  const setEntries = useEntriesStore((s) => s.setEntries);

  const entries = useQuery({
    queryKey: [...ENTRIES_KEY, groupUuid ?? "all"],
    queryFn: () =>
      tauriCommand<Entry[]>("get_entries", groupUuid ? { groupUuid } : undefined),
    staleTime: 10_000,
  });

  // Bridge TanStack Query results into Zustand store
  useEffect(() => {
    if (entries.data) {
      setEntries(entries.data);
    }
  }, [entries.data, setEntries]);

  const createMutation = useMutation({
    mutationFn: (params: {
      groupUuid?: string;
      title: string;
      username: string;
      password: string;
      url?: string;
      notes?: string;
      tags?: string[];
      customFields?: Record<string, string>;
    }) => tauriCommand<Entry>("create_entry", { entry: params }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ENTRIES_KEY });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (params: { uuid: string; update: Partial<Entry> }) =>
      tauriCommand<Entry>("update_entry", params),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ENTRIES_KEY });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (uuid: string) => tauriCommand<void>("delete_entry", { uuid }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ENTRIES_KEY });
    },
  });

  return {
    entries,
    createEntry: createMutation.mutateAsync,
    updateEntry: updateMutation.mutateAsync,
    deleteEntry: deleteMutation.mutateAsync,
    isCreating: createMutation.isPending,
    isUpdating: updateMutation.isPending,
    isDeleting: deleteMutation.isPending,
  };
}
