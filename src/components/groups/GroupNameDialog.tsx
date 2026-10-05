import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface GroupNameDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  initialName: string;
  submitLabel: string;
  /** Rejecting keeps the dialog open and shows the error. */
  onSubmit: (name: string) => Promise<void>;
}

/** Folder name prompt, shared by "new folder" and "rename". */
export function GroupNameDialog({
  open,
  onOpenChange,
  title,
  initialName,
  submitLabel,
  onSubmit,
}: GroupNameDialogProps) {
  const { t } = useTranslation();
  const inputId = useId();
  const [name, setName] = useState(initialName);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setName(initialName);
      setError(null);
    }
  }, [open, initialName]);

  const required = t("groups.errors.GROUP_NAME_REQUIRED");

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (busy) return;
    if (!name.trim()) {
      setError(required);
      return;
    }
    setBusy(true);
    try {
      await onSubmit(name.trim());
      onOpenChange(false);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg === "GROUP_NAME_REQUIRED" ? required : msg);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-1.5">
            <Label htmlFor={inputId}>{t("groups.name")}</Label>
            <Input
              id={inputId}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                setError(null);
              }}
              autoFocus
              autoComplete="off"
            />
            {error && <p className="text-destructive text-xs">{error}</p>}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t("entries.cancel")}
            </Button>
            <Button type="submit" disabled={busy}>
              {submitLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
