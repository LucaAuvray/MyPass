/** Réglages et état de la synchronisation serveur. */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriCommand, isWebMode } from "@/lib/tauri";
import { useSyncStatus, syncNowAndRefresh } from "@/hooks/useSync";
import { Card, CardContent } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { RefreshCw } from "lucide-react";

type SyncConfig = { serverUrl: string; enabled: boolean; hasToken: boolean };

export function SyncView() {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const web = isWebMode();
  const config = useQuery({
    queryKey: ["sync-config"],
    queryFn: () => tauriCommand<SyncConfig>("get_sync_config"),
  });
  const status = useSyncStatus();
  const [serverUrl, setServerUrl] = useState<string | null>(null);
  const [token, setToken] = useState("");

  const save = useMutation({
    mutationFn: (enabled: boolean) =>
      tauriCommand("set_sync_config", {
        serverUrl: serverUrl ?? config.data?.serverUrl ?? "",
        token: token || null,
        enabled,
      }),
    onSuccess: () => {
      setToken("");
      qc.invalidateQueries({ queryKey: ["sync-config"] });
    },
  });
  const syncNow = useMutation({
    mutationFn: () => syncNowAndRefresh(qc),
    onSettled: () => qc.invalidateQueries({ queryKey: ["sync-status"] }),
  });

  const cfg = config.data;
  const st = status.data;
  const enabled = cfg?.enabled ?? false;

  return (
    <div className="space-y-6 p-4 md:p-6">
      <div>
        <h1 className="font-display text-2xl font-bold tracking-tight">{t("sync.title")}</h1>
      </div>

      <Card>
        <CardContent className="space-y-4 p-6">
          {!web && (
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <RefreshCw className="size-4 text-primary" />
                <h3 className="text-sm font-semibold">{t("sync.enable")}</h3>
              </div>
              <Switch checked={enabled} onCheckedChange={(checked) => save.mutate(checked)} />
            </div>
          )}

          {!web && (
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground">{t("sync.serverUrl")}</label>
              <Input
                value={serverUrl ?? cfg?.serverUrl ?? ""}
                onChange={(e) => setServerUrl(e.target.value)}
                placeholder="https://sync.example.com"
              />
            </div>
          )}

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">{t("sync.token")}</label>
            <Input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder={cfg?.hasToken ? t("sync.tokenKept") : undefined}
            />
          </div>

          <Button size="sm" onClick={() => save.mutate(enabled)} disabled={save.isPending}>
            {t("common.update")}
          </Button>
        </CardContent>
      </Card>

      <div className="flex items-center justify-between rounded-xl border border-border bg-muted/30 p-4">
        <div className="space-y-1">
          <p className="text-sm font-medium">{t(`sync.state.${st?.state ?? "idle"}`)}</p>
          {st?.detail && <p className="text-xs text-muted-foreground">{st.detail}</p>}
          {st?.lastSync && (
            <p className="text-xs text-muted-foreground">
              {t("sync.lastSync")}: {new Date(Number(st.lastSync) * 1000).toLocaleString()}
            </p>
          )}
          {st?.serverVersion && (
            <p className="text-xs text-muted-foreground">
              {t("sync.serverVersion")}: {st.serverVersion}
            </p>
          )}
        </div>
        <Button size="sm" onClick={() => syncNow.mutate()} disabled={syncNow.isPending || st?.state === "syncing"}>
          {t("sync.syncNow")}
        </Button>
      </div>
    </div>
  );
}
