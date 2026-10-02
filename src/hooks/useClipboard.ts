import { useCallback, useRef } from "react";

interface UseClipboardOptions {
  /** Auto-clear timeout in milliseconds. Default: 30_000 (30s). */
  clearAfter?: number;
}

export function useClipboard(options: UseClipboardOptions = {}) {
  const { clearAfter = 30_000 } = options;
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const copy = useCallback(
    async (text: string): Promise<boolean> => {
      try {
        // Use Tauri clipboard plugin when available
        const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
        await writeText(text);
      } catch {
        // Fallback to browser clipboard
        try {
          await navigator.clipboard.writeText(text);
        } catch {
          return false;
        }
      }

      // Auto-clear after timeout
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(async () => {
        try {
          const { clear } = await import("@tauri-apps/plugin-clipboard-manager");
          await clear();
        } catch {
          // Silently fail — clipboard auto-clear is best-effort
        }
      }, clearAfter);

      return true;
    },
    [clearAfter],
  );

  return { copy };
}
