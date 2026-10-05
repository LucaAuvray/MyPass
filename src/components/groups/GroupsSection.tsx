import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { Plus } from "lucide-react";
import { useGroups } from "@/hooks/useGroups";
import { useEntriesStore } from "@/stores/entriesStore";
import { GroupTree } from "./GroupTree";
import { GroupNameDialog } from "./GroupNameDialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import type { Group } from "@/types/group";

type NameDialog = { mode: "create"; parentUuid?: string } | { mode: "rename"; group: Group };

/** Sidebar "Folders" block: the tree, plus its create / rename / delete dialogs. */
export function GroupsSection({ onNavigate }: { onNavigate?: () => void }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { root, createGroup, renameGroup, deleteGroup } = useGroups();
  const groupFilter = useEntriesStore((s) => s.groupFilter);
  const setGroupFilter = useEntriesStore((s) => s.setGroupFilter);
  const [nameDialog, setNameDialog] = useState<NameDialog | null>(null);
  const [toDelete, setToDelete] = useState<Group | null>(null);

  const select = (uuid: string) => {
    navigate("/");
    setGroupFilter(uuid);
    onNavigate?.();
  };

  const confirmDelete = async () => {
    if (!toDelete) return;
    await deleteGroup(toDelete.uuid);
    if (groupFilter === toDelete.uuid) setGroupFilter(null);
    setToDelete(null);
  };

  const renaming = nameDialog?.mode === "rename" ? nameDialog.group : null;

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between px-2.5">
        <span className="text-muted-foreground text-xs font-semibold tracking-wide uppercase">
          {t("groups.title")}
        </span>
        <button
          onClick={() => setNameDialog({ mode: "create" })}
          className="text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground rounded p-0.5 transition-colors"
          title={t("groups.newGroup")}
          aria-label={t("groups.newGroup")}
        >
          <Plus className="size-3.5" />
        </button>
      </div>

      {root && (
        <GroupTree
          root={root}
          selectedUuid={groupFilter}
          onSelect={select}
          onCreate={(parentUuid) => setNameDialog({ mode: "create", parentUuid })}
          onRename={(group) => setNameDialog({ mode: "rename", group })}
          onDelete={setToDelete}
        />
      )}

      <GroupNameDialog
        open={!!nameDialog}
        onOpenChange={(open) => !open && setNameDialog(null)}
        title={renaming ? t("groups.renameTitle") : t("groups.newGroup")}
        initialName={renaming?.name ?? ""}
        submitLabel={renaming ? t("groups.save") : t("groups.create")}
        onSubmit={async (name) => {
          if (renaming) await renameGroup({ uuid: renaming.uuid, name });
          else
            await createGroup({
              name,
              parentUuid: nameDialog?.mode === "create" ? nameDialog.parentUuid : undefined,
            });
        }}
      />

      <AlertDialog open={!!toDelete} onOpenChange={(open) => !open && setToDelete(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("groups.deleteTitle", { name: toDelete?.name ?? "" })}
            </AlertDialogTitle>
            <AlertDialogDescription>{t("groups.deleteDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("entries.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={confirmDelete}>
              {t("groups.delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
