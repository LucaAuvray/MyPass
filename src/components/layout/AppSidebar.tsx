import { useState } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/appStore";
import { useDatabase } from "@/hooks/useDatabase";
import { useEntriesStore } from "@/stores/entriesStore";
import { GroupTree } from "@/components/groups/GroupTree";
import { SearchDialog } from "@/components/layout/SearchDialog";
import { LanguageSwitcher } from "@/components/shared/LanguageSwitcher";
import { Lock, FolderOpen, Settings, Shield, KeyRound, Plus, Upload, Download, Search, Contact, CreditCard, FileText, Terminal, RefreshCw } from "lucide-react";
import { Separator } from "@/components/ui/separator";
import { EntryForm } from "@/components/entries/EntryForm";
import { ItemForm } from "@/components/entries/ItemForm";
import { SshKeyForm } from "@/components/entries/SshKeyForm";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ImportWizard } from "@/components/import-export/ImportWizard";
import { ExportDialog } from "@/components/import-export/ExportDialog";
import type { Group } from "@/types/group";
import type { ItemKind } from "@/lib/items";
import logo from "@/assets/logo.png";

interface AppSidebarProps {
  className?: string;
  groups?: Group[];
  /** Appelé quand un item de navigation est cliqué (fermeture du drawer mobile) */
  onNavigate?: () => void;
}

export function AppSidebar({ className, groups = [], onNavigate }: AppSidebarProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const sidebarCollapsed = useAppStore((s) => s.sidebarCollapsed);
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const { lockDatabase } = useDatabase();
  // Verrouillage réel (ferme le coffre backend + flush des push en attente),
  // pas seulement l'UI — appStore.lock seul laisserait le coffre déchiffré.
  const lock = () => void lockDatabase().catch(() => {});
  const selectEntry = useEntriesStore((s) => s.selectEntry);
  const kindFilter = useEntriesStore((s) => s.kindFilter);
  const setKindFilter = useEntriesStore((s) => s.setKindFilter);
  const [showNewEntry, setShowNewEntry] = useState(false);
  const [newItemKind, setNewItemKind] = useState<Exclude<ItemKind, "login"> | null>(null);
  const [showImport, setShowImport] = useState(false);
  const [showExport, setShowExport] = useState(false);

  return (
    <>
      <SearchDialog />
      <EntryForm open={showNewEntry} onOpenChange={setShowNewEntry} />
      {newItemKind && newItemKind !== "ssh_key" && (
        <ItemForm
          kind={newItemKind}
          open={!!newItemKind}
          onOpenChange={(o) => !o && setNewItemKind(null)}
        />
      )}
      <SshKeyForm open={newItemKind === "ssh_key"} onOpenChange={(o) => !o && setNewItemKind(null)} />
      <ImportWizard open={showImport} onOpenChange={setShowImport} />
      <ExportDialog open={showExport} onOpenChange={setShowExport} />

      <aside
        className={cn(
          "flex flex-col border-r border-border bg-sidebar transition-all duration-300",
          sidebarCollapsed ? "w-16" : "w-60",
          className,
        )}
      >
        {/* Logo */}
        <div className="flex h-14 items-center gap-2.5 border-b border-sidebar-border px-4">
          <img src={logo} alt="MyPass" className="size-8 shrink-0 rounded-lg shadow-sm shadow-primary/25" />
          {!sidebarCollapsed && (
            <span className="font-display text-lg font-semibold tracking-tight text-sidebar-foreground">
              {t("app.name")}
            </span>
          )}
        </div>

        {!sidebarCollapsed && (
          <>
            {/* New Entry Button */}
            <div className="px-3 pt-3">
              <DropdownMenu>
                <DropdownMenuTrigger className="flex w-full items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90">
                  <Plus className="size-4" />
                  {t("nav.newItem")}
                </DropdownMenuTrigger>
                <DropdownMenuContent align="start" className="w-52">
                  <DropdownMenuItem onClick={() => setShowNewEntry(true)}>
                    <KeyRound className="size-4" /> {t("nav.newPassword")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("identity")}>
                    <Contact className="size-4" /> {t("items.new.identity")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("card")}>
                    <CreditCard className="size-4" /> {t("items.new.card")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("document")}>
                    <FileText className="size-4" /> {t("items.new.document")}
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => setNewItemKind("ssh_key")}>
                    <Terminal className="size-4" /> {t("items.new.ssh_key")}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>

            {/* Groups */}
            <div className="mt-3 flex-1 overflow-y-auto px-2">
              <GroupTree
                groups={groups}
                selectedGroupId={null}
                onSelectGroup={() => {}}
              />
            </div>

            <Separator className="mx-3" />

            {/* Bottom nav — onClick délégué : tout item cliqué notifie onNavigate */}
            <nav className="flex flex-col gap-0.5 p-2" onClick={onNavigate}>
              <SidebarItem icon={FolderOpen} label={t("nav.allItems")} active={location.pathname === "/" && !kindFilter} onClick={() => { navigate("/"); setKindFilter(null); selectEntry(null); }} />
              <SidebarItem icon={Contact} label={t("nav.identities")} active={location.pathname === "/" && kindFilter === "identity"} onClick={() => { navigate("/"); setKindFilter("identity"); }} />
              <SidebarItem icon={CreditCard} label={t("nav.cards")} active={location.pathname === "/" && kindFilter === "card"} onClick={() => { navigate("/"); setKindFilter("card"); }} />
              <SidebarItem icon={FileText} label={t("nav.documents")} active={location.pathname === "/" && kindFilter === "document"} onClick={() => { navigate("/"); setKindFilter("document"); }} />
              <SidebarItem icon={Terminal} label={t("nav.sshKeys")} active={location.pathname === "/" && kindFilter === "ssh_key"} onClick={() => { navigate("/"); setKindFilter("ssh_key"); }} />
              <SidebarItem icon={Terminal} label={t("nav.sshAgent")} active={location.pathname === "/ssh-agent"} onClick={() => navigate("/ssh-agent")} />
              <SidebarItem icon={Shield} label={t("nav.security")} active={location.pathname === "/security"} onClick={() => navigate("/security")} />
              <SidebarItem icon={RefreshCw} label={t("nav.sync")} active={location.pathname === "/sync"} onClick={() => navigate("/sync")} />
              <SidebarItem icon={Search} label={t("nav.search")} shortcut="⌘K" active={false} onClick={() => setSearchOpen(true)} />
              <SidebarItem icon={Settings} label={t("nav.settings")} active={location.pathname === "/browser"} onClick={() => navigate("/browser")} />
            </nav>

            {/* Import / Export */}
            <div className="flex gap-1 p-2">
              <button onClick={() => setShowImport(true)} className="flex flex-1 items-center justify-center gap-1.5 rounded-lg px-2 py-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground">
                <Upload className="size-3.5" />{t("nav.import")}
              </button>
              <button onClick={() => setShowExport(true)} className="flex flex-1 items-center justify-center gap-1.5 rounded-lg px-2 py-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground">
                <Download className="size-3.5" />{t("nav.export")}
              </button>
            </div>

            {/* Language Switcher */}
            <div className="px-2">
              <LanguageSwitcher className="w-full justify-center" />
            </div>

            {/* Lock button */}
            <div className="p-2">
              <button onClick={lock} className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground">
                <Lock className="size-4" />{t("nav.lock")}
              </button>
            </div>
          </>
        )}

        {sidebarCollapsed && (
          <nav className="flex flex-1 flex-col items-center gap-3 pt-4">
            <CollapsedIcon icon={Plus} label={t("nav.newPassword")} onClick={() => setShowNewEntry(true)} />
            <CollapsedIcon icon={FolderOpen} label={t("nav.allItems")} />
            <CollapsedIcon icon={Shield} label={t("nav.security")} />
            <LanguageSwitcher />
            <div className="mt-auto">
              <CollapsedIcon icon={Lock} label={t("nav.lock")} onClick={lock} />
            </div>
          </nav>
        )}
      </aside>
    </>
  );
}

