import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { tauriCommand } from "@/lib/tauri";
import { Terminal } from "lucide-react";

interface SignRequest {
  requestId: string;
  title: string;
  fingerprint: string;
}

/** Popup globale : l'agent SSH demande l'autorisation de signer. Timeout côté
 *  backend (30 s) — l'événement ssh-sign-request-closed referme le dialog. */
export function SshSignRequestDialog() {
  const { t } = useTranslation();
  // FIFO : VS Code Remote-SSH peut ouvrir 2-3 connexions d'affilée, chacune
  // émettant sa propre ssh-sign-request — une demande cachée ne doit pas
  // mourir au timeout de 30 s côté backend.
  const [requests, setRequests] = useState<SignRequest[]>([]);
  const [remember, setRemember] = useState(false);
  const request = requests[0] ?? null;

  useEffect(() => {
    if (!("__TAURI__" in window)) return;
    const unlistenRequest = listen<SignRequest>("ssh-sign-request", (e) => {
      setRequests((r) => [...r, e.payload]);
      // Amène la fenêtre au premier plan : la demande vient d'un terminal.
      const w = getCurrentWindow();
      w.show()
        .then(() => w.setFocus())
        .catch(() => {});
    });
    const unlistenClosed = listen<{ requestId: string }>("ssh-sign-request-closed", (e) => {
      setRequests((r) => r.filter((req) => req.requestId !== e.payload.requestId));
    });
    return () => {
      unlistenRequest.then((f) => f());
      unlistenClosed.then((f) => f());
    };
  }, []);

  // Nouvelle tête de file (nouvelle demande affichée) : le choix "se
  // souvenir" ne doit jamais fuiter d'une demande à la suivante.
  useEffect(() => {
    setRemember(false);
  }, [request?.requestId]);

  const respond = async (approved: boolean) => {
    if (!request) return;
    const { requestId } = request;
    setRequests((r) => r.filter((req) => req.requestId !== requestId));
    await tauriCommand("respond_ssh_sign", { requestId, approved, remember }).catch(() => {});
  };

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
            <Terminal className="text-primary size-5" /> {t("sshAgent.requestTitle")}
          </DialogTitle>
          <DialogDescription>{t("sshAgent.requestDesc")}</DialogDescription>
        </DialogHeader>
        {request && (
          <div className="border-border bg-muted/30 space-y-1 rounded-xl border p-3">
            <p className="text-sm font-semibold">{request.title}</p>
            <p className="text-muted-foreground font-mono text-xs break-all">
              {request.fingerprint}
            </p>
          </div>
        )}
        <div className="flex items-center gap-2">
          <Checkbox
            id="ssh-remember"
            checked={remember}
            onCheckedChange={(v) => setRemember(v === true)}
          />
          <Label htmlFor="ssh-remember" className="text-xs">
            {t("sshAgent.remember")}
          </Label>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => respond(false)}>
            {t("sshAgent.deny")}
          </Button>
          <Button onClick={() => respond(true)}>{t("sshAgent.approve")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
