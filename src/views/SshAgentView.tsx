import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { tauriCommand } from "@/lib/tauri";
import { Terminal, CheckCircle, XCircle, AlertTriangle } from "lucide-react";

interface SshAgentStatus {
  enabled: boolean;
  listening: boolean;
  serviceRunning: boolean;
}

export function SshAgentView() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [status, setStatus] = useState<SshAgentStatus | null>(null);
  const [serviceError, setServiceError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    tauriCommand<SshAgentStatus>("get_ssh_agent_status").then(setStatus).catch(() => {});
  }, []);

  useEffect(() => {
    tauriCommand<boolean>("is_ssh_agent_enabled").then(setEnabled).catch(() => {});
    refresh();
  }, [refresh]);

  const handleToggle = async (checked: boolean) => {
    setEnabled(checked);
    try {
      await tauriCommand("toggle_ssh_agent", { enabled: checked });
    } catch {
      setEnabled(!checked);
    }
    refresh();
    // Le bind du pipe peut ne pas avoir abouti au moment du refresh immédiat.
    setTimeout(refresh, 500);
  };

  const handleDisableService = async () => {
    setServiceError(null);
    try {
      await tauriCommand("disable_windows_ssh_agent_service");
    } catch {
      // UAC refusée ou échec du sc stop — l'utilisateur doit le savoir.
      setServiceError(t("sshAgent.disableServiceError"));
      refresh();
      return;
    }
    // Reprend le pipe une fois le service arrêté.
    await tauriCommand("toggle_ssh_agent", { enabled: false }).catch(() => {});
    await tauriCommand("toggle_ssh_agent", { enabled: true }).catch(() => {});
    refresh();
    setTimeout(refresh, 500);
  };

  // Banner et badges basés sur le seul payload status — pas sur le state
  // optimiste `enabled` du Switch (deux round-trips qui peuvent diverger).
  const conflict = !!status && status.enabled && !status.listening && status.serviceRunning;

  return (
    <div className="space-y-6 p-4 md:p-6">
      <div>
        <h1 className="font-display text-2xl font-bold tracking-tight">{t("sshAgent.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("sshAgent.subtitle")}</p>
      </div>

      <Card>
        <CardContent className="flex items-center justify-between p-6">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <Terminal className="size-4 text-primary" />
              <h3 className="text-sm font-semibold">{t("sshAgent.enable")}</h3>
              {enabled && status && (status.listening ? (
                <Badge variant="secondary" className="gap-1 text-[10px] text-green-600"><CheckCircle className="size-2.5" />{t("sshAgent.active")}</Badge>
              ) : (
                <Badge variant="secondary" className="gap-1 text-[10px] text-muted-foreground"><XCircle className="size-2.5" />{t("sshAgent.inactive")}</Badge>
              ))}
            </div>
            <p className="text-xs text-muted-foreground">{t("sshAgent.enableDesc")}</p>
          </div>
          <Switch checked={enabled} onCheckedChange={handleToggle} />
        </CardContent>
      </Card>

      {conflict && (
        <div className="flex items-start gap-3 rounded-xl border border-amber-500/40 bg-amber-500/10 p-4">
          <AlertTriangle className="size-5 shrink-0 text-amber-500" />
          <div className="flex-1">
            <h4 className="text-sm font-semibold">{t("sshAgent.conflictTitle")}</h4>
            <p className="text-xs text-muted-foreground">{t("sshAgent.conflictDesc")}</p>
          </div>
          <Button size="sm" onClick={handleDisableService}>{t("sshAgent.disableService")}</Button>
        </div>
      )}

      {serviceError && <p className="text-xs text-destructive">{serviceError}</p>}

      <div className="rounded-xl border border-border bg-muted/30 p-4">
        <h4 className="text-sm font-semibold">{t("sshAgent.testTitle")}</h4>
        <p className="mt-1 text-xs text-muted-foreground">{t("sshAgent.testDesc")}</p>
        <code className="mt-2 block rounded bg-secondary px-2 py-1 font-mono text-xs">ssh-add -l</code>
      </div>
    </div>
  );
}
