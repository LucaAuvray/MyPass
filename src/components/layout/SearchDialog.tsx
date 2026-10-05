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
      <DialogContent className="gap-0 p-0 sm:max-w-xl" showCloseButton={false}>
        <div className="border-border flex items-center gap-2 border-b px-4 py-3">
          <Search className="text-muted-foreground size-4 shrink-0" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("nav.search")}
            className="border-0 bg-transparent p-0 text-sm shadow-none outline-none focus-visible:ring-0"
            autoFocus
          />
          <kbd className="border-border text-muted-foreground hidden rounded-md border px-1.5 py-0.5 text-[10px] sm:inline-block">
            ESC
          </kbd>
        </div>

        {query && filtered.length > 0 && (
          <div className="max-h-80 overflow-y-auto p-2">
            {filtered.slice(0, 15).map((entry) => (
              <button
                key={entry.uuid}
                onClick={() => handleSelect(entry.uuid)}
                className="hover:bg-muted flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors"
              >
                <EntryIcon url={entry.url} size="sm" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{entry.title}</p>
                  <p className="text-muted-foreground truncate text-xs">{entry.username}</p>
                </div>
                {entry.url && (
                  <span className="text-muted-foreground hidden truncate text-xs sm:inline">
                    {entry.url}
                  </span>
                )}
              </button>
            ))}
          </div>
        )}

        {query && filtered.length === 0 && (
          <div className="text-muted-foreground py-12 text-center text-sm">
            {t("entries.noResults")}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
