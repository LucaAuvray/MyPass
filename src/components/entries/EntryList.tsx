import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useEntriesStore } from "@/stores/entriesStore";
import { itemKind } from "@/lib/items";
import { EntryCard } from "./EntryCard";
import { ItemForm } from "./ItemForm";
import { SshKeyForm } from "./SshKeyForm";
import { EmptyState } from "@/components/shared/EmptyState";
import { Key, Contact, CreditCard, FileText, Terminal, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

const EMPTY_ICONS = {
  identity: Contact,
  card: CreditCard,
  document: FileText,
  ssh_key: Terminal,
} as const;

interface EntryListProps {
  onCreateClick?: () => void;
}

export function EntryList({ onCreateClick }: EntryListProps) {
  const { t } = useTranslation();
  const entries = useEntriesStore((s) => s.entries);
  const searchQuery = useEntriesStore((s) => s.searchQuery);
  const selectedEntryId = useEntriesStore((s) => s.selectedEntryId);
  const selectEntry = useEntriesStore((s) => s.selectEntry);
  const kindFilter = useEntriesStore((s) => s.kindFilter);
  const [showItemForm, setShowItemForm] = useState(false);

  const byKind = kindFilter
    ? entries.filter((e) => itemKind(e) === kindFilter)
    : entries;

  const filtered = searchQuery
    ? byKind.filter(
        (e) =>
          e.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          e.username.toLowerCase().includes(searchQuery.toLowerCase()) ||
          e.url.toLowerCase().includes(searchQuery.toLowerCase()),
      )
    : byKind;

  if (byKind.length === 0) {
    // État vide propre au type filtré : texte dédié et création du bon type
    if (kindFilter && kindFilter !== "login") {
      const EmptyIcon = EMPTY_ICONS[kindFilter];
      return (
        <>
          {kindFilter === "ssh_key" ? (
            <SshKeyForm open={showItemForm} onOpenChange={setShowItemForm} />
          ) : (
            <ItemForm kind={kindFilter} open={showItemForm} onOpenChange={setShowItemForm} />
          )}
          <EmptyState
            icon={<EmptyIcon className="size-12" />}
            title={t(`items.empty.${kindFilter}`)}
            description={t(`items.emptyDesc.${kindFilter}`)}
            action={
              <Button onClick={() => setShowItemForm(true)} size="sm">
                <Plus className="size-4" data-icon="inline-start" />
                {t(`items.new.${kindFilter}`)}
              </Button>
            }
            className="py-24"
          />
        </>
      );
    }

    return (
      <EmptyState
        icon={<Key className="size-12" />}
        title={t("entries.empty")}
        description={t("entries.emptyDesc")}
        action={
          onCreateClick ? (
            <Button onClick={onCreateClick} size="sm">
              <Plus className="size-4" data-icon="inline-start" />
              {t("entries.new")}
            </Button>
          ) : undefined
        }
        className="py-24"
      />
    );
  }

  if (filtered.length === 0) {
    return (
      <EmptyState
        title={t("entries.noResults")}
        description={t("entries.noResultsDesc", { query: searchQuery })}
        className="py-24"
      />
    );
  }

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {filtered.map((entry) => (
        <EntryCard
          key={entry.uuid}
          entry={entry}
          isSelected={selectedEntryId === entry.uuid}
          onClick={() => selectEntry(entry.uuid)}
        />
      ))}
    </div>
  );
}

export function EntryListSkeleton() {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
      {Array.from({ length: 8 }).map((_, i) => (
        <div key={i} className="rounded-xl border border-border p-4">
          <div className="flex items-start gap-3">
            <Skeleton className="size-10 rounded-xl" />
            <div className="flex-1 space-y-2">
              <Skeleton className="h-4 w-24" />
              <Skeleton className="h-3 w-32" />
              <div className="flex gap-1">
                <Skeleton className="h-4 w-12 rounded-full" />
                <Skeleton className="h-4 w-16 rounded-full" />
              </div>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
