import { useQuery } from "@tanstack/react-query";
import { tauriCommand } from "@/lib/tauri";

export type TotpCode = { code: string; period: number; secondsRemaining: number };

/** Current 2FA code of an entry, refreshed every second while mounted. */
export function useTotpCode(uuid: string) {
  return useQuery({
    queryKey: ["totp", uuid],
    queryFn: () => tauriCommand<TotpCode>("get_totp_code", { uuid }),
    refetchInterval: 1000,
    retry: false,
    staleTime: 0,
  });
}

/** "123456" → "123 456", "12345678" → "1234 5678"; odd lengths unchanged. */
export function formatTotp(code: string): string {
  const half = code.length / 2;
  return Number.isInteger(half) ? `${code.slice(0, half)} ${code.slice(half)}` : code;
}
