import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/appStore";
import { AutoLockTimer } from "@/components/security/AutoLockTimer";
import { EntryForm } from "@/components/entries/EntryForm";
import { Menu, Search, Plus } from "lucide-react";

interface AppHeaderProps { className?: string; }

export function AppHeader({ className }: AppHeaderProps) {
  const { t } = useTranslation();
  const setMobileMenuOpen = useAppStore((s) => s.setMobileMenuOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const [showNewEntry, setShowNewEntry] = useState(false);

  return (
    <header className={cn("flex h-14 shrink-0 items-center gap-3 border-b border-border bg-background px-4", className)}>
      <button onClick={() => setMobileMenuOpen(true)} className="rounded-lg p-1.5 text-muted-foreground hover:bg-secondary md:hidden" aria-label={t("common.menu")}><Menu className="size-5" /></button>
      <button onClick={() => setSearchOpen(true)} className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-muted/50 px-3 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted md:max-w-md">
        <Search className="size-4" /><span>{t("nav.search")}</span>
        <kbd className="ml-auto hidden rounded-md border border-border px-1.5 py-0.5 text-xs md:inline-block">⌘K</kbd>
      </button>
      <AutoLockTimer />
      <button onClick={() => setShowNewEntry(true)} className="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90" aria-label={t("entries.new")}>
        <Plus className="size-4" /><span className="hidden sm:inline">{t("entries.new")}</span>
      </button>
      <EntryForm open={showNewEntry} onOpenChange={setShowNewEntry} />
    </header>
  );
}
