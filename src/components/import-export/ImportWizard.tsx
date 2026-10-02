import { useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { tauriCommand } from "@/lib/tauri";
import { FileSpreadsheet, Shield, Lock, Chrome, Apple, FileJson, ArrowRight, Check, AlertTriangle, Loader2 } from "lucide-react";
import type { ImportFormat, ImportResult, ParsedEntry, DuplicateGroup } from "@/types/import";
import { parseCsvToEntries, parseJsonToEntries, computeDuplicateGroups, findNonDuplicates, vaultEntryToDupEntry, resolveDuplicates } from "@/lib/dedup";
import { DedupDialog } from "@/components/import-export/DedupDialog";

interface ImportWizardProps { open: boolean; onOpenChange: (open: boolean) => void; }

type Step = "format" | "parsing" | "dedup" | "importing" | "done";

/** Returns the file extension filter for a format. */
function fileExt(fmt: ImportFormat): string {
  if (fmt === "1pux") return "1pux";
  if (fmt === "bitwarden" || fmt === "protonpass") return "json";
  return "csv";
}

/** Parse file content into ParsedEntry[] based on format. */
function parseFileContent(content: string, fmt: ImportFormat): ParsedEntry[] {
  if (fmt === "1pux" || fmt === "bitwarden" || fmt === "protonpass") {
    return parseJsonToEntries(content, fmt);
  }
  return parseCsvToEntries(content, fmt);
}

export function ImportWizard({ open, onOpenChange }: ImportWizardProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [step, setStep] = useState<Step>("format");
  const [fileName, setFileName] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Dedup state
  const [duplicateGroups, setDuplicateGroups] = useState<DuplicateGroup[]>([]);
  const [nonDuplicates, setNonDuplicates] = useState<ParsedEntry[]>([]);
  const [parsingProgress, setParsingProgress] = useState(0);

  // Hidden file input for browser fallback
  const fileInputRef = useRef<HTMLInputElement>(null);
  const pendingFormatRef = useRef<ImportFormat | null>(null);

  const FORMATS = [
    { id: "csv" as ImportFormat, name: "CSV", icon: FileSpreadsheet, ext: ".csv" },
    { id: "google" as ImportFormat, name: "Google", icon: Chrome, ext: ".csv" },
    { id: "apple" as ImportFormat, name: "Apple", icon: Apple, ext: ".csv" },
    { id: "1pux" as ImportFormat, name: "1Password", icon: Shield, ext: ".1pux" },
    { id: "bitwarden" as ImportFormat, name: "Bitwarden", icon: Lock, ext: ".json" },
    { id: "protonpass" as ImportFormat, name: "Proton Pass", icon: FileJson, ext: ".json" },
  ];

  // ── Analysis phase: parse content, detect duplicates ────────────

  const analyzeAndImport = async (fmt: ImportFormat, content: string, name: string) => {
    setStep("parsing");
    setFileName(name);
    setParsingProgress(30);

    try {
      // 1. Parse the file
      const parsed = parseFileContent(content, fmt);
      setParsingProgress(60);

      if (parsed.length === 0) {
        setError("Aucune entrée trouvée dans le fichier");
        setStep("format");
        return;
      }

      // 2. Get existing vault entries for dedup comparison
      let vaultDupEntries: ReturnType<typeof vaultEntryToDupEntry>[] = [];
      try {
        const vaultEntries = await tauriCommand<Array<{
          uuid: string; title: string; username: string; password: string;
          url: string; notes?: string; tags?: string[];
          customFields?: Record<string, string>; created?: string; modified?: string;
        }>>("get_entries_for_dedup");
        if (vaultEntries) {
          vaultDupEntries = vaultEntries.map(vaultEntryToDupEntry);
        }
      } catch {
        // In dev mode, use get_entries fallback
        try {
          const entries = await tauriCommand<Array<{
            uuid: string; title: string; username: string; password: string;
            url: string; notes?: string; tags?: string[];
            customFields?: Record<string, string>; created?: string; modified?: string;
          }>>("get_entries");
          if (entries) {
            vaultDupEntries = entries.map(vaultEntryToDupEntry);
          }
        } catch {
          // No vault entries available
        }
      }

      setParsingProgress(80);

      // 3. Compute duplicate groups
      const groups = computeDuplicateGroups(parsed, vaultDupEntries);
      const nonDup = findNonDuplicates(parsed, groups);

      setParsingProgress(100);

      // 4. If duplicates found → show dedup dialog
      if (groups.length > 0) {
        setDuplicateGroups(groups);
        setNonDuplicates(nonDup);
        setStep("dedup");
      } else {
        // No duplicates → import directly
        await executeDirectImport(fmt, parsed, name);
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Échec de l'analyse");
      setStep("format");
    }
  };

  // ── Direct import (no dedup needed) ────────────────────────────

  const executeDirectImport = async (fmt: ImportFormat, entries: ParsedEntry[], _name: string) => {
    setStep("importing");
    setLoading(true);

    try {
      const res = await tauriCommand<ImportResult>("import_entries", {
        entries: entries.map((e) => ({
          title: e.title,
          username: e.username,
          password: e.password,
          url: e.url,
          notes: e.notes,
          tags: e.tags,
          customFields: e.customFields,
          totp: e.totp,
        })),
        format: fmt,
      });
      setResult(res);
      setStep("done");
      // Invalidate entries query to refresh the list
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Échec de l'import");
      setStep("format");
    } finally {
      setLoading(false);
    }
  };

  // ── Dedup confirmed → import resolved entries ──────────────────

  const handleDedupConfirm = async (resolvedGroups: DuplicateGroup[]) => {
    // Combine resolved duplicates with non-duplicate entries
    const resolution = resolveDuplicates(resolvedGroups, nonDuplicates);
    setStep("importing");
    setLoading(true);

    try {
      const res = await tauriCommand<ImportResult>("import_entries", {
        entries: resolution.resolvedEntries,
        format: pendingFormatRef.current ?? "csv",
      });
      setResult({ ...res, duplicates: resolution.totalDiscarded });
      setStep("done");
      // Invalidate entries query to refresh the list
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg || "Échec de l'import");
      setStep("format");
    } finally {
      setLoading(false);
    }
  };

  // ── Dedup cancelled ────────────────────────────────────────────

  const handleDedupCancel = () => {
    setDuplicateGroups([]);
    setNonDuplicates([]);
    setStep("format");
  };

  // ── File selection ─────────────────────────────────────────────

  /** Open the browser file picker — works in Tauri webview AND browser. */
  const handleSelectAndImport = (fmt: ImportFormat) => {
    setError(null);
    pendingFormatRef.current = fmt;

    if (fileInputRef.current) {
      fileInputRef.current.value = "";
      fileInputRef.current.accept = `.${fileExt(fmt)}`;
      fileInputRef.current.click();
    } else {
      setError(t("import.pickerError"));
    }
  };

  /** HTML file input onChange → read content & analyze. */
  const handleFilePicked = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    const fmt = pendingFormatRef.current;
    if (!file || !fmt) {
      setError(t("import.noFile"));
      return;
    }

    try {
      const content = await file.text();
      await analyzeAndImport(fmt, content, file.name);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(t("import.readError", { msg }));
    }
  };

  // ── Reset & Close ──────────────────────────────────────────────

  const reset = () => {
    setStep("format");
    setFileName(null);
    setResult(null);
    setError(null);
    setDuplicateGroups([]);
    setNonDuplicates([]);
    setParsingProgress(0);
    pendingFormatRef.current = null;
  };

  const handleClose = () => {
    // BUG #2 fix: close first, then reset (so dialog closes before content changes)
    onOpenChange(false);
    reset();
  };

  // ── Render ─────────────────────────────────────────────────────

  return (
    <>
      <Dialog open={open && step !== "dedup"} onOpenChange={handleClose}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader><DialogTitle>{t("import.title")}</DialogTitle></DialogHeader>

          {/* Hidden file input for browser fallback */}
          <input
            ref={fileInputRef}
            type="file"
            className="hidden"
            onChange={handleFilePicked}
          />

          {step === "format" && (
            <>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                {FORMATS.map((fmt) => (
                  <button
                    key={fmt.id}
                    onClick={() => handleSelectAndImport(fmt.id)}
                    disabled={loading}
                    className="flex items-start gap-3 rounded-lg border border-border p-4 text-left hover:border-primary/50 hover:bg-muted/50 disabled:opacity-50"
                  >
                    <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                      <fmt.icon className="size-4 text-primary" />
                    </div>
                    <div className="min-w-0">
                      <p className="text-sm font-semibold">{fmt.name}</p>
                      <Badge variant="secondary" className="mt-1 text-[10px]">{fmt.ext}</Badge>
                    </div>
                    <ArrowRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
                  </button>
                ))}
              </div>
              {error && <p className="text-xs text-destructive">{error}</p>}
              <div className="flex items-center gap-2 pt-2 text-[10px] text-muted-foreground">
                <AlertTriangle className="size-3" />
                {t("import.warning")}
              </div>
            </>
          )}

          {step === "parsing" && (
            <div className="flex flex-col items-center gap-4 py-8">
              <Loader2 className="size-10 animate-spin text-primary" />
              <p className="text-sm">
                {fileName ? t("import.analyzingNamed", { file: fileName }) : t("import.analyzingFile")}
              </p>
              <Progress value={parsingProgress} className="w-full" />
              <p className="text-xs text-muted-foreground">
                {t("dedup.analyzing")}
              </p>
            </div>
          )}

          {step === "importing" && (
            <div className="flex flex-col items-center gap-4 py-8">
              <Loader2 className="size-10 animate-spin text-primary" />
              <p className="text-sm">
                {fileName ? t("import.importingNamed", { file: fileName }) : t("import.importingFile")}
              </p>
              <Progress value={50} className="w-full" />
            </div>
          )}

          {step === "done" && result && (
            <div className="flex flex-col items-center gap-4 py-8">
              <div className="flex size-16 items-center justify-center rounded-2xl bg-green-500/10">
                <Check className="size-8 text-green-500" />
              </div>
              <div className="text-center space-y-1">
                <h3 className="text-lg font-bold">{t("import.success")}</h3>
                <p className="text-sm">{t("import.imported", { count: result.imported })}</p>
                {result.duplicates > 0 && (
                  <p className="text-xs text-amber-500">{t("import.duplicatesRemoved", { count: result.duplicates })}</p>
                )}
              </div>
              <Button onClick={handleClose} className="w-full">
                {t("common.done")}
              </Button>
            </div>
          )}
        </DialogContent>
      </Dialog>

      {/* Dedup dialog: conditionally rendered so it unmounts when done */}
      {step === "dedup" && (
        <DedupDialog
          open={true}
          groups={duplicateGroups}
          nonDuplicateCount={nonDuplicates.length}
          onConfirm={handleDedupConfirm}
          onCancel={handleDedupCancel}
        />
      )}
    </>
  );
}
