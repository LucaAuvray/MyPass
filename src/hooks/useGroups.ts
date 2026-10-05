import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { tauriCommand } from "@/lib/tauri";
import type { Group } from "@/types/group";

/** Folder tree (`root` = the vault's root folder) and the folder mutations. */
export function useGroups() {
  const queryClient = useQueryClient();
  const groups = useQuery({
    queryKey: ["groups"],
    queryFn: () => tauriCommand<Group[]>("get_groups"),
    staleTime: 10_000,
  });

  // Folder changes move entries around and change counts: refresh both.
  const onSuccess = () => {
    queryClient.invalidateQueries({ queryKey: ["groups"] });
    queryClient.invalidateQueries({ queryKey: ["entries"] });
  };

  const create = useMutation({
    mutationFn: (p: { name: string; parentUuid?: string }) =>
      tauriCommand<Group>("create_group", p),
    onSuccess,
  });
  const rename = useMutation({
    mutationFn: (p: { uuid: string; name: string }) => tauriCommand<Group>("update_group", p),
    onSuccess,
  });
  const remove = useMutation({
    mutationFn: (uuid: string) => tauriCommand<void>("delete_group", { uuid }),
    onSuccess,
  });
  const move = useMutation({
    mutationFn: (p: { entryUuid: string; groupUuid: string }) =>
      tauriCommand<void>("move_entry", p),
    onSuccess,
  });

  return {
    root: groups.data?.[0],
    createGroup: create.mutateAsync,
    renameGroup: rename.mutateAsync,
    deleteGroup: remove.mutateAsync,
    moveEntry: move.mutateAsync,
  };
}
