import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { Dialog, DialogContent } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useAppStore } from "@/stores/appStore";
import { useEntriesStore } from "@/stores/entriesStore";
import { EntryIcon } from "@/components/shared/EntryIcon";
import { Search } from "lucide-react";

export function SearchDialog() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const searchOpen = useAppStore((s) => s.searchOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const entries = useEntriesStore((s) => s.entries);
  const selectEntry = useEntriesStore((s) => s.selectEntry);
  const [query, setQuery] = useState("");

  const filtered = query
    ? entries.filter(
        (e) =>
          e.title.toLowerCase().includes(query.toLowerCase()) ||
          e.username.toLowerCase().includes(query.toLowerCase()) ||
          e.url.toLowerCase().includes(query.toLowerCase()),
      )
    : [];

  // ⌘K / Ctrl+K keyboard shortcut
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if ((e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setSearchOpen(!searchOpen);
      }
      if (e.key === "Escape" && searchOpen) {
        setSearchOpen(false);
      }
    },
    [searchOpen, setSearchOpen],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const handleSelect = (uuid: string) => {
    selectEntry(uuid);
    setSearchOpen(false);
    setQuery("");
    navigate("/");
  };

  const handleOpenChange = (open: boolean) => {
    setSearchOpen(open);
    if (!open) setQuery("");
  };

  return (
    <Dialog open={searchOpen} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-xl p-0 gap-0" showCloseButton={false}>
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <Search className="size-4 shrink-0 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("nav.search")}
            className="border-0 bg-transparent p-0 text-sm shadow-none outline-none focus-visible:ring-0"
            autoFocus
          />
          <kbd className="hidden rounded-md border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground sm:inline-block">
            ESC
          </kbd>
        </div>

        {query && filtered.length > 0 && (
          <div className="max-h-80 overflow-y-auto p-2">
            {filtered.slice(0, 15).map((entry) => (
              <button
                key={entry.uuid}
                onClick={() => handleSelect(entry.uuid)}
                className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors hover:bg-muted"
              >
                <EntryIcon url={entry.url} size="sm" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{entry.title}</p>
                  <p className="truncate text-xs text-muted-foreground">
                    {entry.username}
                  </p>
                </div>
                {entry.url && (
                  <span className="hidden truncate text-xs text-muted-foreground sm:inline">
                    {entry.url}
                  </span>
                )}
              </button>
            ))}
          </div>
        )}

        {query && filtered.length === 0 && (
          <div className="py-12 text-center text-sm text-muted-foreground">
            {t("entries.noResults")}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
