import { useState, useEffect, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { useDatabase } from "@/hooks/useDatabase";
import { syncNowAndRefresh } from "@/hooks/useSync";
import { LanguageSwitcher } from "@/components/shared/LanguageSwitcher";
import { isWebMode, hasWebToken, setWebToken } from "@/lib/tauri";
import { Lock, Eye, EyeOff, Key, Upload, KeyRound, Server, Download, ArrowLeft } from "lucide-react";
import logo from "@/assets/logo.png";

/** unlock: vault present · choose: desktop without vault · fetch/create: its two ways out. */
type Mode = "unlock" | "choose" | "fetch" | "create";

export default function UnlockView() {
  const { t, i18n } = useTranslation();
  const {
    vaultLocation,
    openDatabase,
    createDatabase,
    fetchFromServer,
    isOpening,
    isCreating,
    isFetching,
    openError,
    createError,
    fetchError,
  } = useDatabase();
  const web = isWebMode();
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [mode, setMode] = useState<Mode>(web ? "unlock" : "choose");
  const [dbName, setDbName] = useState("");
  const [token, setToken] = useState("");
  const [serverUrl, setServerUrl] = useState("");
  const needsToken = web && !hasWebToken();
  const queryClient = useQueryClient();
  const busy = isOpening || isCreating || isFetching;

  // Desktop: the vault on disk decides — present means unlock, nothing else.
  const current: Mode | null = web
    ? mode
    : vaultLocation.isPending
      ? null
      : vaultLocation.data?.exists
        ? "unlock"
        : mode === "unlock"
          ? "choose"
          : mode;

  // Update vault name when language changes
  useEffect(() => {
    setDbName(t("unlock.vaultName"));
  }, [i18n.language, t]);

  const go = (next: Mode) => {
    setMode(next);
    setPassword("");
  };

  /** Run an open/create/fetch; its error is shown from the mutation state. */
  const run = async (action: () => Promise<unknown>) => {
    try {
      await action();
      void syncNowAndRefresh(queryClient).catch(() => {});
    } catch {
      // displayed below via openError / createError / fetchError
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (web && token) setWebToken(token);
    if (!password || busy) return;
    if (current === "unlock") void run(() => openDatabase({ password }));
    if (current === "create" && dbName) void run(() => createDatabase({ password, name: dbName, encryption: "aes256" }));
    if (current === "fetch" && serverUrl && token) void run(() => fetchFromServer({ serverUrl, token, password }));
  };

  const rawError =
    current === "unlock" ? openError : current === "create" ? createError : current === "fetch" ? fetchError : undefined;
  const message = rawError ?? vaultLocation.error?.message;
  const error =
    message && /^[A-Z_]+$/.test(message) && i18n.exists(`unlock.errors.${message}`) ? t(`unlock.errors.${message}`) : message;

  const title =
    current === "unlock"
      ? t("unlock.title")
      : current === "create"
        ? t("unlock.createTitle")
        : current === "fetch"
          ? t("unlock.fetchTitle")
          : current === "choose"
            ? t("unlock.noVaultTitle")
            : "";

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background p-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <img src={logo} alt="MyPass" className="mx-auto mb-4 size-16 rounded-2xl shadow-lg shadow-primary/25" />
          <h1 className="font-display text-2xl font-bold tracking-tight text-foreground">{t("app.name")}</h1>
          {title && <p className="mt-1.5 text-sm text-muted-foreground">{title}</p>}
        </div>
        {error && <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-2.5 text-sm text-destructive">{error}</div>}

        {current === "choose" && (
          <div className="flex flex-col gap-3">
            <button onClick={() => go("fetch")} className="flex items-center justify-center gap-2 rounded-xl bg-primary py-2.5 text-sm font-semibold text-primary-foreground transition-all hover:bg-primary/90 active:scale-[0.98]"><Download className="size-4" />{t("unlock.fetchFromServer")}</button>
            <button onClick={() => go("create")} className="inline-flex items-center justify-center gap-1.5 rounded-xl border border-border py-2.5 text-sm text-muted-foreground transition-colors hover:text-foreground"><Upload className="size-3.5" />{t("unlock.switchToCreate")}</button>
          </div>
        )}

        {(current === "unlock" || current === "create" || current === "fetch") && (
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            {current === "create" && (
              <FieldRow icon={<KeyRound className="size-4 shrink-0 text-muted-foreground" />}>
                <input type="text" value={dbName} onChange={(e) => setDbName(e.target.value)} placeholder={t("unlock.vaultName")} className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" />
              </FieldRow>
            )}
            {current === "fetch" && (
              <FieldRow icon={<Server className="size-4 shrink-0 text-muted-foreground" />}>
                <input type="url" value={serverUrl} onChange={(e) => setServerUrl(e.target.value)} placeholder={t("unlock.serverUrl")} autoComplete="url" className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" />
              </FieldRow>
            )}
            {(needsToken || current === "fetch") && (
              <FieldRow icon={<KeyRound className="size-4 shrink-0 text-muted-foreground" />}>
                <input type="password" value={token} onChange={(e) => setToken(e.target.value)} placeholder={t("unlock.serverToken")} autoComplete="off" className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" />
              </FieldRow>
            )}
            <FieldRow icon={<Key className="size-4 shrink-0 text-muted-foreground" />}>
              <input type={showPassword ? "text" : "password"} value={password} onChange={(e) => setPassword(e.target.value)} placeholder={t("unlock.password")} className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" autoComplete={current === "unlock" ? "current-password" : "new-password"} autoFocus />
              <button type="button" onClick={() => setShowPassword(!showPassword)} className="shrink-0 rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground" aria-label={showPassword ? t("unlock.hide") : t("unlock.show")}>{showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</button>
            </FieldRow>
            <button type="submit" disabled={busy} className="flex items-center justify-center gap-2 rounded-xl bg-primary py-2.5 text-sm font-semibold text-primary-foreground transition-all hover:bg-primary/90 active:scale-[0.98] disabled:opacity-50"><Lock className="size-4" />{busy ? "..." : current === "unlock" ? t("unlock.unlock") : current === "create" ? t("unlock.create") : t("unlock.fetch")}</button>
          </form>
        )}

        <div className="mt-6 text-center">
          {web && (
            <button onClick={() => go(mode === "unlock" ? "create" : "unlock")} className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"><Upload className="size-3.5" />{mode === "unlock" ? t("unlock.switchToCreate") : t("unlock.switchToUnlock")}</button>
          )}
          {!web && (current === "fetch" || current === "create") && (
            <button onClick={() => go("choose")} className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"><ArrowLeft className="size-3.5" />{t("unlock.back")}</button>
          )}
        </div>
        {!web && vaultLocation.data && <p className="mt-4 break-all text-center text-[11px] text-muted-foreground">{vaultLocation.data.path}</p>}
      </div>
      <div className="mt-auto flex flex-col items-center gap-4 pt-8"><LanguageSwitcher /><p className="text-xs text-muted-foreground">{t("app.footer")}</p></div>
    </div>
  );
}

function FieldRow({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="glass-card rounded-xl p-1">
      <div className="flex items-center gap-2 rounded-lg bg-muted/50 px-3 py-2">
        {icon}
        {children}
      </div>
    </div>
  );
}
