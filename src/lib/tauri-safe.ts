/**
 * Detección de entorno Tauri + wrapper seguro de invoke.
 *
 * En navegador (Vite sin Tauri) `invoke` no existe: sin este guard,
 * cualquier provider que cargue datos al arrancar dejaría la UI
 * clavada en "Cargando…". safeInvoke rechaza con NotInTauriError
 * para que la UI pueda degradar a un estado vacío.
 */

/**
 * True si corremos dentro de un webview de Tauri 2: detecta el
 * global `__TAURI_INTERNALS__` que Tauri inyecta ahí.
 */
export function isTauriEnvironment(): boolean {
  if (typeof window === "undefined") return false;
  const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
  return w.__TAURI_INTERNALS__ !== undefined;
}

/**
 * Error que lanza `invoke` fuera de Tauri. La UI lo captura y aplica
 * un fallback en vez de romper.
 */
export class NotInTauriError extends Error {
  constructor(message = "Esta función solo está disponible dentro de la app de escritorio Tauri.") {
    super(message);
    this.name = "NotInTauriError";
  }
}

/**
 * invoke de @tauri-apps/api/core con guard de entorno: fuera de Tauri
 * rechaza con NotInTauriError.
 */
export async function safeInvoke<T = unknown>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauriEnvironment()) {
    throw new NotInTauriError();
  }
  // Import lazy: en navegador, importar @tauri-apps/api dispara
  // errores de transformCallback antes de poder usar nada.
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export function isNotInTauriError(e: unknown): boolean {
  return e instanceof NotInTauriError;
}
