import { useEffect } from "react";
import { useForm, Controller } from "react-hook-form";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useEntries } from "@/hooks/useEntries";
import {
  type ItemKind,
  IDENTITY_FIELDS,
  CARD_FIELDS,
  DOCUMENT_FIELDS,
  DOC_KINDS,
  MYPASS_TYPE_KEY,
} from "@/lib/items";

type ItemFormValues = { title: string; notes: string } & Record<string, string>;

interface ItemFormProps {
  kind: Exclude<ItemKind, "login">;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editEntry?: { uuid: string; title: string; notes: string; customFields: Record<string, string> };
}

const KIND_FIELDS: Record<Exclude<ItemKind, "login">, readonly string[]> = {
  identity: IDENTITY_FIELDS,
  card: CARD_FIELDS,
  document: DOCUMENT_FIELDS,
  // Édition d'une clé SSH : titre/commentaire/notes seulement — le matériel
  // de clé (privée/publique/fingerprint) n'est jamais éditable à la main.
  ssh_key: ["SSH_Comment"],
};

export function ItemForm({ kind, open, onOpenChange, editEntry }: ItemFormProps) {
  const { t } = useTranslation();
  const { createEntry, updateEntry } = useEntries();
  const isEditing = !!editEntry;
  const fields = KIND_FIELDS[kind];

  const form = useForm<ItemFormValues>({ defaultValues: defaultsFrom(editEntry, fields) });

  // Réinitialise quand on ouvre pour un autre élément / une création
  useEffect(() => {
    if (open) form.reset(defaultsFrom(editEntry, fields));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, editEntry?.uuid]);

  const onSubmit = async (data: ItemFormValues) => {
    const customFields: Record<string, string> = { [MYPASS_TYPE_KEY]: kind };
    for (const key of fields) {
      // valeur vide => suppression côté backend en édition ; ignorée à la création
      customFields[key] = (data[key] ?? "").trim();
    }
    if (isEditing && editEntry) {
      await updateEntry({
        uuid: editEntry.uuid,
        update: { title: data.title, notes: data.notes, customFields },
      });
    } else {
      await createEntry({
        title: data.title,
        username: "",
        password: "",
        notes: data.notes,
        customFields: Object.fromEntries(Object.entries(customFields).filter(([, v]) => v !== "")),
      });
    }
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {isEditing ? t("items.editTitle", { kind: t(`items.${kind}`) }) : t(`items.new.${kind}`)}
          </DialogTitle>
          <DialogDescription>{t(`items.desc.${kind}`)}</DialogDescription>
        </DialogHeader>

        <form onSubmit={form.handleSubmit(onSubmit)} className="flex flex-col gap-4">
          <div className="space-y-1.5">
            <Label htmlFor="item-title">{t("entries.title")}</Label>
            <Input
              id="item-title"
              autoFocus
              {...form.register("title", { required: t("entries.titleRequired") })}
              placeholder={t(`items.titlePlaceholder.${kind}`)}
            />
            {form.formState.errors.title && (
              <p className="text-xs text-destructive">{form.formState.errors.title.message}</p>
            )}
          </div>

          <div className="grid grid-cols-2 gap-3">
            {fields.map((key) => (
              <ItemField key={key} fieldKey={key} form={form} t={t} />
            ))}
          </div>

          <div className="space-y-1.5">
            <Label htmlFor="item-notes">{t("entries.notes")}</Label>
            <Textarea id="item-notes" rows={2} {...form.register("notes")} />
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t("entries.cancel")}
            </Button>
            <Button type="submit">{isEditing ? t("entries.save") : t("entries.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function defaultsFrom(
  editEntry: ItemFormProps["editEntry"],
  fields: readonly string[],
): ItemFormValues {
  const values: ItemFormValues = {
    title: editEntry?.title ?? "",
    notes: editEntry?.notes ?? "",
  };
  for (const key of fields) values[key] = editEntry?.customFields[key] ?? "";
  return values;
}

/** Un champ d'élément : select pour DOC_Kind, input typé sinon. */
function ItemField({
  fieldKey,
  form,
  t,
}: {
  fieldKey: string;
  form: ReturnType<typeof useForm<ItemFormValues>>;
  t: (key: string) => string;
}) {
  const label = t(`items.fields.${fieldKey}`);
  const wide = fieldKey === "ID_Address" || fieldKey === "CC_Number";
  const isDate = fieldKey.endsWith("Date");
  const isSecret = fieldKey === "CC_Number" || fieldKey === "CC_CVC" || fieldKey === "DOC_Number";

  if (fieldKey === "DOC_Kind") {
    return (
      <div className="space-y-1.5">
        <Label>{label}</Label>
        <Controller
          control={form.control}
          name={fieldKey}
          render={({ field }) => (
            <Select value={field.value || undefined} onValueChange={field.onChange}>
              <SelectTrigger>
                <SelectValue placeholder={label} />
              </SelectTrigger>
              <SelectContent>
                {DOC_KINDS.map((k) => (
                  <SelectItem key={k} value={k}>
                    {t(`items.docKinds.${k}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
      </div>
    );
  }

  return (
    <div className={wide ? "col-span-2 space-y-1.5" : "space-y-1.5"}>
      <Label htmlFor={`item-${fieldKey}`}>{label}</Label>
      <Input
        id={`item-${fieldKey}`}
        type={isDate ? "date" : "text"}
        inputMode={fieldKey === "CC_Number" || fieldKey === "CC_CVC" ? "numeric" : undefined}
        className={isSecret ? "font-mono" : undefined}
        maxLength={fieldKey === "CC_CVC" ? 4 : fieldKey === "CC_ExpMonth" ? 2 : fieldKey === "CC_ExpYear" ? 4 : undefined}
        placeholder={fieldKey === "CC_ExpMonth" ? "MM" : fieldKey === "CC_ExpYear" ? "AAAA" : undefined}
        {...form.register(fieldKey)}
      />
    </div>
  );
}
