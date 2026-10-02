import { useTranslation } from "react-i18next";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { useEntriesStore } from "@/stores/entriesStore";
import { estimateStrength } from "@/lib/password-strength";
import { StrengthBadge } from "@/components/security/StrengthMeter";
import { HibpCheck } from "@/components/security/HibpCheck";
import { AlertTriangle, ShieldCheck, Clock, RefreshCw, TrendingUp, Zap } from "lucide-react";

export function SecurityView() {
  const { t } = useTranslation();
  const entries = useEntriesStore((s) => s.entries);

  const analyzed = entries.map((e) => ({ ...e, strength: estimateStrength(e.password || "") }));
  const weak = analyzed.filter((e) => e.strength.score <= 2);
  const fair = analyzed.filter((e) => e.strength.score === 3);
  const strong = analyzed.filter((e) => e.strength.score >= 4);
  const reused = findReused(analyzed);
  const old = analyzed.filter((e) => { if (!e.modified) return false; try { return Date.now() - new Date(e.modified).getTime() > 90 * 86400000; } catch { return false; } });

  const healthScore = entries.length > 0 ? Math.round(((strong.length * 100 + fair.length * 50) / entries.length) * (1 - (reused.length + old.length) / (entries.length * 2 || 1))) : 100;
  const scoreLabel = healthScore >= 80 ? t("security.great") : healthScore >= 50 ? t("security.fair") : t("security.needsWork");
  const scoreDesc = healthScore >= 80 ? t("security.greatDesc") : healthScore >= 50 ? t("security.fairDesc") : t("security.needsWorkDesc");
  const scoreColor = healthScore >= 80 ? "#10B981" : healthScore >= 50 ? "#F59E0B" : "#EF4444";

  return (
    <div className="space-y-6 p-4 md:p-6">
      <div><h1 className="font-display text-2xl font-bold tracking-tight">{t("security.title")}</h1><p className="mt-1 text-sm text-muted-foreground">{t("security.subtitle")}</p></div>

      <Card className="overflow-hidden"><CardContent className="p-6"><div className="flex items-center gap-6">
        <div className="relative flex size-20 shrink-0 items-center justify-center"><svg className="size-full -rotate-90" viewBox="0 0 80 80"><circle cx="40" cy="40" r="34" fill="none" stroke="currentColor" strokeWidth="6" className="text-muted" /><circle cx="40" cy="40" r="34" fill="none" strokeWidth="6" strokeLinecap="round" stroke={scoreColor} strokeDasharray={`${(healthScore / 100) * 213.6} 213.6`} /></svg><span className="absolute font-display text-xl font-bold">{healthScore}</span></div>
        <div><h3 className="text-lg font-bold">{scoreLabel}</h3><p className="text-sm text-muted-foreground">{scoreDesc}</p>
          <div className="mt-3 flex flex-wrap gap-2"><Badge variant="secondary">{entries.length} {t("security.total").toLowerCase()}</Badge><Badge variant="secondary" className="text-green-600 bg-green-50 dark:bg-green-950">{strong.length} {t("security.strong").toLowerCase()}</Badge>{weak.length > 0 && <Badge variant="secondary" className="text-destructive bg-destructive/10">{weak.length} {t("security.weak").toLowerCase()}</Badge>}{reused.length > 0 && <Badge variant="secondary" className="text-amber-600 bg-amber-50 dark:bg-amber-950">{reused.length} {t("security.reused").toLowerCase()}</Badge>}</div>
        </div>
      </div></CardContent></Card>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard title={t("security.total")} value={entries.length} icon={<ShieldCheck className="size-5" />} subtitle={t("security.stored")} />
        <StatCard title={t("security.weak")} value={weak.length} icon={<AlertTriangle className="size-5" />} variant="destructive" subtitle={t("security.scoreDesc")} highlight={weak.length > 0} />
        <StatCard title={t("security.reused")} value={reused.length} icon={<RefreshCw className="size-5" />} variant="warning" subtitle={t("security.sharedDesc")} highlight={reused.length > 0} />
        <StatCard title={t("security.old")} value={old.length} icon={<Clock className="size-5" />} variant="muted" subtitle={t("security.rotationDesc")} />
      </div>

      <Tabs defaultValue="overview">
        <TabsList>
          <TabsTrigger value="overview" className="gap-1.5"><TrendingUp className="size-3.5" /> {t("security.overview")}</TabsTrigger>
          <TabsTrigger value="weak" className="gap-1.5"><AlertTriangle className="size-3.5" /> {t("security.weak")} ({weak.length})</TabsTrigger>
          <TabsTrigger value="reused" className="gap-1.5"><RefreshCw className="size-3.5" /> {t("security.reused")} ({reused.length})</TabsTrigger>
          <TabsTrigger value="check" className="gap-1.5"><Zap className="size-3.5" /> {t("security.checkBreach")}</TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="mt-4">
          {entries.length === 0 ? <Card><CardContent className="py-12 text-center text-muted-foreground"><ShieldCheck className="mx-auto mb-3 size-8" /><p>{t("security.noPasswords")}</p></CardContent></Card> : (
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card><CardHeader className="pb-2"><CardTitle className="text-sm">{t("security.distribution")}</CardTitle></CardHeader><CardContent className="space-y-3"><StrengthRow label={t("security.veryStrong")} count={analyzed.filter((e) => e.strength.score === 5).length} color="#7C3AED" total={entries.length} /><StrengthRow label={t("security.strong")} count={strong.filter((e) => e.strength.score === 4).length} color="#2563EB" total={entries.length} /><StrengthRow label={t("security.good")} count={fair.length} color="#10B981" total={entries.length} /><StrengthRow label={t("security.weak")} count={weak.length} color="#EF4444" total={entries.length} /></CardContent></Card>
              <Card><CardHeader className="pb-2"><CardTitle className="text-sm">{t("security.actions")}</CardTitle></CardHeader><CardContent className="space-y-2">
                {weak.length > 0 && <ActionItem icon={<AlertTriangle className="size-4 text-destructive" />} title={t("security.updateWeak", { count: weak.length })} description={t("security.updateWeakDesc")} />}
                {reused.length > 0 && <ActionItem icon={<RefreshCw className="size-4 text-amber-500" />} title={t("security.changeReused", { count: reused.length })} description={t("security.changeReusedDesc")} />}
                {old.length > 0 && <ActionItem icon={<Clock className="size-4 text-muted-foreground" />} title={t("security.rotateOld", { count: old.length })} description={t("security.rotateOldDesc")} />}
                {weak.length === 0 && reused.length === 0 && old.length === 0 && <p className="py-4 text-center text-sm text-muted-foreground">{t("security.noIssues")}</p>}
              </CardContent></Card>
            </div>
          )}
        </TabsContent>
        <TabsContent value="weak" className="mt-4">{weak.length === 0 ? <Card><CardContent className="py-8 text-center text-sm text-muted-foreground">{t("security.noWeak")}</CardContent></Card> : <PasswordList entries={weak} showStrength />}</TabsContent>
        <TabsContent value="reused" className="mt-4">{reused.length === 0 ? <Card><CardContent className="py-8 text-center text-sm text-muted-foreground">{t("security.noReused")}</CardContent></Card> : <PasswordList entries={reused} />}</TabsContent>
        <TabsContent value="check" className="mt-4"><HibpCheck /></TabsContent>
      </Tabs>
    </div>
  );
}