function SidebarItem({
  icon: Icon,
  label,
  active,
  shortcut,
  onClick,
}: {
  icon: React.ElementType;
  label: string;
  active?: boolean;
  shortcut?: string;
  onClick?: () => void;
}) {
  const sidebarCollapsed = useAppStore((s) => s.sidebarCollapsed);

  return (
    <button
      className={cn(
        "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-sm font-medium transition-colors",
        active ? "bg-sidebar-accent text-sidebar-accent-foreground" : "text-sidebar-foreground hover:bg-sidebar-accent/50",
        sidebarCollapsed && "justify-center px-0",
      )}
      title={sidebarCollapsed ? label : undefined}
      onClick={onClick}
    >
      <Icon className="size-[18px] shrink-0" />
      {!sidebarCollapsed && (
        <>
          <span className="flex-1 truncate text-left">{label}</span>
          {shortcut && <kbd className="hidden rounded-md border border-sidebar-border px-1.5 py-0.5 text-[10px] text-muted-foreground lg:inline-block">{shortcut}</kbd>}
        </>
      )}
    </button>
  );
}

function CollapsedIcon({ icon: Icon, label, onClick }: { icon: React.ElementType; label: string; onClick?: () => void }) {
  return (
    <button className="flex size-9 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground" title={label} onClick={onClick}>
      <Icon className="size-5" />
    </button>
  );
}
