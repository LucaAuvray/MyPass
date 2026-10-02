import { useCallback } from "react";
import type { PasswordConfig, PassphraseConfig, StrengthResult } from "@/lib/crypto";
import { tauriCommand } from "@/lib/tauri";

export function usePasswordGenerator() {
  const generatePassword = useCallback(
    async (config: PasswordConfig): Promise<string | null> => {
      const result = await tauriCommand<string>("generate_password", {
        config,
      });
      return result;
    },
    [],
  );

  const generatePassphrase = useCallback(
    async (config: PassphraseConfig): Promise<string | null> => {
      const result = await tauriCommand<string>("generate_passphrase", {
        config,
      });
      return result;
    },
    [],
  );

  const evaluateStrength = useCallback(
    async (password: string): Promise<StrengthResult | null> => {
      const result = await tauriCommand<StrengthResult>("evaluate_strength", {
        password,
      });
      return result;
    },
    [],
  );

  return { generatePassword, generatePassphrase, evaluateStrength };
}
