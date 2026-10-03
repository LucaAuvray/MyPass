import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { ChevronRight, Folder, FolderOpen, Plus, MoreHorizontal } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Group } from "@/types/group";

interface GroupTreeProps {
  groups: Group[];
  selectedGroupId: string | null;
  onSelectGroup: (uuid: string | null) => void;
  onCreateGroup?: (parentUuid?: string) => void;
}

export function GroupTree({ groups, selectedGroupId, onSelectGroup, onCreateGroup }: GroupTreeProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-1">
      {/* All Items */}
      <button
        onClick={() => onSelectGroup(null)}
        className={cn(
          "flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm font-medium transition-colors",
          selectedGroupId === null
            ? "bg-sidebar-accent text-sidebar-accent-foreground"
            : "text-sidebar-foreground hover:bg-sidebar-accent/50",
        )}
      >
        <FolderOpen className="size-4 shrink-0" />
        <span className="truncate text-left">{t("nav.allItems")}</span>
      </button>

      {groups.map((group) => (
        <GroupTreeItem
          key={group.uuid}
          group={group}
          selectedGroupId={selectedGroupId}
          onSelectGroup={onSelectGroup}
          onCreateGroup={onCreateGroup}
          depth={0}
        />
      ))}

      {onCreateGroup && (
        <Button
          variant="ghost"
          size="sm"
          className="mt-1 justify-start gap-2 text-muted-foreground hover:text-foreground"
          onClick={() => onCreateGroup()}
        >
          <Plus className="size-3.5" />
          {t("groups.newGroup")}
        </Button>
      )}
    </div>
  );
}

function GroupTreeItem({
  group,
  selectedGroupId,
  onSelectGroup,
  onCreateGroup,
  depth,
}: {
  group: Group;
  selectedGroupId: string | null;
  onSelectGroup: (uuid: string | null) => void;
  onCreateGroup?: (parentUuid?: string) => void;
  depth: number;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(group.isExpanded);
  const hasChildren = group.children && group.children.length > 0;
  const isSelected = selectedGroupId === group.uuid;
  const entryCount = group.entryCount;

  return (
    <div>
      <div className="group flex items-center gap-0.5">
        {hasChildren ? (
          <button
            onClick={() => setExpanded(!expanded)}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground"
          >
            <ChevronRight className={cn("size-3 transition-transform", expanded && "rotate-90")} />
          </button>
        ) : (
          <span className="w-4" />
        )}
        <button
          onClick={() => onSelectGroup(group.uuid)}
          className={cn(
            "flex flex-1 items-center gap-2 rounded-lg px-2.5 py-1.5 text-sm font-medium transition-colors",
            isSelected
              ? "bg-sidebar-accent text-sidebar-accent-foreground"
              : "text-sidebar-foreground hover:bg-sidebar-accent/50",
          )}
          style={{ paddingLeft: `${depth * 12 + 10}px` }}
        >
          {expanded ? (
            <FolderOpen className="size-4 shrink-0" />
          ) : (
            <Folder className="size-4 shrink-0" />
          )}
          <span className="truncate text-left">{group.name}</span>
          <span className="ml-auto text-xs text-muted-foreground">{entryCount}</span>
        </button>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="icon"
                className="size-6 shrink-0 opacity-0 group-hover:opacity-100"
              >
                <MoreHorizontal className="size-3" />
              </Button>
            }
          />
          <DropdownMenuContent align="end">
            {onCreateGroup && (
              <DropdownMenuItem onClick={() => onCreateGroup(group.uuid)}>
                <Plus className="size-3.5" /> {t("groups.addSubgroup")}
              </DropdownMenuItem>
            )}
            <DropdownMenuItem>{t("groups.rename")}</DropdownMenuItem>
            <DropdownMenuItem className="text-destructive">{t("groups.delete")}</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {expanded && hasChildren && (
        <div>
          {group.children.map((child) => (
            <GroupTreeItem
              key={child.uuid}
              group={child}
              selectedGroupId={selectedGroupId}
              onSelectGroup={onSelectGroup}
              onCreateGroup={onCreateGroup}
              depth={depth + 1}
            />
          ))}
        </div>
      )}
    </div>
  );
}
