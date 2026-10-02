/**
 * Typed wrapper around Tauri's invoke function.
 * 
 * In Tauri runtime, uses the native IPC bridge; in web mode, the wasm vault.
 * `npm run dev` in a plain browser alone gets the mock (./mock).
 */
export type TauriInvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

const TOKEN_KEY = "mypass.web.token";

/** Mode « web » : app servie par mypass-server, coffre en wasm (build `--mode web`). */
export function isWebMode(): boolean {
  return (
    typeof window !== "undefined" &&
    !("__TAURI_INTERNALS__" in window) &&
    import.meta.env.MODE === "web"
  );
}

export function hasWebToken(): boolean {
  return !!localStorage.getItem(TOKEN_KEY);
}

export function setWebToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token.trim());
}

export function getWebToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

/** 401 serveur : le token stocké est invalide → le purger pour que
 * l'UnlockView ré-affiche le champ (spec : « le client redemande le token »). */
export function clearWebToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

let _invoke: TauriInvokeFn;

async function getInvoke(): Promise<TauriInvokeFn> {
  if (_invoke) return _invoke;

  // Check if we're running inside a real Tauri window.
  // Tauri v2 only injects `__TAURI__` when withGlobalTauri is enabled;
  // `__TAURI_INTERNALS__` is always present in a real Tauri webview.
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  if (isTauri) {
    try {
      const mod = await import("@tauri-apps/api/core");
      if (typeof mod.invoke === "function") {
        _invoke = mod.invoke as TauriInvokeFn;
        console.info("[MyPass] Tauri runtime detected. Using native IPC.");
        return _invoke;
      }
    } catch (e) {
      console.warn("[MyPass] Tauri API import failed:", e);
    }
  }

  if (isWebMode()) {
    console.info("[MyPass] Web mode. Vault via WASM + sync server.");
    const { createWebInvoke } = await import("./web");
    _invoke = createWebInvoke();
    return _invoke;
  }

  if (import.meta.env.DEV) {
    console.info("[MyPass] Browser dev mode. Using mock backend.");
    const { createMockInvoke } = await import("./mock");
    _invoke = createMockInvoke();
    return _invoke;
  }

  throw new Error("NO_BACKEND");
}

// =============================================================================
// Public API
// =============================================================================

export async function tauriCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = await getInvoke();
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    // Tauri rejects with the command's raw error (a string): give every
    // caller a real Error so `.message` works on desktop as in web mode.
    throw e instanceof Error ? e : new Error(String(e));
  }
}