function StatCard({ title, value, icon, variant = "default", subtitle, highlight }: { title: string; value: number; icon: React.ReactNode; variant?: "default" | "destructive" | "warning" | "muted"; subtitle?: string; highlight?: boolean }) {
  const cm = { default: "text-primary", destructive: "text-destructive", warning: "text-amber-500", muted: "text-muted-foreground" };
  return <Card className={highlight && variant === "destructive" ? "border-destructive/30" : undefined}><CardContent className="flex items-center gap-4 p-4"><div className={cm[variant]}>{icon}</div><div><p className="text-2xl font-bold text-foreground">{value}</p><p className="text-xs text-muted-foreground">{title}</p>{subtitle && <p className="text-[10px] text-muted-foreground">{subtitle}</p>}</div></CardContent></Card>;
}
function StrengthRow({ label, count, color, total }: { label: string; count: number; color: string; total: number }) {
  const pct = total > 0 ? (count / total) * 100 : 0;
  return <div className="space-y-1"><div className="flex items-center justify-between text-xs"><span>{label}</span><span className="tabular-nums text-muted-foreground">{count}</span></div><Progress value={pct} className="h-1.5" style={{ "--progress-color": color } as React.CSSProperties} /></div>;
}
function ActionItem({ icon, title, description }: { icon: React.ReactNode; title: string; description: string }) {
  return <div className="flex items-start gap-3 rounded-lg border border-border p-3"><div className="mt-0.5">{icon}</div><div><p className="text-sm font-medium">{title}</p><p className="text-xs text-muted-foreground">{description}</p></div></div>;
}
function PasswordList({ entries, showStrength }: { entries: { uuid: string; title: string; username: string; strength: ReturnType<typeof estimateStrength> }[]; showStrength?: boolean }) {
  const { t } = useTranslation();
  return <Card><CardContent className="divide-y divide-border p-0">{entries.map((e) => <div key={e.uuid} className="flex items-center justify-between px-4 py-3"><div className="min-w-0"><p className="truncate text-sm font-medium">{e.title}</p><p className="truncate text-xs text-muted-foreground">{e.username}</p></div>{showStrength && <StrengthBadge score={e.strength.score} label={e.strength.label} />}<Button variant="ghost" size="sm" className="ml-2 h-7 text-xs">{t("common.update")}</Button></div>)}</CardContent></Card>;
}
function findReused<T extends { password: string; uuid: string }>(entries: T[]): T[] { const s = new Map<string, string>(); const r = new Map<string, boolean>(); for (const e of entries) { if (!e.password || e.password.length < 4) continue; if (s.has(e.password)) r.set(e.password, true); else s.set(e.password, e.uuid); } return entries.filter((e) => r.has(e.password)); }
