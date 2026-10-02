import { useTranslation } from "react-i18next";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { EntryIcon } from "@/components/shared/EntryIcon";
import { cn } from "@/lib/utils";
import { itemKind, maskCardNumber, cardBrand } from "@/lib/items";
import { Contact, CreditCard, FileText, Terminal } from "lucide-react";
import type { Entry } from "@/stores/entriesStore";

const BRAND_LABELS = { visa: "Visa", mastercard: "Mastercard", amex: "Amex", other: "" } as const;

interface EntryCardProps {
  entry: Entry;
  isSelected?: boolean;
  onClick?: () => void;
}

export function EntryCard({ entry, isSelected, onClick }: EntryCardProps) {
  const { t } = useTranslation();

  const kind = itemKind(entry);
  const cf = entry.customFields ?? {};
  const subtitle =
    kind === "card"
      ? `${`${BRAND_LABELS[cardBrand(cf.CC_Number ?? "")]} ${maskCardNumber(cf.CC_Number ?? "")}`.trim()}${cf.CC_ExpMonth ? ` · ${cf.CC_ExpMonth}/${cf.CC_ExpYear ?? ""}` : ""}`
      : kind === "identity"
        ? cf.ID_Email || [cf.ID_FirstName, cf.ID_LastName].filter(Boolean).join(" ")
        : kind === "document"
          ? t(`items.docKinds.${cf.DOC_Kind || "other"}`)
          : kind === "ssh_key"
            ? cf.SSH_Fingerprint || cf.SSH_Algorithm || ""
            : entry.username || t("entries.noUsername");

  return (
    <Card
      className={cn(
        "group cursor-pointer transition-all duration-150 hover:shadow-md",
        "border-border hover:border-primary/30",
        isSelected && "ring-2 ring-primary border-primary",
      )}
      onClick={onClick}
    >
      <div className="flex items-start gap-3 p-4">
        {kind === "login" ? (
          <EntryIcon url={entry.url} size="md" />
        ) : (
          <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-secondary text-muted-foreground">
            {kind === "identity" ? <Contact className="size-5" /> : kind === "card" ? <CreditCard className="size-5" /> : kind === "ssh_key" ? <Terminal className="size-5" /> : <FileText className="size-5" />}
          </div>
        )}

        <div className="min-w-0 flex-1">
          <h3 className="truncate text-sm font-semibold text-foreground">
            {entry.title || t("entries.untitled")}
          </h3>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">
            {subtitle}
          </p>

          {entry.tags.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1">
              {entry.tags.slice(0, 3).map((tag) => (
                <Badge key={tag} variant="secondary" className="text-[10px]">
                  {tag}
                </Badge>
              ))}
              {entry.tags.length > 3 && (
                <Badge variant="secondary" className="text-[10px]">
                  +{entry.tags.length - 3}
                </Badge>
              )}
            </div>
          )}
        </div>

        <div className="shrink-0 text-right">
          <span className="text-[10px] text-muted-foreground">
            {formatDate(entry.modified, t)}
          </span>
        </div>
      </div>
    </Card>
  );
}

function formatDate(dateStr: string, _t: ReturnType<typeof useTranslation>["t"]): string {
  if (!dateStr) return "";
  try {
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - date.getTime();
    const days = Math.floor(diff / 86400000);
    if (days === 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days}d`;
    if (days < 30) return `${Math.floor(days / 7)}w`;
    return date.toLocaleDateString();
  } catch {
    return dateStr.slice(0, 10);
  }
}
