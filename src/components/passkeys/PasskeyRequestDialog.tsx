import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { KeyRound } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { tauriCommand } from "@/lib/tauri";

interface PasskeyAccount {
  /** Entry uuid, or "new" for a registration into a new entry. */
  id: string;
  username: string;
  title: string;
}

interface PasskeyRequest {
  requestId: string;
  kind: "register" | "get";
  rpId: string;
  accounts: PasskeyAccount[];
  replaces: boolean;
}

/** Popup globale : un site demande, via l'extension, de créer ou d'utiliser une
 *  clé d'accès. Délai côté backend (15 à 120 s) : passkey-request-closed referme
 *  le dialog. Annuler laisse l'extension basculer sur la fenêtre de Windows. */
export function PasskeyRequestDialog() {
  const { t } = useTranslation();
  // FIFO : deux onglets peuvent demander une clé presque en même temps.
  const [requests, setRequests] = useState<PasskeyRequest[]>([]);
  const [choice, setChoice] = useState<{ requestId: string; id: string } | null>(null);
  // Demandes arrivées fenêtre minimisée : on la re-minimise après la réponse
  // pour rendre la main au navigateur.
  const minimizedFor = useRef(new Set<string>());
  const request = requests[0] ?? null;
  const selected =
    choice && choice.requestId === request?.requestId
      ? choice.id
      : (request?.accounts[0]?.id ?? null);

  useEffect(() => {
    // __TAURI__ only exists with withGlobalTauri; __TAURI_INTERNALS__ always does.
    if (!("__TAURI_INTERNALS__" in window)) return;
    const unlistenRequest = listen<PasskeyRequest>("passkey-request", async (e) => {
      setRequests((r) => [...r, e.payload]);
      const w = getCurrentWindow();
      try {
        if (await w.isMinimized()) minimizedFor.current.add(e.payload.requestId);
        await w.unminimize();
        await w.show();
        await w.setFocus();
      } catch {
        // Le dialog s'affiche quand même.
      }
    });
    const unlistenClosed = listen<{ requestId: string }>("passkey-request-closed", (e) => {
      setRequests((r) => r.filter((req) => req.requestId !== e.payload.requestId));
    });
    return () => {
      unlistenRequest.then((f) => f());
      unlistenClosed.then((f) => f());
    };
  }, []);

  const respond = async (approved: boolean) => {
    if (!request) return;
    const { requestId } = request;
    setRequests((r) => r.filter((req) => req.requestId !== requestId));
    await tauriCommand("respond_passkey", { requestId, choice: approved ? selected : null }).catch(
      () => {},
    );
    if (minimizedFor.current.delete(requestId)) {
      await getCurrentWindow()
        .minimize()
        .catch(() => {});
    }
  };

  const isRegister = request?.kind === "register";
  const site = request?.rpId ?? "";
  const label = (a: PasskeyAccount) => a.username || a.title;

  return (
    <Dialog
      open={!!request}
      onOpenChange={(o) => {
        if (!o) respond(false);
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <KeyRound className="text-primary size-5" />
            {isRegister ? t("passkeys.registerTitle") : t("passkeys.getTitle")}
          </DialogTitle>
          <DialogDescription>
            {isRegister
              ? t("passkeys.registerDesc", { site, account: request?.accounts[0]?.username ?? "" })
              : t("passkeys.getDesc", { site })}
          </DialogDescription>
        </DialogHeader>
        {request && isRegister && request.replaces && (
          <p className="text-muted-foreground text-sm">{t("passkeys.replaces")}</p>
        )}
        {request &&
          !isRegister &&
          (request.accounts.length > 1 ? (
            <ToggleGroup
              orientation="vertical"
              variant="outline"
              className="w-full"
              value={selected ? [selected] : []}
              onValueChange={(v) => {
                if (v[0]) setChoice({ requestId: request.requestId, id: v[0] });
              }}
            >
              {request.accounts.map((a) => (
                <ToggleGroupItem key={a.id} value={a.id} className="justify-start">
                  {label(a)}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          ) : (
            <div className="border-border bg-muted/30 rounded-xl border p-3">
              <p className="text-sm font-semibold">
                {request.accounts[0] && label(request.accounts[0])}
              </p>
            </div>
          ))}
        <p className="text-muted-foreground text-xs">{t("passkeys.cancelHint")}</p>
        <DialogFooter>
          <Button variant="outline" onClick={() => respond(false)}>
            {t("passkeys.cancel")}
          </Button>
          <Button onClick={() => respond(true)}>
            {isRegister ? t("passkeys.create") : t("passkeys.signIn")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
