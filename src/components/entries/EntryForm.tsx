import { useMemo } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
} from "@/components/ui/sheet";
import { Label } from "@/components/ui/label";
import { useEntries } from "@/hooks/useEntries";
import { RefreshCw } from "lucide-react";
import { usePasswordGenerator } from "@/hooks/usePasswordGenerator";

const makeEntrySchema = (t: (key: string) => string) =>
  z.object({
    title: z.string().min(1, t("entries.titleRequired")),
    username: z.string().min(1, t("entries.usernameRequired")),
    password: z.string().min(4, t("entries.passwordMin")),
    url: z.string().optional(),
    notes: z.string().optional(),
    tags: z.string().optional(),
    groupUuid: z.string().optional(),
  });

type EntryFormValues = z.infer<ReturnType<typeof makeEntrySchema>>;

interface EntryFormProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editEntry?: { uuid: string; title: string; username: string; password: string; url: string; notes: string };
}

export function EntryForm({ open, onOpenChange, editEntry }: EntryFormProps) {
  const { t } = useTranslation();
  const { createEntry, updateEntry } = useEntries();
  const { generatePassword } = usePasswordGenerator();
  const isEditing = !!editEntry;
  const entrySchema = useMemo(() => makeEntrySchema(t), [t]);

  const form = useForm<EntryFormValues>({
    resolver: zodResolver(entrySchema),
    defaultValues: {
      title: editEntry?.title ?? "",
      username: editEntry?.username ?? "",
      password: editEntry?.password ?? "",
      url: editEntry?.url ?? "",
      notes: editEntry?.notes ?? "",
      tags: "",
    },
  });

  const handleGeneratePassword = async () => {
    const pwd = await generatePassword({
      length: 20,
      uppercase: true,
      lowercase: true,
      digits: true,
      symbols: true,
      excludeSimilar: false,
      excludeAmbiguous: false,
    });
    if (pwd) form.setValue("password", pwd);
  };

  const onSubmit = async (data: EntryFormValues) => {
    if (isEditing && editEntry) {
      const { tags, ...rest } = data;
      await updateEntry({
        uuid: editEntry.uuid,
        update: {
          ...rest,
          tags: tags ? tags.split(",").map((t: string) => t.trim()) : undefined,
        },
      });
    } else {
      await createEntry({
        title: data.title,
        username: data.username,
        password: data.password,
        url: data.url,
        notes: data.notes,
        tags: data.tags ? data.tags.split(",").map((t: string) => t.trim()) : [],
        groupUuid: data.groupUuid,
      });
    }
    form.reset();
    onOpenChange(false);
  };

  const content = (
    <form onSubmit={form.handleSubmit(onSubmit)} className="flex flex-col gap-4">
      <div className="space-y-4">
        <div className="space-y-1.5">
          <Label htmlFor="title">{t("entries.title")}</Label>
          <Input id="title" {...form.register("title")} placeholder="Google" autoFocus />
          {form.formState.errors.title && (
            <p className="text-xs text-destructive">{form.formState.errors.title.message}</p>
          )}
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="username">{t("entries.username")}</Label>
          <Input id="username" {...form.register("username")} placeholder="user@example.com" />
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="password">{t("entries.password")}</Label>
          <div className="flex items-center gap-2">
            <Input
              id="password"
              type="password"
              {...form.register("password")}
              className="font-mono"
              placeholder={t("entries.passwordPlaceholder")}
            />
            <Button
              type="button"
              variant="outline"
              size="icon"
              onClick={handleGeneratePassword}
              title={t("entries.generate")}
            >
              <RefreshCw className="size-4" />
            </Button>
          </div>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="url">{t("entries.url")}</Label>
          <Input id="url" {...form.register("url")} placeholder="https://example.com" />
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="notes">{t("entries.notes")}</Label>
          <Textarea id="notes" {...form.register("notes")} placeholder={t("entries.notesPlaceholder")} rows={3} />
        </div>
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
          {t("entries.cancel")}
        </Button>
        <Button type="submit">
          {isEditing ? t("entries.save") : t("entries.create")}
        </Button>
      </DialogFooter>
    </form>
  );

  if (typeof window !== "undefined" && window.innerWidth < 768) {
    return (
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent side="bottom" className="h-[90vh] overflow-y-auto">
          <SheetHeader>
            <SheetTitle>{isEditing ? t("entries.edit") : t("entries.newEntry")}</SheetTitle>
            <SheetDescription>
              {isEditing ? t("entries.editEntryDesc") : t("entries.newEntryDesc")}
            </SheetDescription>
          </SheetHeader>
          {content}
        </SheetContent>
      </Sheet>
    );
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{isEditing ? t("entries.edit") : t("entries.newEntry")}</DialogTitle>
          <DialogDescription>
            {isEditing ? t("entries.editEntryDesc") : t("entries.newEntryDesc")}
          </DialogDescription>
        </DialogHeader>
        {content}
      </DialogContent>
    </Dialog>
  );
}
