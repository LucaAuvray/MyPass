import { useState } from "react";
import { useTranslation } from "react-i18next";
import { EntryList } from "@/components/entries/EntryList";
import { EntryDetail } from "@/components/entries/EntryDetail";
import { EntryForm } from "@/components/entries/EntryForm";
import { useEntriesStore } from "@/stores/entriesStore";
import { useEntries } from "@/hooks/useEntries";
import { PanelRightClose } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function AllItemsView() {
  const { t } = useTranslation();
  // Trigger TanStack Query → Zustand bridge
  useEntries();
  const selectedEntryId = useEntriesStore((s) => s.selectedEntryId);
  const selectEntry = useEntriesStore((s) => s.selectEntry);
  const [showNewEntry, setShowNewEntry] = useState(false);

  return (
    <div className="flex h-full">
      {/* Entry list */}
      <div className={cn("flex-1 overflow-y-auto p-4 md:p-6", selectedEntryId && "hidden lg:block lg:w-1/2")}>
        <EntryList onCreateClick={() => setShowNewEntry(true)} />
        <EntryForm open={showNewEntry} onOpenChange={setShowNewEntry} />
      </div>

      {/* Detail panel */}
      {selectedEntryId && (
        <div className="w-full border-l border-border bg-card lg:w-1/2">
          <div className="flex items-center justify-between border-b border-border px-4 py-2">
            <span className="text-xs text-muted-foreground">{t("entries.details")}</span>
            <Button
              variant="ghost"
              size="icon"
              className="size-7"
              onClick={() => selectEntry(null)}
            >
              <PanelRightClose className="size-4" />
            </Button>
          </div>
          <div className="overflow-y-auto">
            <EntryDetail />
          </div>
        </div>
      )}
    </div>
  );
}
