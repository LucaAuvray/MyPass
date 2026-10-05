import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Eye, EyeOff, Zap, Trash2, CheckCircle2 } from "lucide-react";
import type { DuplicateGroup } from "@/types/import";

interface DedupDialogProps {
  open: boolean;
  groups: DuplicateGroup[];
  nonDuplicateCount: number;
  onConfirm: (resolvedGroups: DuplicateGroup[]) => void;
  onCancel: () => void;
}

export function DedupDialog({
  open,
  groups,
  nonDuplicateCount,
  onConfirm,
  onCancel,
}: DedupDialogProps) {
  const { t } = useTranslation();
  const [localGroups, setLocalGroups] = useState<DuplicateGroup[]>(() =>
    groups.map((g) => ({ ...g, selectedIndex: g.selectedIndex })),
  );
  const [revealedPasswords, setRevealedPasswords] = useState<Record<string, boolean>>({});

  const toggleReveal = useCallback((id: string) => {
    setRevealedPasswords((prev) => ({ ...prev, [id]: !prev[id] }));
  }, []);

  const selectEntry = useCallback((groupId: string, entryIndex: number) => {
    setLocalGroups((prev) =>
      prev.map((g) => (g.id === groupId ? { ...g, selectedIndex: entryIndex } : g)),
    );
  }, []);

  const selectAllImports = useCallback(() => {
    setLocalGroups((prev) =>
      prev.map((g) => {
        const importIdx = g.entries.findIndex((e) => e.source === "import");
        return { ...g, selectedIndex: importIdx >= 0 ? importIdx : 0 };
      }),
    );
  }, []);

  const handleConfirm = useCallback(() => {
    onConfirm(localGroups);
  }, [localGroups, onConfirm]);

  const allResolved = localGroups.every((g) => g.selectedIndex >= 0);
  const totalKept = nonDuplicateCount + localGroups.length;
  const totalDiscarded = localGroups.reduce((sum, g) => sum + g.entries.length - 1, 0);

  return (
    <Dialog open={open} onOpenChange={() => onCancel()}>
      <DialogContent className="max-h-[90vh] max-w-5xl overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Trash2 className="size-5 text-amber-500" />
            {t("dedup.title")}
          </DialogTitle>
        </DialogHeader>

        <p className="text-muted-foreground text-sm">
          {t("dedup.description", { count: localGroups.length })}
        </p>

        {/* Quick actions */}
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={selectAllImports}>
            <Zap className="mr-1 size-3" />
            {t("dedup.selectAllImports")}
          </Button>
        </div>

        {/* Groups */}
        <div className="space-y-6">
          {localGroups.map((group) => (
            <DuplicateGroupCard
              key={group.id}
              group={group}
              revealedPasswords={revealedPasswords}
              onToggleReveal={toggleReveal}
              onSelect={(idx) => selectEntry(group.id, idx)}
              t={t}
            />
          ))}
        </div>

        {/* Summary */}
        <div className="border-border flex items-center justify-between border-t pt-4">
          <div className="flex items-center gap-2 text-sm">
            <CheckCircle2 className="size-4 text-green-500" />
            <span>{t("dedup.summary", { kept: totalKept, discarded: totalDiscarded })}</span>
          </div>
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={onCancel}>
              {t("dedup.cancel")}
            </Button>
            <Button onClick={handleConfirm} disabled={!allResolved}>
              {t("dedup.confirm")}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// ── Duplicate Group Card ─────────────────────────────────────────

interface DuplicateGroupCardProps {
  group: DuplicateGroup;
  revealedPasswords: Record<string, boolean>;
  onToggleReveal: (id: string) => void;
  onSelect: (index: number) => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}

function DuplicateGroupCard({
  group,
  revealedPasswords,
  onToggleReveal,
  onSelect,
  t,
}: DuplicateGroupCardProps) {
  const isInternal = group.entries.every((e) => e.source === "import");

  return (
    <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-4">
      {/* Group header */}
      <div className="mb-3 flex items-center gap-2">
        <Badge variant="outline" className="border-amber-500/50 text-amber-600">
          {isInternal ? t("dedup.sourceInternal") : t("dedup.title")}
        </Badge>
        <span className="text-sm font-medium">{group.matchKey}</span>
        {isInternal && (
          <span className="text-muted-foreground text-xs">
            {t("dedup.internalLabel", { count: group.entries.length })}
          </span>
        )}
      </div>

      {/* Entry cards */}
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
        {group.entries.map((entry, idx) => {
          const entryId = entry.uuid ?? entry.tempId ?? `entry-${idx}`;
          const isSelected = group.selectedIndex === idx;
          const isRevealed = revealedPasswords[entryId] ?? false;

          return (
            <div
              key={entryId}
              className={`relative cursor-pointer rounded-lg border-2 p-3 transition-all ${
                isSelected ? "border-primary bg-primary/5" : "border-border hover:border-primary/30"
              }`}
              onClick={() => onSelect(idx)}
            >
              {/* Radio indicator */}
              <div className="mb-2 flex items-center justify-between">
                <div
                  className={`flex size-5 items-center justify-center rounded-full border-2 ${
                    isSelected ? "border-primary" : "border-muted-foreground/30"
                  }`}
                >
                  {isSelected && <div className="bg-primary size-2.5 rounded-full" />}
                </div>
                <Badge
                  variant={entry.source === "import" ? "default" : "secondary"}
                  className="text-[10px]"
                >
                  {entry.source === "import" ? t("dedup.sourceImport") : t("dedup.sourceVault")}
                </Badge>
              </div>

              {/* Fields */}
              <div className="space-y-1.5 text-sm">
                <FieldRow label={t("dedup.fieldTitle")} value={entry.title} />
                <FieldRow label={t("dedup.fieldUsername")} value={entry.username} />
                <FieldRow label={t("dedup.fieldUrl")} value={entry.url} />

                {/* Password with reveal */}
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground text-xs">{t("dedup.fieldPassword")}</span>
                  <div className="flex items-center gap-1">
                    <span className="font-mono text-xs">
                      {isRevealed ? entry.password : "••••••••"}
                    </span>
                    <button
                      type="button"
                      className="hover:bg-muted inline-flex size-5 items-center justify-center rounded"
                      onClick={(e) => {
                        e.stopPropagation();
                        onToggleReveal(entryId);
                      }}
                      aria-label={t("dedup.revealPassword")}
                    >
                      {isRevealed ? (
                        <EyeOff className="text-muted-foreground size-3" />
                      ) : (
                        <Eye className="text-muted-foreground size-3" />
                      )}
                    </button>
                  </div>
                </div>

                {entry.notes && <FieldRow label={t("dedup.fieldNotes")} value={entry.notes} />}
                {!entry.notes && (
                  <FieldRow label={t("dedup.fieldNotes")} value={t("dedup.noNotes")} muted />
                )}

                {entry.tags.length > 0 && (
                  <FieldRow label={t("dedup.fieldTags")} value={entry.tags.join(", ")} />
                )}
                {entry.tags.length === 0 && (
                  <FieldRow label={t("dedup.fieldTags")} value={t("dedup.noTags")} muted />
                )}

                <FieldRow
                  label={t("dedup.fieldTotp")}
                  value={entry.totp || entry.hasTotp ? t("dedup.hasTotp") : t("dedup.noTotp")}
                  muted={!(entry.totp || entry.hasTotp)}
                />

                {entry.created && (
                  <FieldRow
                    label={t("dedup.fieldCreated")}
                    value={entry.created.substring(0, 10)}
                  />
                )}
                {entry.modified && (
                  <FieldRow
                    label={t("dedup.fieldModified")}
                    value={entry.modified.substring(0, 10)}
                  />
                )}

                {Object.keys(entry.customFields).length > 0 && (
                  <FieldRow
                    label={t("dedup.fieldCustomFields")}
                    value={Object.entries(entry.customFields)
                      .map(([k, v]) => `${k}: ${v}`)
                      .join(", ")}
                  />
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Field Row ────────────────────────────────────────────────────

function FieldRow({ label, value, muted }: { label: string; value: string; muted?: boolean }) {
  return (
    <div className="flex items-start justify-between gap-2">
      <span className="text-muted-foreground shrink-0 text-xs">{label}</span>
      <span className={`text-right text-xs break-all ${muted ? "text-muted-foreground/50" : ""}`}>
        {value || "—"}
      </span>
    </div>
  );
}
