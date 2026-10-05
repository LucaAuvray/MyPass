import { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { AlertTriangle, CheckCircle, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

interface HibpCheckProps {
  className?: string;
  onResult?: (found: boolean, count: number) => void;
}

export function HibpCheck({ className, onResult }: HibpCheckProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<{ found: boolean; count: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const checkPassword = useCallback(async () => {
    if (!password || password.length < 3) return;
    setChecking(true);
    setError(null);
    setResult(null);
    try {
      const encoder = new TextEncoder();
      const data = encoder.encode(password);
      const hashBuffer = await crypto.subtle.digest("SHA-1", data);
      const hashArray = Array.from(new Uint8Array(hashBuffer));
      const hashHex = hashArray
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("")
        .toUpperCase();
      const prefix = hashHex.slice(0, 5);
      const suffix = hashHex.slice(5);
      const response = await fetch(`https://api.pwnedpasswords.com/range/${prefix}`, {
        headers: { "Add-Padding": "true" },
      });
      const body = await response.text();
      const found = body.split("\n").some((line) => line.startsWith(suffix));
      let count = 0;
      if (found) {
        const matchLine = body.split("\n").find((l) => l.startsWith(suffix));
        count = matchLine ? parseInt(matchLine.split(":")[1]?.trim() || "0", 10) : 0;
      }
      setResult({ found, count });
      onResult?.(found, count);
    } catch {
      setError("Failed to check. Try again later.");
    } finally {
      setChecking(false);
    }
  }, [password, onResult]);

  return (
    <Card className={className}>
      <CardContent className="space-y-3 p-4">
        <h4 className="text-sm font-semibold">{t("security.checkBreach")}</h4>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            checkPassword();
          }}
          className="flex gap-2"
        >
          <Input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={t("unlock.password")}
            className="h-8 text-sm"
          />
          <Button
            size="sm"
            onClick={checkPassword}
            disabled={checking || !password}
            className="h-8"
          >
            {checking ? <Loader2 className="size-3.5 animate-spin" /> : t("security.checkBreach")}
          </Button>
        </form>
        {error && <p className="text-destructive text-xs">{error}</p>}
        {result && !checking && (
          <div
            className={cn(
              "flex items-center gap-2 rounded-lg px-3 py-2 text-xs",
              result.found
                ? "bg-destructive/10 text-destructive"
                : "bg-green-500/10 text-green-600",
            )}
          >
            {result.found ? (
              <>
                <AlertTriangle className="size-3.5" />
                {t("security.found", { count: result.count })}
              </>
            ) : (
              <>
                <CheckCircle className="size-3.5" />
                {t("security.notFound")}
              </>
            )}
          </div>
        )}
        <p className="text-muted-foreground text-[10px] leading-relaxed">
          {t("security.checkDesc")}
        </p>
      </CardContent>
    </Card>
  );
}
