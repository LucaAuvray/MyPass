import { useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { useAppStore } from "@/stores/appStore";
import { useEntriesStore } from "@/stores/entriesStore";
import { EntryIcon } from "@/components/shared/EntryIcon";

export function SearchCommand() {
  const { t } = useTranslation();
  const searchOpen = useAppStore((s) => s.searchOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const entries = useEntriesStore((s) => s.entries);
  const selectEntry = useEntriesStore((s) => s.selectEntry);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (
        (e.key === "k" && (e.metaKey || e.ctrlKey)) ||
        (e.key === "p" && (e.metaKey || e.ctrlKey))
      ) {
        e.preventDefault();
        setSearchOpen(true);
      }
    },
    [setSearchOpen],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return (
    <CommandDialog open={searchOpen} onOpenChange={setSearchOpen}>
      <CommandInput placeholder={t("nav.search")} />
      <CommandList>
        <CommandEmpty>{t("entries.noResults")}</CommandEmpty>
        <CommandGroup heading={t("nav.allItems")}>
          {entries.slice(0, 20).map((entry) => (
            <CommandItem
              key={entry.uuid}
              onSelect={() => {
                selectEntry(entry.uuid);
                setSearchOpen(false);
              }}
              className="flex items-center gap-3"
            >
              <EntryIcon url={entry.url} size="sm" />
              <div className="flex flex-col">
                <span className="text-sm font-medium">{entry.title}</span>
                <span className="text-muted-foreground text-xs">{entry.username}</span>
              </div>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
