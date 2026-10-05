import { useEffect, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { AppSidebar } from "./AppSidebar";
import { MobileNav } from "./MobileNav";
import { AppHeader } from "./AppHeader";
import { useAppStore } from "@/stores/appStore";
import { SshSignRequestDialog } from "@/components/ssh/SshSignRequestDialog";
import { isWebMode } from "@/lib/tauri";
import { useSyncStatus, syncNowAndRefresh } from "@/hooks/useSync";
import { useDatabase } from "@/hooks/useDatabase";

interface AppShellProps {
  children: ReactNode;
}

export default function AppShell({ children }: AppShellProps) {
  const { t } = useTranslation();
  const status = useSyncStatus();
  const { lockDatabase } = useDatabase();
  const queryClient = useQueryClient();

  useEffect(() => {
    const id = setInterval(() => {
      void syncNowAndRefresh(queryClient).catch(() => {});
    }, 60_000);
    return () => clearInterval(id);
  }, [queryClient]);

  // Mode web : verrouiller après 60 s onglet masqué (rien ne doit rester
  // déchiffré dans un onglet oublié en arrière-plan sur iPhone).
  useEffect(() => {
    if (!isWebMode()) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let hiddenAt: number | null = null;
    const doLock = () => void lockDatabase().catch(() => {});
    const onVisibility = () => {
      if (document.hidden) {
        clearTimeout(timer);
        hiddenAt = Date.now();
        timer = setTimeout(() => {
          doLock();
          hiddenAt = null;
        }, 60_000);
      } else {
        clearTimeout(timer);
        // iOS gèle les timers d'une page en arrière-plan : le timeout ne fire
        // jamais pendant la suspension — on rattrape au retour en comparant
        // les horodatages.
        if (hiddenAt !== null && Date.now() - hiddenAt >= 60_000) doLock();
        hiddenAt = null;
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [lockDatabase]);

  const notSynced = status.data?.state === "offline" || status.data?.state === "error";
  const mobileMenuOpen = useAppStore((s) => s.mobileMenuOpen);
  const setMobileMenuOpen = useAppStore((s) => s.setMobileMenuOpen);

  return (
    // pt safe-area : en PWA iOS plein écran, ne pas passer sous la barre de statut
    <div className="bg-background flex h-screen overflow-hidden pt-[env(safe-area-inset-top)]">
      {/* Desktop Sidebar */}
      <AppSidebar className="hidden md:flex" />

      <div className="flex flex-1 flex-col overflow-hidden">
        <AppHeader />
        {notSynced && (
          <div
            className="border-border flex items-center gap-1.5 border-b bg-amber-500/10 px-4 py-1 text-xs text-amber-600"
            title={status.data?.detail ?? undefined}
          >
            <span className="size-1.5 rounded-full bg-amber-500" />
            {t("sync.notSynced")}
          </div>
        )}
        <main className="flex-1 overflow-y-auto p-4 md:p-6 lg:p-8">{children}</main>

        {/* Mobile Bottom Nav — dans la colonne verticale, pas en 3e colonne du flex */}
        <MobileNav className="md:hidden" />
      </div>

      {/* Drawer mobile : la sidebar complète en overlay (monté seulement ouvert,
          sinon ses dialogs portalisés doubleraient ceux de la sidebar desktop) */}
      {mobileMenuOpen && (
        <div className="fixed inset-0 z-50 md:hidden">
          <div className="absolute inset-0 bg-black/50" onClick={() => setMobileMenuOpen(false)} />
          <AppSidebar
            className="absolute inset-y-0 left-0 flex"
            onNavigate={() => setMobileMenuOpen(false)}
          />
        </div>
      )}

      <SshSignRequestDialog />
    </div>
  );
}
