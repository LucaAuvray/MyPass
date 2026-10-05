import { useEffect, useRef, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { useDatabase } from "@/hooks/useDatabase";
import { useAppStore } from "@/stores/appStore";
import { tauriCommand } from "@/lib/tauri";
import { Clock } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from "@/components/ui/dropdown-menu";

const DELAY_CHOICES = [1, 5, 15, 30, 60, 0]; // minutes; 0 = never

interface BrowserActivityStatus {
  browsers: { last_seen: string }[];
}

interface AutoLockTimerProps {
  className?: string;
}

export function AutoLockTimer({ className }: AutoLockTimerProps) {
  const { t } = useTranslation();
  // Lock the real backend vault, not just the UI flag — otherwise the Rust
  // DbState stays open and the browser bridge keeps serving passwords after
  // the screen shows "locked".
  const { lockDatabase } = useDatabase();
  const timeoutMinutes = useAppStore((s) => s.autoLockMinutes);
  const setAutoLockMinutes = useAppStore((s) => s.setAutoLockMinutes);
  const [remainingSeconds, setRemainingSeconds] = useState(timeoutMinutes * 60);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const lastActivityRef = useRef(Date.now());

  const resetTimer = useCallback(() => {
    lastActivityRef.current = Date.now();
    setRemainingSeconds(timeoutMinutes * 60);
  }, [timeoutMinutes]);

  useEffect(() => {
    if (timeoutMinutes <= 0) return;
    resetTimer();

    const handleActivity = () => resetTimer();
    window.addEventListener("mousemove", handleActivity);
    window.addEventListener("keydown", handleActivity);
    window.addEventListener("click", handleActivity);

    // Browser extension traffic (autofill, password generation…) is user
    // activity too — without this the vault locks mid-browsing because only
    // the MyPass window's own events reset the timer.
    let tick = 0;
    const checkBrowserActivity = () => {
      tauriCommand<BrowserActivityStatus>("get_browser_status")
        .then((status) => {
          const latest = Math.max(0, ...status.browsers.map((b) => Number(b.last_seen) || 0));
          if (latest * 1000 > lastActivityRef.current) {
            resetTimer();
          }
        })
        .catch(() => {});
    };

    timerRef.current = setInterval(() => {
      if (++tick % 30 === 0) checkBrowserActivity();

      const elapsed = (Date.now() - lastActivityRef.current) / 1000;
      const remaining = Math.max(0, timeoutMinutes * 60 - Math.floor(elapsed));
      setRemainingSeconds(remaining);

      if (remaining <= 0) {
        void lockDatabase();
        if (timerRef.current) clearInterval(timerRef.current);
      }
    }, 1000);

    return () => {
      window.removeEventListener("mousemove", handleActivity);
      window.removeEventListener("keydown", handleActivity);
      window.removeEventListener("click", handleActivity);
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [timeoutMinutes, lockDatabase, resetTimer]);

  const minutes = Math.floor(remainingSeconds / 60);
  const seconds = remainingSeconds % 60;
  const isLow = timeoutMinutes > 0 && remainingSeconds < 30;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        className={cn(
          "hover:bg-secondary flex items-center gap-1.5 rounded-lg px-1.5 py-1 text-xs transition-colors",
          isLow ? "text-amber-500" : "text-muted-foreground",
          className,
        )}
        title={t("security.autoLock") ?? "Auto-lock timer"}
      >
        <Clock className="size-3" />
        {timeoutMinutes <= 0 ? (
          <span>{t("security.autoLockNever")}</span>
        ) : (
          <span>
            {minutes}:{seconds.toString().padStart(2, "0")}
          </span>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup
          value={String(timeoutMinutes)}
          onValueChange={(v) => setAutoLockMinutes(Number(v))}
        >
          {/* Base UI: GroupLabel must live inside a Group/RadioGroup */}
          <DropdownMenuLabel>{t("security.autoLock")}</DropdownMenuLabel>
          {DELAY_CHOICES.map((m) => (
            <DropdownMenuRadioItem key={m} value={String(m)}>
              {m === 0 ? t("security.autoLockNever") : t("security.autoLockAfter", { count: m })}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
