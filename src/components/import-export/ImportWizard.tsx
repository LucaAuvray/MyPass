import { useState, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { tauriCommand } from "@/lib/tauri";
import { FileUp, ArrowRight, Check, AlertTriangle, Loader2 } from "lucide-react";
import type { ImportedEntry, ImportResult, ParsedEntry, DuplicateGroup, DedupResolution } from "@/types/import";
import { computeDuplicateGroups, decodeImportBytes, findNonDuplicates, vaultEntryToDupEntry, resolveDuplicates } from "@/lib/dedup";
import { DedupDialog } from "@/components/import-export/DedupDialog";

interface ImportWizardProps { open: boolean; onOpenChange: (open: boolean) => void; }

type Step = "pick" | "parsing" | "dedup" | "importing" | "done";

/** Shape of get_entries rows used for the duplicate comparison (EntryInfo sends has_totp). */
type VaultEntry = Parameters<typeof vaultEntryToDupEntry>[0] & { has_totp?: boolean };

export function ImportWizard({ open, onOpenChange }: ImportWizardProps) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [step, setStep] = useState<Step>("pick");
  const [fileName, setFileName] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [ignored, setIgnored] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [parsingProgress, setParsingProgress] = useState(0);

  // Dedup state
  const [duplicateGroups, setDuplicateGroups] = useState<DuplicateGroup[]>([]);
  const [nonDuplicates, setNonDuplicates] = useState<ParsedEntry[]>([]);
  const parsedCountRef = useRef(0);
  const fileInputRef = useRef<HTMLInputElement>(null);

  /** Error codes from mypass-core are translated, anything else shown as is. */
  const describe = (err: unknown, fallbackKey: string) => {
    const msg = err instanceof Error ? err.message : String(err ?? "");
    if (/^[A-Z_]+$/.test(msg) && i18n.exists(`import.errors.${msg}`)) return t(`import.errors.${msg}`);
    return msg || t(fallbackKey);
  };

  // ── Analysis: parse in mypass-core, then compare with the vault ──

  const analyze = async (content: string, name: string) => {
    setStep("parsing");
    setFileName(name);
    setParsingProgress(30);
    try {
      const imported = await tauriCommand<ImportedEntry[]>("parse_import", { content });
      const parsed: ParsedEntry[] = imported.map((e, i) => ({ ...e, tempId: `import-${i}` }));
      parsedCountRef.current = parsed.length;
      setParsingProgress(60);

      const vault = await tauriCommand<VaultEntry[]>("get_entries", {});
      setParsingProgress(80);

      const groups = computeDuplicateGroups(parsed, (vault ?? []).map((v) => vaultEntryToDupEntry({ ...v, hasTotp: v.has_totp })));
      const nonDup = findNonDuplicates(parsed, groups);
      setParsingProgress(100);

      if (groups.length > 0) {
        setDuplicateGroups(groups);
        setNonDuplicates(nonDup);
        setStep("dedup");
      } else {
        await runImport(resolveDuplicates([], nonDup));
      }
    } catch (err: unknown) {
      setError(describe(err, "import.analyzeFailed"));
      setStep("pick");
    }
  };

  // ── Import: send what dedup kept ────────────────────────────────

  const runImport = async (resolution: DedupResolution) => {
    setStep("importing");
    try {
      const res = await tauriCommand<ImportResult>("import_entries", { entries: resolution.resolvedEntries });
      setResult(res);
      // Rows of the file not sent (the vault's copy was kept) count as ignored.
      setIgnored(parsedCountRef.current - resolution.resolvedEntries.length + res.skipped);
      setStep("done");
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    } catch (err: unknown) {
      setError(describe(err, "import.importFailed"));
      setStep("pick");
    }
  };

  const handleDedupConfirm = (resolvedGroups: DuplicateGroup[]) => runImport(resolveDuplicates(resolvedGroups, nonDuplicates));

  const handleDedupCancel = () => {
    setDuplicateGroups([]);
    setNonDuplicates([]);
    setStep("pick");
  };

  // ── File selection ─────────────────────────────────────────────

  /** The browser file picker — works in the Tauri webview and in the PWA. */
  const handlePick = () => {
    setError(null);
    if (fileInputRef.current) {
      fileInputRef.current.value = "";
      fileInputRef.current.click();
    } else {
      setError(t("import.pickerError"));
    }
  };

  const handleFilePicked = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) {
      setError(t("import.noFile"));
      return;
    }
    try {
      await analyze(decodeImportBytes(await file.arrayBuffer()), file.name);
    } catch (err: unknown) {
      setError(t("import.readError", { msg: err instanceof Error ? err.message : String(err) }));
    }
  };

  // ── Reset & Close ──────────────────────────────────────────────

  const reset = () => {
    setStep("pick");
    setFileName(null);
    setResult(null);
    setIgnored(0);
    setError(null);
    setDuplicateGroups([]);
    setNonDuplicates([]);
    setParsingProgress(0);
  };

  const handleClose = () => {
    // Close first, then reset, so the dialog closes before its content changes.
    onOpenChange(false);
    reset();
  };

  // ── Render ─────────────────────────────────────────────────────

  return (
    <>
      <Dialog open={open && step !== "dedup"} onOpenChange={handleClose}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader><DialogTitle>{t("import.title")}</DialogTitle></DialogHeader>

          <input ref={fileInputRef} type="file" accept=".csv,.json" className="hidden" onChange={handleFilePicked} />

          {step === "pick" && (
            <>
              <button
                onClick={handlePick}
                className="flex items-start gap-3 rounded-lg border border-border p-4 text-left hover:border-primary/50 hover:bg-muted/50"
              >
                <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10">
                  <FileUp className="size-4 text-primary" />
                </div>
                <div className="min-w-0">
                  <p className="text-sm font-semibold">{t("import.pickFile")}</p>
                  <p className="text-xs text-muted-foreground">{t("import.pickFileDesc")}</p>
                  <div className="mt-1 flex gap-1">
                    <Badge variant="secondary" className="text-[10px]">.csv</Badge>
                    <Badge variant="secondary" className="text-[10px]">.json</Badge>
                  </div>
                </div>
                <ArrowRight className="ml-auto size-4 shrink-0 text-muted-foreground" />
              </button>
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
              <p className="text-xs text-muted-foreground">{t("dedup.analyzing")}</p>
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
              <div className="space-y-1 text-center">
                <h3 className="text-lg font-bold">{t("import.success")}</h3>
                <p className="text-sm">{t("import.imported", { count: result.imported })}</p>
                <p className="text-sm">{t("import.updated", { count: result.updated })}</p>
                {ignored > 0 && <p className="text-xs text-amber-500">{t("import.skipped", { count: ignored })}</p>}
              </div>
              <Button onClick={handleClose} className="w-full">{t("common.done")}</Button>
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
