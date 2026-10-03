import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useEntriesStore } from "@/stores/entriesStore";
import { useEntries } from "@/hooks/useEntries";
import { CopyButton } from "@/components/shared/CopyButton";
import { useReveal } from "@/components/shared/RevealButton";
import { EntryIcon } from "@/components/shared/EntryIcon";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { CardContent } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Globe, User, Lock, KeyRound, Clock, Pencil, Trash2, ExternalLink, Eye, EyeOff, Contact, CreditCard, FileText, Terminal, ShieldCheck } from "lucide-react";
import { useTotpCode, formatTotp } from "@/hooks/useTotpCode";
import { cn } from "@/lib/utils";
import { itemKind, IDENTITY_FIELDS, CARD_FIELDS, DOCUMENT_FIELDS, SSH_FIELDS, SECRET_FIELDS } from "@/lib/items";
import { EntryForm } from "./EntryForm";
import { ItemForm } from "./ItemForm";

interface EntryDetailProps {
  className?: string;
}

export function EntryDetail({ className }: EntryDetailProps) {
  const { t, i18n } = useTranslation();
  const entries = useEntriesStore((s) => s.entries);
  const selectedEntryId = useEntriesStore((s) => s.selectedEntryId);
  const selectEntry = useEntriesStore((s) => s.selectEntry);
  const { deleteEntry } = useEntries();
  const { revealed: showPassword, toggle: togglePassword } = useReveal();
  const [editOpen, setEditOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const entry = entries.find((e) => e.uuid === selectedEntryId);

  if (!entry) {
    return (
      <div className={cn("flex items-center justify-center py-24 text-muted-foreground", className)}>
        <p className="text-sm">{t("entries.selectEntry")}</p>
      </div>
    );
  }

  const kind = itemKind(entry);

  const handleDelete = async () => {
    await deleteEntry(entry.uuid);
    selectEntry(null);
    setDeleteOpen(false);
  };

  return (
    <div className={cn("flex flex-col", className)}>
      {/* Header */}
      <div className="flex items-start gap-4 p-6">
        {kind === "login" ? (
          <EntryIcon url={entry.url} size="lg" />
        ) : (
          <div className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-secondary text-muted-foreground">
            {kind === "identity" ? <Contact className="size-6" /> : kind === "card" ? <CreditCard className="size-6" /> : kind === "ssh_key" ? <Terminal className="size-6" /> : <FileText className="size-6" />}
          </div>
        )}
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-bold text-foreground">{entry.title}</h2>
          <p className="mt-0.5 text-sm text-muted-foreground">{kind === "login" ? entry.url : t(`items.${kind}`)}</p>
          {entry.tags.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1">
              {entry.tags.map((tag) => (
                <Badge key={tag} variant="secondary" className="text-xs">
                  {tag}
                </Badge>
              ))}
            </div>
          )}
        </div>
        <div className="flex gap-1">
          <Button variant="ghost" size="icon" className="size-8" onClick={() => setEditOpen(true)}>
            <Pencil className="size-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="size-8 text-destructive hover:text-destructive"
            onClick={() => setDeleteOpen(true)}
          >
            <Trash2 className="size-4" />
          </Button>
        </div>
      </div>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("entries.deleteTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("entries.deleteDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("entries.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={handleDelete}>
              {t("entries.deleteConfirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Separator />

      {/* Fields */}
      <CardContent className="space-y-4 p-6">
        {kind === "login" && (
          <>
            {/* Username */}
            <FieldRow
              icon={<User className="size-4" />}
              label={t("entries.username")}
              value={entry.username}
              actions={<CopyButton text={entry.username} />}
            />

            {/* Password */}
            <FieldRow
              icon={<Lock className="size-4" />}
              label={t("entries.password")}
              value={showPassword ? entry.password : "••••••••••••"}
              mono
              actions={
                <div className="flex items-center gap-0.5">
                  <CopyButton text={entry.password} />
                  <button onClick={togglePassword} className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" aria-label={showPassword ? t("unlock.hide") : t("unlock.show")}>
                    {showPassword ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                  </button>
                </div>
              }
            />

            {entry.totp && <TotpRow uuid={entry.uuid} />}

            {/* URL */}
            {entry.url && (
              <FieldRow
                icon={<Globe className="size-4" />}
                label={t("entries.website")}
                value={entry.url}
                actions={
                  <a
                    href={entry.url.startsWith("http") ? entry.url : `https://${entry.url}`}
                    target="_blank"
                    rel="noreferrer"
                    className="rounded p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
                  >
                    <ExternalLink className="size-4" />
                  </a>
                }
              />
            )}
          </>
        )}

        {kind !== "login" && <ItemFields kind={kind} customFields={entry.customFields ?? {}} />}

        {/* Notes */}
        {entry.notes && (
          <FieldRow
            icon={<KeyRound className="size-4" />}
            label={t("entries.notes")}
            value={entry.notes}
          />
        )}

        {/* Timestamps */}
        <Separator />
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Clock className="size-3.5" />
          <span>{t("entries.modified", { date: formatDate(entry.modified, i18n.language) })}</span>
        </div>
      </CardContent>

      {kind === "login" ? (
        <EntryForm open={editOpen} onOpenChange={setEditOpen} editEntry={entry} />
      ) : (
        <ItemForm
          kind={kind}
          open={editOpen}
          onOpenChange={setEditOpen}
          editEntry={{ uuid: entry.uuid, title: entry.title, notes: entry.notes, customFields: entry.customFields ?? {} }}
        />
      )}
    </div>
  );
}

const KIND_FIELDS = { identity: IDENTITY_FIELDS, card: CARD_FIELDS, document: DOCUMENT_FIELDS, ssh_key: [...SSH_FIELDS, "SSH_PrivateKey"] } as const;

function ItemFields({ kind, customFields }: { kind: "identity" | "card" | "document" | "ssh_key"; customFields: Record<string, string> }) {
  const { t } = useTranslation();
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});

  return (
    <>
      {KIND_FIELDS[kind].filter((key) => customFields[key]).map((key) => {
        const secret = SECRET_FIELDS.includes(key);
        const isRevealed = !!revealed[key];
        const raw = key === "DOC_Kind" ? t(`items.docKinds.${customFields[key]}`) : customFields[key];
        return (
          <FieldRow
            key={key}
            icon={kind === "identity" ? <Contact className="size-4" /> : kind === "card" ? <CreditCard className="size-4" /> : kind === "ssh_key" ? <Terminal className="size-4" /> : <FileText className="size-4" />}
            label={t(`items.fields.${key}`)}
            value={secret && !isRevealed ? "••••••••" : raw}
            mono={secret}
            actions={
              <div className="flex items-center gap-0.5">
                <CopyButton text={raw} />
                {secret && (
                  <button onClick={() => setRevealed((r) => ({ ...r, [key]: !r[key] }))} className="rounded p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground" aria-label={isRevealed ? t("unlock.hide") : t("unlock.show")}>
                    {isRevealed ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                  </button>
                )}
              </div>
            }
          />
        );
      })}
    </>
  );
}

function TotpRow({ uuid }: { uuid: string }) {
  const { t } = useTranslation();
  const { data, error } = useTotpCode(uuid);

  if (error) {
    const msg = error.message === "TOTP_INVALID" ? t("entries.errors.TOTP_INVALID") : error.message;
    return <FieldRow icon={<ShieldCheck className="size-4" />} label={t("entries.totpCode")} value={msg} />;
  }

  return (
    <div className="space-y-1.5">
      <FieldRow
        icon={<ShieldCheck className="size-4" />}
        label={t("entries.totpCode")}
        value={data ? formatTotp(data.code) : "••• •••"}
        mono
        actions={data && <CopyButton text={data.code} />}
      />
      {data && (
        <div className="ml-7 h-1 overflow-hidden rounded-full bg-secondary">
          <div
            className={cn("h-full transition-[width] duration-1000 ease-linear", data.secondsRemaining <= 5 ? "bg-destructive" : "bg-primary")}
            style={{ width: `${(data.secondsRemaining / data.period) * 100}%` }}
          />
        </div>
      )}
    </div>
  );
}

function FieldRow({
  icon,
  label,
  value,
  mono,
  actions,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  mono?: boolean;
  actions?: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-muted-foreground">{icon}</span>
      <div className="min-w-0 flex-1">
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className={cn("truncate text-sm font-medium", mono && "font-mono")}>{value}</p>
      </div>
      {actions && <div className="flex shrink-0 items-center">{actions}</div>}
    </div>
  );
}

function formatDate(dateStr: string, locale?: string): string {
  if (!dateStr) return "";
  try {
    return new Date(dateStr).toLocaleDateString(locale, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return dateStr.slice(0, 10);
  }
}
