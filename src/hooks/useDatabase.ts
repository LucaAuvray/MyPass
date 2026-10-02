import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { tauriCommand } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import i18n from "@/i18n/config";
import type { DatabaseInfo } from "@/types/database";

const DB_INFO_KEY = ["database", "info"];

export function useDatabase() {
  const queryClient = useQueryClient();
  const lock = useAppStore((s) => s.lock);
  const unlock = useAppStore((s) => s.unlock);

  const isLocked = useAppStore((s) => s.isLocked);

  const dbInfo = useQuery({
    queryKey: DB_INFO_KEY,
    queryFn: () => tauriCommand<DatabaseInfo | null>("get_database_info"),
    enabled: !isLocked,
  });

  const openMutation = useMutation({
    mutationFn: (params: { path: string; password: string; keyfilePath?: string }) =>
      tauriCommand<DatabaseInfo>("open_database", params),
    onSuccess: (data) => {
      queryClient.setQueryData(DB_INFO_KEY, data);
      unlock();
    },
  });

  const createMutation = useMutation({
    mutationFn: (params: {
      path: string;
      password: string;
      name: string;
      encryption?: string;
      keyfilePath?: string;
    }) => tauriCommand<DatabaseInfo>("create_database", params),
    onSuccess: (data) => {
      queryClient.setQueryData(DB_INFO_KEY, data);
      unlock();
    },
  });

  const saveMutation = useMutation({
    mutationFn: () => tauriCommand<void>("save_database"),
  });

  const lockMutation = useMutation({
    mutationFn: () => tauriCommand<void>("lock_database"),
    onSuccess: () => {
      queryClient.clear();
      lock();
    },
    onError: () => {
      // lock_database rejette volontairement en cas de push en attente non
      // synchronisé (anti-perte) — sans feedback, l'utilisateur clique
      // « Verrouiller » et ne voit rien se passer.
      toast.error(i18n.t("sync.lockFailed"));
    },
  });

  return {
    dbInfo,
    openDatabase: openMutation.mutateAsync,
    createDatabase: createMutation.mutateAsync,
    saveDatabase: saveMutation.mutateAsync,
    lockDatabase: lockMutation.mutateAsync,
    isOpening: openMutation.isPending,
    isCreating: createMutation.isPending,
    isSaving: saveMutation.isPending,
    openError: openMutation.error?.message,
    createError: createMutation.error?.message,
  };
}
