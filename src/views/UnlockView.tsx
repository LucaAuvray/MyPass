import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { useDatabase } from "@/hooks/useDatabase";
import { syncNowAndRefresh } from "@/hooks/useSync";
import { LanguageSwitcher } from "@/components/shared/LanguageSwitcher";
import { isWebMode, hasWebToken, setWebToken } from "@/lib/tauri";
import { Lock, Eye, EyeOff, Key, Upload, KeyRound } from "lucide-react";
import logo from "@/assets/logo.png";

export default function UnlockView() {
  const { t, i18n } = useTranslation();
  const { openDatabase, createDatabase, isOpening, isCreating, openError, createError } = useDatabase();
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [mode, setMode] = useState<"unlock" | "create">("unlock");
  const dbPath = "mypass-vault.kdbx";
  const [dbName, setDbName] = useState("");
  const web = isWebMode();
  const [token, setToken] = useState("");
  const needsToken = web && !hasWebToken();
  const queryClient = useQueryClient();

  // Update vault name when language changes
  useEffect(() => {
    setDbName(t("unlock.vaultName"));
  }, [i18n.language, t]);

  const handleUnlock = async (e: React.FormEvent) => {
    e.preventDefault();
    if (web && token) setWebToken(token);
    if (!password) return;
    if (mode === "unlock") {
      await openDatabase({ path: dbPath, password });
      void syncNowAndRefresh(queryClient).catch(() => {});
    }
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (web && token) setWebToken(token);
    if (!password || !dbName) return;
    await createDatabase({ path: dbPath, password, name: dbName, encryption: "aes256" });
    void syncNowAndRefresh(queryClient).catch(() => {});
  };

  const error = mode === "unlock" ? openError : createError;

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-background p-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <img src={logo} alt="MyPass" className="mx-auto mb-4 size-16 rounded-2xl shadow-lg shadow-primary/25" />
          <h1 className="font-display text-2xl font-bold tracking-tight text-foreground">{t("app.name")}</h1>
          <p className="mt-1.5 text-sm text-muted-foreground">{mode === "unlock" ? t("unlock.title") : t("unlock.createTitle")}</p>
        </div>
        {error && <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/10 px-4 py-2.5 text-sm text-destructive">{error}</div>}
        <form onSubmit={mode === "unlock" ? handleUnlock : handleCreate} className="flex flex-col gap-4">
          {mode === "create" && (
            <div className="glass-card rounded-xl p-1">
              <div className="flex items-center gap-2 rounded-lg bg-muted/50 px-3 py-2">
                <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                <input type="text" value={dbName} onChange={(e) => setDbName(e.target.value)} placeholder={t("unlock.vaultName")} className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" />
              </div>
            </div>
          )}
          {needsToken && (
            <div className="glass-card rounded-xl p-1">
              <div className="flex items-center gap-2 rounded-lg bg-muted/50 px-3 py-2">
                <KeyRound className="size-4 shrink-0 text-muted-foreground" />
                <input
                  type="password"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  placeholder={t("unlock.serverToken")}
                  autoComplete="off"
                  className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
                />
              </div>
            </div>
          )}
          <div className="glass-card rounded-xl p-1">
            <div className="flex items-center gap-2 rounded-lg bg-muted/50 px-3 py-2">
              <Key className="size-4 shrink-0 text-muted-foreground" />
              <input type={showPassword ? "text" : "password"} value={password} onChange={(e) => setPassword(e.target.value)} placeholder={t("unlock.password")} className="w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground" autoComplete={mode === "create" ? "new-password" : "current-password"} autoFocus />
              <button type="button" onClick={() => setShowPassword(!showPassword)} className="shrink-0 rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground" aria-label={showPassword ? t("unlock.hide") : t("unlock.show")}>{showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}</button>
            </div>
          </div>
          <button type="submit" disabled={isOpening || isCreating} className="flex items-center justify-center gap-2 rounded-xl bg-primary py-2.5 text-sm font-semibold text-primary-foreground transition-all hover:bg-primary/90 active:scale-[0.98] disabled:opacity-50"><Lock className="size-4" />{isOpening || isCreating ? "..." : mode === "unlock" ? t("unlock.unlock") : t("unlock.create")}</button>
        </form>
        <div className="mt-6 text-center">
          <button onClick={() => { setMode(mode === "unlock" ? "create" : "unlock"); setPassword(""); }} className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"><Upload className="size-3.5" />{mode === "unlock" ? t("unlock.switchToCreate") : t("unlock.switchToUnlock")}</button>
        </div>
      </div>
      <div className="mt-auto flex flex-col items-center gap-4 pt-8"><LanguageSwitcher /><p className="text-xs text-muted-foreground">{t("app.footer")}</p></div>
    </div>
  );
}
