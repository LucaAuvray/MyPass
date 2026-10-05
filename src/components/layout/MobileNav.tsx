import { useLocation, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/stores/appStore";
import { useEntriesStore } from "@/stores/entriesStore";
import { Home, Search, Shield, RefreshCw } from "lucide-react";

interface MobileNavProps {
  className?: string;
}

export function MobileNav({ className }: MobileNavProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const location = useLocation();
  const setSearchOpen = useAppStore((s) => s.setSearchOpen);
  const setKindFilter = useEntriesStore((s) => s.setKindFilter);

  return (
    <nav
      className={cn(
        // min-h (pas h) : le padding safe-area iOS (barre home) agrandit la nav
        "border-border bg-background flex min-h-16 shrink-0 items-center justify-around border-t pb-[env(safe-area-inset-bottom)]",
        className,
      )}
    >
      <MobileNavItem
        icon={Home}
        label={t("nav.home")}
        active={location.pathname === "/"}
        onClick={() => {
          navigate("/");
          setKindFilter(null);
        }}
      />
      <MobileNavItem icon={Search} label={t("nav.search")} onClick={() => setSearchOpen(true)} />
      <MobileNavItem
        icon={Shield}
        label={t("nav.security")}
        active={location.pathname === "/security"}
        onClick={() => navigate("/security")}
      />
      <MobileNavItem
        icon={RefreshCw}
        label={t("nav.sync")}
        active={location.pathname === "/sync"}
        onClick={() => navigate("/sync")}
      />
    </nav>
  );
}

interface MobileNavItemProps {
  icon: React.ElementType;
  label: string;
  active?: boolean;
  onClick?: () => void;
}

function MobileNavItem({ icon: Icon, label, active, onClick }: MobileNavItemProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex flex-col items-center gap-0.5 text-[10px] font-medium transition-colors",
        active ? "text-primary" : "text-muted-foreground",
      )}
    >
      <Icon className="size-5" />
      <span>{label}</span>
    </button>
  );
}
