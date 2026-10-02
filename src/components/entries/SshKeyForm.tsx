import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { useEntries } from "@/hooks/useEntries";
import { tauriCommand } from "@/lib/tauri";
import { MYPASS_TYPE_KEY } from "@/lib/items";
import { FileKey, Sparkles } from "lucide-react";

interface SshKeyFields {
  privateKey: string;
  publicKey: string;
  fingerprint: string;
  algorithm: string;
  comment: string;
}

interface SshKeyFormProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SshKeyForm({ open, onOpenChange }: SshKeyFormProps) {
  const { t } = useTranslation();
  const { createEntry } = useEntries();
  const fileRef = useRef<HTMLInputElement>(null);
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  const [key, setKey] = useState<SshKeyFields | null>(null);
  const [pendingContent, setPendingContent] = useState<string | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const reset = () => {
    setTitle("");
    setNotes("");
    setKey(null);
    setPendingContent(null);
    setPassphrase("");
    setError(null);
    setSubmitting(false);
  };

  const close = (o: boolean) => {
    if (!o) reset();
    onOpenChange(o);
  };

  const parse = async (content: string, pass?: string) => {
    setError(null);
    try {
      const parsed = await tauriCommand<SshKeyFields>("parse_ssh_key", {
        content,
        passphrase: pass ?? null,
      });
      setKey(parsed);
      setPendingContent(null);
      if (!title) setTitle(parsed.comment || t("items.new.ssh_key"));
    } catch (e) {
      const msg = String(e);
      if (msg.includes("PASSPHRASE_REQUIRED")) setPendingContent(content);
      else if (msg.includes("PASSPHRASE_INVALID")) {
        setPendingContent(content);
        setError(t("sshKeys.passphraseInvalid"));
      } else setError(t("sshKeys.parseError"));
    }
  };

  const handleFile = async (file: File | undefined) => {
    if (!file) return;
    await parse(await file.text());
  };

  const handleGenerate = async () => {
    setError(null);
    try {
      const generated = await tauriCommand<SshKeyFields>("generate_ssh_key", {
        comment: title || "mypass",
      });
      setKey(generated);
      setPendingContent(null);
      if (!title) setTitle(t("items.new.ssh_key"));
    } catch {
      setError(t("sshKeys.generateError"));
    }
  };

  const handleCreate = async () => {
    if (!key || !title.trim() || submitting) return;
    setSubmitting(true);
    try {
      await createEntry({
        title: title.trim(),
        username: "",
        password: "",
        notes,
        customFields: {
          [MYPASS_TYPE_KEY]: "ssh_key",
          SSH_PrivateKey: key.privateKey,
          SSH_PublicKey: key.publicKey,
          SSH_Fingerprint: key.fingerprint,
          SSH_Algorithm: key.algorithm,
          SSH_Comment: key.comment,
        },
      });
      close(false);
    } catch {
      setError(t("sshKeys.createError"));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("items.new.ssh_key")}</DialogTitle>
          <DialogDescription>{t("items.desc.ssh_key")}</DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="space-y-1.5">
            <Label htmlFor="ssh-title">{t("entries.title")}</Label>
            <Input id="ssh-title" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder={t("items.titlePlaceholder.ssh_key")} />
          </div>

          {!key && (
            <div className="flex gap-2">
              <Button type="button" variant="outline" className="flex-1" onClick={() => fileRef.current?.click()}>
                <FileKey className="size-4" /> {t("sshKeys.importFile")}
              </Button>
              <Button type="button" variant="outline" className="flex-1" onClick={handleGenerate}>
                <Sparkles className="size-4" /> {t("sshKeys.generate")}
              </Button>
              <input ref={fileRef} type="file" className="hidden" onChange={(e) => handleFile(e.target.files?.[0])} />
            </div>
          )}

          {pendingContent && (
            <div className="space-y-1.5">
              <Label htmlFor="ssh-passphrase">{t("sshKeys.passphrase")}</Label>
              <div className="flex gap-2">
                <Input id="ssh-passphrase" type="password" value={passphrase} onChange={(e) => setPassphrase(e.target.value)} />
                <Button type="button" onClick={() => parse(pendingContent, passphrase)}>
                  {t("sshKeys.unlock")}
                </Button>
              </div>
            </div>
          )}

          {key && (
            <div className="space-y-2 rounded-xl border border-border bg-muted/30 p-3">
              <p className="break-all font-mono text-xs">{key.publicKey}</p>
              <p className="font-mono text-xs text-muted-foreground">{key.fingerprint}</p>
            </div>
          )}

          {error && <p className="text-xs text-destructive">{error}</p>}

          <div className="space-y-1.5">
            <Label htmlFor="ssh-notes">{t("entries.notes")}</Label>
            <Textarea id="ssh-notes" rows={2} value={notes} onChange={(e) => setNotes(e.target.value)} />
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => close(false)}>
              {t("entries.cancel")}
            </Button>
            <Button type="button" disabled={!key || !title.trim() || submitting} onClick={handleCreate}>
              {t("entries.create")}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}
