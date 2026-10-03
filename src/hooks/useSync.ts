/** Statut de synchronisation + déclenchement partagés (desktop et web). */
import { useQuery, type QueryClient } from "@tanstack/react-query";
import { tauriCommand } from "@/lib/tauri";

export type SyncStatus = {
  state: string;
  detail: string | null;
  lastSync: string | null;
  serverVersion: string | null;
};

export function useSyncStatus() {
  return useQuery({
    queryKey: ["sync-status"],
    queryFn: () => tauriCommand<SyncStatus>("get_sync_status"),
    refetchInterval: 5000,
  });
}

/** sync_now + invalidation des listes : un pull peut ramener du neuf. */
export async function syncNowAndRefresh(qc: QueryClient): Promise<SyncStatus> {
  const status = await tauriCommand<SyncStatus>("sync_now");
  await qc.invalidateQueries({ queryKey: ["entries"] });
  await qc.invalidateQueries({ queryKey: ["groups"] });
  return status;
}
