import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { tauriCommand } from "@/lib/tauri";
import { FileSpreadsheet, FileJson, Download, AlertTriangle } from "lucide-react";
import { useEntriesStore } from "@/stores/entriesStore";
import type { ExportFormat } from "@/types/import";

interface ExportDialogProps { open: boolean; onOpenChange: (open: boolean) => void; }
const FORMATS: { id: ExportFormat; name: string; icon: React.ElementType; ext: string }[] = [
  { id: "json", name: "JSON", icon: FileJson, ext: ".json" },
  { id: "csv", name: "CSV", icon: FileSpreadsheet, ext: ".csv" },
];

export function ExportDialog({ open, onOpenChange }: ExportDialogProps) {
  const { t } = useTranslation();
  const entries = useEntriesStore((s) => s.entries);
  const [format, setFormat] = useState<ExportFormat>("json");
  // Unticked entries, not ticked ones: "everything" stays the default even when
  // the entries load after this dialog mounted (it lives in the sidebar).
  const [deselected, setDeselected] = useState<Set<string>>(new Set());
  const [exporting, setExporting] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selected = entries.filter((e) => !deselected.has(e.uuid)).map((e) => e.uuid);
  const selectAll = selected.length === entries.length;
  const isSelected = (uuid: string) => !deselected.has(uuid);
  const toggleEntry = (uuid: string) => { const n = new Set(deselected); if (n.has(uuid)) { n.delete(uuid); } else { n.add(uuid); } setDeselected(n); };
  const toggleAll = () => setDeselected(selectAll ? new Set(entries.map((e) => e.uuid)) : new Set());

  const handleExport = async () => {
    setExporting(true); setError(null);
    try {
      const fileName = `mypass-export-${new Date().toISOString().slice(0, 10)}.${format}`;
      // An empty list means "all" to export_entries.
      const uuids = selectAll ? [] : selected;
      const { saved } = await tauriCommand<{ saved: boolean }>("export_entries", { format, uuids, fileName });
      // Cancelled "Save as": stay on the dialog, nothing to report.
      if (saved) setDone(true);
    } catch (err: unknown) { setError(err instanceof Error && err.message ? err.message : t("export.failed")); }
    finally { setExporting(false); }
  };

  const reset = () => { setDone(false); setError(null); };
  const handleClose = () => { reset(); onOpenChange(false); };

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader><DialogTitle>{t("export.title")}</DialogTitle><DialogDescription>{t("export.description")}</DialogDescription></DialogHeader>
        {!done ? (<>
          <div className="space-y-1.5"><Label className="text-xs">{t("export.format")}</Label>
            <div className="grid grid-cols-2 gap-2">{FORMATS.map((fmt) => (
              <button key={fmt.id} onClick={() => setFormat(fmt.id)} className={`flex items-center gap-2 rounded-lg border px-3 py-2 text-left text-sm ${format === fmt.id ? "border-primary bg-primary/5 text-primary" : "border-border hover:border-primary/30"}`}>
                <fmt.icon className="size-4 shrink-0" /><div><span className="font-medium">{fmt.name}</span><span className="ml-1 text-[10px] text-muted-foreground">{fmt.ext}</span></div>
              </button>
            ))}</div>
            {format === "csv" && <p className="text-[11px] text-muted-foreground">{t("export.csvHint")}</p>}
          </div>
          <div className="space-y-2">
            <div className="flex items-center gap-2"><Checkbox id="sel-all" checked={selectAll} onCheckedChange={toggleAll} /><Label htmlFor="sel-all" className="text-sm">{t("export.selectAll", { count: entries.length })}</Label></div>
            {entries.length > 0 && (<div className="max-h-40 overflow-y-auto rounded-lg border border-border">{entries.slice(0, 20).map((e) => (<div key={e.uuid} className="flex items-center gap-2 border-b border-border px-3 py-2 last:border-0"><Checkbox checked={isSelected(e.uuid)} onCheckedChange={() => toggleEntry(e.uuid)} /><div className="min-w-0"><p className="truncate text-sm font-medium">{e.title}</p><p className="truncate text-xs text-muted-foreground">{e.username}</p></div></div>))}</div>)}
          </div>
          {error && <p className="text-xs text-destructive">{error}</p>}
          <div className="flex items-start gap-2 rounded-lg bg-destructive/10 p-3 text-xs text-destructive"><AlertTriangle className="size-3.5 shrink-0 mt-0.5" /><span>{t("export.warning")}</span></div>
          <div className="flex gap-2"><Button variant="outline" onClick={handleClose} className="flex-1">{t("entries.cancel")}</Button><Button onClick={handleExport} disabled={selected.length === 0 || exporting} className="flex-1 gap-1"><Download className="size-4" />{exporting ? "..." : t("export.exportBtn", { count: selected.length })}</Button></div>
        </>) : (<div className="flex flex-col items-center gap-4 py-8"><div className="flex size-16 items-center justify-center rounded-2xl bg-green-500/10"><Download className="size-8 text-green-500" /></div><div className="text-center"><h3 className="text-lg font-bold">{t("export.success")}</h3><p className="text-sm text-muted-foreground">{t("export.exported", { count: selected.length, format: format.toUpperCase() })}</p></div><Button onClick={handleClose} className="w-full">{t("common.done")}</Button></div>)}
      </DialogContent>
    </Dialog>
  );
}
