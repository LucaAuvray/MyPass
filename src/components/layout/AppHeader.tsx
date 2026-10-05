import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/appStore";
import { AutoLockTimer } from "@/components/security/AutoLockTimer";
import { EntryForm } from "@/components/entries/EntryForm";
import { Menu, Search, Plus } from "lucide-react";

interface AppHeaderProps {
  className?: string;
}

export function AppHeader({ className }: AppHeaderProps) {
  const { t } = useTranslation();
  const setMobileMenuOpen = useAppStore((s) => s.setMobileMenuOpen);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const [showNewEntry, setShowNewEntry] = useState(false);

  return (
    <header
      className={cn(
        "border-border bg-background flex h-14 shrink-0 items-center gap-3 border-b px-4",
        className,
      )}
    >
      <button
        onClick={() => setMobileMenuOpen(true)}
        className="text-muted-foreground hover:bg-secondary rounded-lg p-1.5 md:hidden"
        aria-label={t("common.menu")}
      >
        <Menu className="size-5" />
      </button>
      <button
        onClick={() => setSearchOpen(true)}
        className="border-input bg-muted/50 text-muted-foreground hover:bg-muted flex flex-1 items-center gap-2 rounded-lg border px-3 py-1.5 text-sm transition-colors md:max-w-md"
      >
        <Search className="size-4" />
        <span>{t("nav.search")}</span>
        <kbd className="border-border ml-auto hidden rounded-md border px-1.5 py-0.5 text-xs md:inline-block">
          ⌘K
        </kbd>
      </button>
      <AutoLockTimer />
      <button
        onClick={() => setShowNewEntry(true)}
        className="bg-primary text-primary-foreground hover:bg-primary/90 flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors"
        aria-label={t("entries.new")}
      >
        <Plus className="size-4" />
        <span className="hidden sm:inline">{t("entries.new")}</span>
      </button>
      <EntryForm open={showNewEntry} onOpenChange={setShowNewEntry} />
    </header>
  );
}
