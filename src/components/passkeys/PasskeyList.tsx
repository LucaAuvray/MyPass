import { useTranslation } from "react-i18next";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/shared/EmptyState";
import { Plus, Shield, Fingerprint } from "lucide-react";
import { cn } from "@/lib/utils";

interface Passkey {
  entry_uuid: string;
  credential_id: string;
  relying_party: string;
  username: string;
  created: string;
  counter: number;
}

interface PasskeyListProps {
  passkeys: Passkey[];
  onSelect?: (pk: Passkey) => void;
  selectedId?: string | null;
}

export function PasskeyList({ passkeys, onSelect, selectedId }: PasskeyListProps) {
  const { t } = useTranslation();
  if (passkeys.length === 0) {
    return (
      <EmptyState
        icon={<Fingerprint className="size-12" />}
        title={t("passkeys.empty")}
        description={t("passkeys.emptyDesc")}
        action={
          <Button size="sm" className="gap-1">
            <Plus className="size-3.5" />
            {t("passkeys.learnMore")}
          </Button>
        }
        className="py-24"
      />
    );
  }

  return (
    <div className="space-y-3">
      {passkeys.map((pk) => (
        <Card
          key={pk.entry_uuid}
          className={cn(
            "cursor-pointer transition-all hover:border-primary/30",
            selectedId === pk.entry_uuid && "ring-2 ring-primary border-primary",
          )}
          onClick={() => onSelect?.(pk)}
        >
          <CardContent className="flex items-center gap-4 p-4">
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-amber-500/10">
              <Fingerprint className="size-5 text-amber-500" />
            </div>
            <div className="min-w-0 flex-1">
              <h3 className="truncate text-sm font-semibold">{pk.relying_party}</h3>
              <p className="truncate text-xs text-muted-foreground">{pk.username}</p>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant="secondary" className="text-[10px]">
                <Shield className="mr-1 size-2.5" />
                Passkey
              </Badge>
              <span className="text-[10px] text-muted-foreground">
                {formatDate(pk.created)}
              </span>
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function formatDate(dateStr: string): string {
  if (!dateStr) return "";
  try { return new Date(dateStr).toLocaleDateString(); } catch { return dateStr.slice(0, 10); }
}
