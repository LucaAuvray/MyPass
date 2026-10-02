import { useTranslation } from "react-i18next";
import { PasskeyList } from "@/components/passkeys/PasskeyList";
import { PasskeyDetail } from "@/components/passkeys/PasskeyDetail";
import { useState } from "react";

interface Passkey {
  entry_uuid: string; credential_id: string; relying_party: string; username: string; created: string; counter: number;
}
const MOCK_PASSKEYS: Passkey[] = [
  { entry_uuid: "1", credential_id: "cred-1-abc...", relying_party: "github.com", username: "dev-user", created: "2024-03-15", counter: 42 },
  { entry_uuid: "2", credential_id: "cred-2-def...", relying_party: "google.com", username: "user@gmail.com", created: "2024-02-10", counter: 127 },
  { entry_uuid: "3", credential_id: "cred-3-ghi...", relying_party: "microsoft.com", username: "user@outlook.com", created: "2024-01-20", counter: 15 },
];

export function PasskeysView() {
  const { t } = useTranslation();
  const [passkeys] = useState<Passkey[]>(MOCK_PASSKEYS);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = passkeys.find((p) => p.entry_uuid === selectedId);

  return (
    <div className="space-y-6 p-4 md:p-6">
      <div>
        <h1 className="font-display text-2xl font-bold tracking-tight">{t("passkeys.title")}</h1>
        <p className="mt-1 text-sm text-muted-foreground">{t("passkeys.subtitle")}</p>
      </div>
      <div className="flex gap-6">
        <div className="flex-1"><PasskeyList passkeys={passkeys} onSelect={(pk) => setSelectedId(selectedId === pk.entry_uuid ? null : pk.entry_uuid)} selectedId={selectedId} /></div>
        {selected && <div className="w-96 shrink-0 rounded-xl border border-border bg-card"><PasskeyDetail passkey={selected} /></div>}
      </div>
    </div>
  );
}
