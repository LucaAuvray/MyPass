import { useTranslation } from "react-i18next";
import { CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Fingerprint,
  Globe,
  Key,
  Shield,
  Clock,
  Copy,
  User,
  Trash2,
} from "lucide-react";

interface Passkey {
  entry_uuid: string;
  credential_id: string;
  relying_party: string;
  username: string;
  created: string;
  counter: number;
}

interface PasskeyDetailProps {
  passkey: Passkey;
  className?: string;
}

export function PasskeyDetail({ passkey, className }: PasskeyDetailProps) {
  const { t } = useTranslation();
  return (
    <div className={className}>
      <div className="flex items-start gap-4 p-6">
        <div className="flex size-14 shrink-0 items-center justify-center rounded-2xl bg-amber-500/10">
          <Fingerprint className="size-7 text-amber-500" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="text-xl font-bold text-foreground">{passkey.relying_party}</h2>
            <Badge variant="secondary" className="text-xs">
              <Shield className="mr-1 size-3" />
              Passkey
            </Badge>
          </div>
          <p className="mt-0.5 text-sm text-muted-foreground">{passkey.username}</p>
        </div>
        <Button variant="ghost" size="icon" className="size-8 text-destructive hover:text-destructive">
          <Trash2 className="size-4" />
        </Button>
      </div>

      <Separator />

      <CardContent className="space-y-4 p-6">
        <FieldRow
          icon={<Globe className="size-4" />}
          label={t("entries.website")}
          value={passkey.relying_party}
        />
        <FieldRow
          icon={<User className="size-4" />}
          label={t("entries.username")}
          value={passkey.username}
        />
        <FieldRow
          icon={<Key className="size-4" />}
          label={t("passkeys.credentialId")}
          value={passkey.credential_id.slice(0, 32) + "..."}
          mono
          actions={
            <button className="rounded p-1 hover:bg-secondary">
              <Copy className="size-3.5" />
            </button>
          }
        />
        <FieldRow
          icon={<Shield className="size-4" />}
          label={t("passkeys.usageCount")}
          value={passkey.counter.toString()}
        />
        <FieldRow
          icon={<Clock className="size-4" />}
          label={t("passkeys.created")}
          value={passkey.created ? new Date(passkey.created).toLocaleDateString() : t("common.unknown")}
        />
      </CardContent>
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
        <p className={`truncate text-sm font-medium ${mono ? "font-mono" : ""}`}>{value}</p>
      </div>
      {actions && <div className="flex shrink-0 items-center">{actions}</div>}
    </div>
  );
}
