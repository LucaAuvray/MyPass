import { useState, useEffect, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { useDatabase } from "@/hooks/useDatabase";
import { syncNowAndRefresh } from "@/hooks/useSync";
import { LanguageSwitcher } from "@/components/shared/LanguageSwitcher";
import { isWebMode, hasWebToken, setWebToken } from "@/lib/tauri";
import {
  Lock,
  Eye,
  EyeOff,
  Key,
  Upload,
  KeyRound,
  Server,
  Download,
  ArrowLeft,
} from "lucide-react";
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
    if (current === "create" && dbName)
      void run(() => createDatabase({ password, name: dbName, encryption: "aes256" }));
    if (current === "fetch" && serverUrl && token)
      void run(() => fetchFromServer({ serverUrl, token, password }));
  };

  const rawError =
    current === "unlock"
      ? openError
      : current === "create"
        ? createError
        : current === "fetch"
          ? fetchError
          : undefined;
  const message = rawError ?? vaultLocation.error?.message;
  const error =
    message && /^[A-Z_]+$/.test(message) && i18n.exists(`unlock.errors.${message}`)
      ? t(`unlock.errors.${message}`)
      : message;

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
    <div className="bg-background flex min-h-screen flex-col items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <img
            src={logo}
            alt="MyPass"
            className="shadow-primary/25 mx-auto mb-4 size-16 rounded-2xl shadow-lg"
          />
          <h1 className="font-display text-foreground text-2xl font-bold tracking-tight">
            {t("app.name")}
          </h1>
          {title && <p className="text-muted-foreground mt-1.5 text-sm">{title}</p>}
        </div>
        {error && (
          <div className="border-destructive/30 bg-destructive/10 text-destructive mb-4 rounded-lg border px-4 py-2.5 text-sm">
            {error}
          </div>
        )}

        {current === "choose" && (
          <div className="flex flex-col gap-3">
            <button
              onClick={() => go("fetch")}
              className="bg-primary text-primary-foreground hover:bg-primary/90 flex items-center justify-center gap-2 rounded-xl py-2.5 text-sm font-semibold transition-all active:scale-[0.98]"
            >
              <Download className="size-4" />
              {t("unlock.fetchFromServer")}
            </button>
            <button
              onClick={() => go("create")}
              className="border-border text-muted-foreground hover:text-foreground inline-flex items-center justify-center gap-1.5 rounded-xl border py-2.5 text-sm transition-colors"
            >
              <Upload className="size-3.5" />
              {t("unlock.switchToCreate")}
            </button>
          </div>
        )}

        {(current === "unlock" || current === "create" || current === "fetch") && (
          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            {current === "create" && (
              <FieldRow icon={<KeyRound className="text-muted-foreground size-4 shrink-0" />}>
                <input
                  type="text"
                  value={dbName}
                  onChange={(e) => setDbName(e.target.value)}
                  placeholder={t("unlock.vaultName")}
                  className="text-foreground placeholder:text-muted-foreground w-full bg-transparent text-sm outline-none"
                />
              </FieldRow>
            )}
            {current === "fetch" && (
              <FieldRow icon={<Server className="text-muted-foreground size-4 shrink-0" />}>
                <input
                  type="url"
                  value={serverUrl}
                  onChange={(e) => setServerUrl(e.target.value)}
                  placeholder={t("unlock.serverUrl")}
                  autoComplete="url"
                  className="text-foreground placeholder:text-muted-foreground w-full bg-transparent text-sm outline-none"
                />
              </FieldRow>
            )}
            {(needsToken || current === "fetch") && (
              <FieldRow icon={<KeyRound className="text-muted-foreground size-4 shrink-0" />}>
                <input
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  placeholder={t("unlock.serverToken")}
                  autoComplete="off"
                  className="text-foreground placeholder:text-muted-foreground w-full bg-transparent text-sm outline-none"
                />
              </FieldRow>
            )}
            <FieldRow icon={<Key className="text-muted-foreground size-4 shrink-0" />}>
              <input
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={t("unlock.password")}
                className="text-foreground placeholder:text-muted-foreground w-full bg-transparent text-sm outline-none"
                autoComplete={current === "unlock" ? "current-password" : "new-password"}
                autoFocus
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="text-muted-foreground hover:bg-secondary hover:text-foreground shrink-0 rounded p-1"
                aria-label={showPassword ? t("unlock.hide") : t("unlock.show")}
              >
                {showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </button>
            </FieldRow>
            <button
              type="submit"
              disabled={busy}
              className="bg-primary text-primary-foreground hover:bg-primary/90 flex items-center justify-center gap-2 rounded-xl py-2.5 text-sm font-semibold transition-all active:scale-[0.98] disabled:opacity-50"
            >
              <Lock className="size-4" />
              {busy
                ? "..."
                : current === "unlock"
                  ? t("unlock.unlock")
                  : current === "create"
                    ? t("unlock.create")
                    : t("unlock.fetch")}
            </button>
          </form>
        )}

        <div className="mt-6 text-center">
          {web && (
            <button
              onClick={() => go(mode === "unlock" ? "create" : "unlock")}
              className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-sm transition-colors"
            >
              <Upload className="size-3.5" />
              {mode === "unlock" ? t("unlock.switchToCreate") : t("unlock.switchToUnlock")}
            </button>
          )}
          {!web && (current === "fetch" || current === "create") && (
            <button
              onClick={() => go("choose")}
              className="text-muted-foreground hover:text-foreground inline-flex items-center gap-1.5 text-sm transition-colors"
            >
              <ArrowLeft className="size-3.5" />
              {t("unlock.back")}
            </button>
          )}
        </div>
        {!web && vaultLocation.data && (
          <p className="text-muted-foreground mt-4 text-center text-[11px] break-all">
            {vaultLocation.data.path}
          </p>
        )}
      </div>
      <div className="mt-auto flex flex-col items-center gap-4 pt-8">
        <LanguageSwitcher />
        <p className="text-muted-foreground text-xs">{t("app.footer")}</p>
      </div>
    </div>
  );
}

function FieldRow({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <div className="glass-card rounded-xl p-1">
      <div className="bg-muted/50 flex items-center gap-2 rounded-lg px-3 py-2">
        {icon}
        {children}
      </div>
    </div>
  );
}
