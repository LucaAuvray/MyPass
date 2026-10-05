import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import {
  ChevronRight,
  Folder,
  FolderOpen,
  Plus,
  MoreHorizontal,
  Pencil,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Group } from "@/types/group";

interface GroupTreeProps {
  /** The vault's root folder: only its children are shown ("All items" stands for the root). */
  root: Group;
  selectedUuid: string | null;
  onSelect: (uuid: string) => void;
  onCreate: (parentUuid?: string) => void;
  onRename: (group: Group) => void;
  onDelete: (group: Group) => void;
}

type ItemProps = Omit<GroupTreeProps, "root"> & { group: Group; depth: number };

export function GroupTree({ root, ...props }: GroupTreeProps) {
  return (
    <div className="flex flex-col gap-0.5">
      {root.children.map((group) => (
        <GroupTreeItem key={group.uuid} group={group} depth={0} {...props} />
      ))}
    </div>
  );
}

function GroupTreeItem({ group, depth, ...props }: ItemProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(group.isExpanded);
  const hasChildren = group.children.length > 0;
  const isSelected = props.selectedUuid === group.uuid;

  return (
    <div>
      <div className="group flex items-center gap-0.5">
        {hasChildren ? (
          <button
            onClick={() => setExpanded(!expanded)}
            className="text-muted-foreground hover:text-foreground rounded p-0.5"
          >
            <ChevronRight className={cn("size-3 transition-transform", expanded && "rotate-90")} />
          </button>
        ) : (
          <span className="w-4" />
        )}
        <button
          onClick={() => props.onSelect(group.uuid)}
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm font-medium transition-colors",
            isSelected
              ? "bg-sidebar-accent text-sidebar-accent-foreground"
              : "text-sidebar-foreground hover:bg-sidebar-accent/50",
          )}
          style={{ paddingLeft: `${depth * 12 + 10}px` }}
        >
          {expanded && hasChildren ? (
            <FolderOpen className="size-4 shrink-0" />
          ) : (
            <Folder className="size-4 shrink-0" />
          )}
          <span className="truncate text-left">{group.name}</span>
          <span className="text-muted-foreground ml-auto text-xs">{group.entryCount}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              // Always visible on touch screens (no hover there), on hover from md up.
              <Button
                variant="ghost"
                size="icon"
                className="size-6 shrink-0 md:opacity-0 md:group-hover:opacity-100 md:focus-visible:opacity-100 md:data-[popup-open]:opacity-100"
                aria-label={group.name}
              >
                <MoreHorizontal className="size-3" />
              </Button>
            }
          />
          <DropdownMenuContent align="end" className="w-52">
            <DropdownMenuItem onClick={() => props.onCreate(group.uuid)}>
              <Plus className="size-3.5" /> {t("groups.addSubgroup")}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => props.onRename(group)}>
              <Pencil className="size-3.5" /> {t("groups.rename")}
            </DropdownMenuItem>
            <DropdownMenuItem className="text-destructive" onClick={() => props.onDelete(group)}>
              <Trash2 className="size-3.5" /> {t("groups.delete")}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {expanded &&
        group.children.map((child) => (
          <GroupTreeItem key={child.uuid} group={child} depth={depth + 1} {...props} />
        ))}
    </div>
  );
}
