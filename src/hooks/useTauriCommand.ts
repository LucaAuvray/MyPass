import { useState, useCallback } from "react";

/**
 * Typed hook wrapping Tauri IPC invoke calls.
 * Returns { data, error, isLoading, execute }.
 */
export function useTauriCommand<TArgs extends Record<string, unknown>, TResult>(
  command: string,
) {
  const [data, setData] = useState<TResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const execute = useCallback(
    async (args?: TArgs): Promise<TResult | null> => {
      setIsLoading(true);
      setError(null);
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke<TResult>(command, args ?? {});
        setData(result);
        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        return null;
      } finally {
        setIsLoading(false);
      }
    },
    [command],
  );

  return { data, error, isLoading, execute };
}
