import * as React from "react";

import type { AppSettings, Theme } from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError } from "@/lib/tauri-safe";

interface SettingsContextValue {
  settings: AppSettings | null;
  loading: boolean;
  error: string | null;
  /** true cuando corre en navegador sin Tauri (p. ej. `pnpm dev`). */
  notInTauri: boolean;
  update: (next: AppSettings) => Promise<void>;
  /**
   * update con debounce (400ms por defecto): la UI refleja el cambio
   * al instante, pero solo se persiste cuando el usuario deja de
   * mover el control. Para sliders y campos con muchos cambios
   * seguidos; selects/toggles pueden usar `update` directo.
   */
  updateDebounced: (next: AppSettings, delayMs?: number) => void;
  refresh: () => Promise<void>;
}

/**
 * Resuelve el tema efectivo a aplicar al <html>: si el usuario eligió
 * "System", consulta la media query `prefers-color-scheme: dark` del SO.
 * Devuelve "dark" o "light" — nunca "system".
 */
function resolveEffectiveTheme(theme: Theme): "dark" | "light" {
  if (theme === "Dark") return "dark";
  if (theme === "Light") return "light";
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return "light";
}

/**
 * Aplica (o quita) la clase `dark` en el <html> según el tema efectivo.
 * Tailwind está configurado con `darkMode: ["class"]`, así que la
 * presencia de `.dark` en cualquier ancestro activa las variantes
 * `dark:`. Las variables CSS en `globals.css` sobrescriben `:root`
 * bajo `.dark`, así que TODOS los componentes cambian automáticamente
 * sin tocar código por componente.
 */
function applyThemeClass(theme: Theme): void {
  if (typeof document === "undefined") return;
  const effective = resolveEffectiveTheme(theme);
  const root = document.documentElement;
  if (effective === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
  // Meta theme-color para que la barra del navegador (en móviles / Tauri
  // con barra de título custom) también se ajuste.
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) {
    meta.setAttribute("content", effective === "dark" ? "#0E0F11" : "#FFFFFF");
  }
}

const SettingsContext = React.createContext<SettingsContextValue | null>(null);

export const SettingsProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [settings, setSettings] = React.useState<AppSettings | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [notInTauri, setNotInTauri] = React.useState(false);

  const refresh = React.useCallback(async () => {
    try {
      setLoading(true);
      const s = await tauri.getSettings();
      setSettings(s);
      setError(null);
      setNotInTauri(false);
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
        // Default para no quedarse en "Cargando…". El tema se respeta
        // desde localStorage: sin backend, es la única forma de que el
        // elegido por el usuario sobreviva a un refresco de página.
        const storedTheme =
          typeof localStorage !== "undefined"
            ? (localStorage.getItem("boxflux.theme") as Theme | null)
            : null;
        setSettings({
          theme: storedTheme ?? "Light",
          language: "es",
          start_minimized: false,
          notifications_enabled: true,
          worker_count: 0,
          automatic_processing: false,
          default_profile: "Web",
          default_quality: 82,
          metadata_behavior: "RemoveSafe",
          default_output_format: "",
          output_mode: "OutputSubfolder",
          custom_output_folder: "",
          preserve_folder_structure: true,
          filename_template: "{name}_optimized{format}",
          cpu_limit_percent: 0,
          max_memory_mb: 0,
          logging_enabled: true,
          diagnostics_enabled: false,
          experimental_features: false,
        });
      } else {
        setError(String(e));
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const update = React.useCallback(
    async (next: AppSettings) => {
      try {
        await tauri.updateSettings(next);
        setSettings(next);
        setError(null);
      } catch (e) {
        // En modo sin Tauri, al menos actualizamos el estado local para
        // que la UI refleje el cambio (no se persistirá hasta que Tauri esté).
        if (isNotInTauriError(e)) {
          setSettings(next);
        } else {
          setError(String(e));
        }
      }
    },
    [],
  );

  // Guardado con debounce: los sliders llaman a update en cada tick del
  // arrastre, y cada update es una escritura a disco de settings.json en
  // el backend. Así solo persiste el valor final del gesto.
  const timerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const updateRef = React.useRef(update);
  updateRef.current = update;
  const updateDebounced = React.useCallback(
    (next: AppSettings, delayMs = 400) => {
      setSettings(next); // feedback inmediato en la UI
      setError(null);
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
      timerRef.current = setTimeout(() => {
        void updateRef.current(next);
      }, delayMs);
    },
    [],
  );

  React.useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  // Aplicar el tema al <html>: settings.theme es solo estado; la clase
  // .dark del documento la pone este effect.
  React.useEffect(() => {
    if (!settings) return;
    applyThemeClass(settings.theme);
    // Espejo en localStorage para que el script anti-flash de index.html
    // aplique el tema ANTES de que React monte; sin esto el primer paint
    // siempre es claro aunque el usuario eligiera oscuro.
    try {
      localStorage.setItem("boxflux.theme", settings.theme);
    } catch {
      /* noop — localStorage puede estar deshabilitado. */
    }
    // Solo reaccionamos a `settings.theme`: depender de `settings`
    // completo ejecutaría este effect en cada cambio de cualquier ajuste.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.theme]);

  // Con tema "System", seguir al SO en vivo sin reiniciar la app.
  React.useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    if (!settings || settings.theme !== "System") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = () => applyThemeClass("System");
    // Algunos navegadores antiguos usan addListener en vez de addEventListener.
    if (typeof mq.addEventListener === "function") {
      mq.addEventListener("change", handler);
      return () => mq.removeEventListener("change", handler);
    }
    // Fallback (Safari < 14).
    mq.addListener(handler);
    return () => mq.removeListener(handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings?.theme]);

  const value = React.useMemo<SettingsContextValue>(
    () => ({
      settings,
      loading,
      error,
      notInTauri,
      update,
      updateDebounced,
      refresh,
    }),
    [settings, loading, error, notInTauri, update, updateDebounced, refresh],
  );

  return (
    <SettingsContext.Provider value={value}>
      {children}
    </SettingsContext.Provider>
  );
};

export function useSettings(): SettingsContextValue {
  const ctx = React.useContext(SettingsContext);
  if (!ctx) {
    throw new Error("useSettings debe usarse dentro de <SettingsProvider>");
  }
  return ctx;
}
