import * as React from "react";

import type { OptimizationDecision } from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError, NotInTauriError } from "@/lib/tauri-safe";
import { useErrorBanner } from "@/components/ui/error-banner";

interface AutoOptimizeState {
  running: boolean;
  decision: OptimizationDecision | null;
  error: string | null;
  cancelledReason: string | null;
  currentRequestId: number | null;
  notInTauri: boolean;
  start: (args: tauri.AutoOptimizeArgs) => Promise<number | null>;
  cancel: () => Promise<void>;
  reset: () => void;
}

const AutoOptimizeContext = React.createContext<AutoOptimizeState | null>(null);

export const AutoOptimizeProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [running, setRunning] = React.useState(false);
  const [decision, setDecision] = React.useState<OptimizationDecision | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [cancelledReason, setCancelledReason] = React.useState<string | null>(null);
  const [currentRequestId, setCurrentRequestId] = React.useState<number | null>(null);
  const [notInTauri, setNotInTauri] = React.useState(false);
  // Banner de errores globales — para que los errores del backend
  // aparezcan en la parte superior de la app, visibles y persistentes.
  const { pushError, dismissError } = useErrorBanner();
  // Ref para evitar re-crear el efecto cuando pushError cambia.
  const pushErrorRef = React.useRef(pushError);
  pushErrorRef.current = pushError;
  const dismissErrorRef = React.useRef(dismissError);
  dismissErrorRef.current = dismissError;

  React.useEffect(() => {
    // unlisten async: si el effect se limpia antes de que listen()
    // resuelva, el UnlistenFn llega tarde y queda registrado para
    // siempre (setState sobre componente desmontado; leak bajo
    // StrictMode). El flag `cancelled` lo desuscribe en cuanto llegue.
    let cancelled = false;
    const unsubs: Array<() => void> = [];
    const track = (p: Promise<() => void>) => {
      void p.then((u) => {
        if (cancelled) {
          u();
        } else {
          unsubs.push(u);
        }
      });
    };
    track(
      tauri.onAutoOptimizeStarted((rid) => {
        setCurrentRequestId(rid);
        setRunning(true);
        // Limpiar errores previos al iniciar.
        dismissErrorRef.current("auto-optimize-failed");
      }),
    );
    track(
      tauri.onAutoOptimizeFinished((rid, dec) => {
        setCurrentRequestId(rid);
        setDecision(dec);
        setRunning(false);
        // Limpiar el banner de error si la optimización terminó bien.
        dismissErrorRef.current("auto-optimize-failed");
      }),
    );
    track(
      tauri.onAutoOptimizeFailed((rid, err) => {
        setCurrentRequestId(rid);
        setError(err);
        setRunning(false);
        // El error va al banner global: visible en todas las páginas y
        // persistente hasta que el usuario lo cierre.
        pushErrorRef.current({
          id: "auto-optimize-failed",
          title: "Error en la optimización",
          description: err,
          severity: "error",
        });
      }),
    );
    track(
      tauri.onAutoOptimizeCancelled((rid, reason) => {
        setCurrentRequestId(rid);
        setCancelledReason(reason);
        setRunning(false);
        dismissErrorRef.current("auto-optimize-failed");
      }),
    );
    track(tauri.onAutoOptimizeStateChanged((r) => setRunning(r)));
    return () => {
      cancelled = true;
      unsubs.forEach((u) => u());
    };
  }, []);

  const start = React.useCallback(
    async (args: tauri.AutoOptimizeArgs): Promise<number | null> => {
      try {
        setDecision(null);
        setError(null);
        setCancelledReason(null);
        // Limpiar errores previos del banner al iniciar un nuevo intento.
        dismissError("auto-optimize-failed");
        const rid = await tauri.startAutoOptimize(args);
        setCurrentRequestId(rid);
        return rid;
      } catch (e) {
        if (e instanceof NotInTauriError || isNotInTauriError(e)) {
          setNotInTauri(true);
          setError(
            "La optimización inteligente solo está disponible dentro de la app de escritorio.",
          );
          pushError({
            id: "auto-optimize-failed",
            title: "App de escritorio requerida",
            description:
              "La optimización inteligente solo está disponible dentro de la app de escritorio Tauri. Ejecuta `pnpm tauri:dev`.",
            severity: "warning",
          });
        } else {
          setError(String(e));
          pushError({
            id: "auto-optimize-failed",
            title: "No se pudo iniciar la optimización",
            description: String(e),
            severity: "error",
          });
        }
        return null;
      }
    },
    [dismissError, pushError],
  );

  const cancel = React.useCallback(async () => {
    try {
      await tauri.cancelAutoOptimize();
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
      }
    }
  }, []);

  const reset = React.useCallback(() => {
    setDecision(null);
    setError(null);
    setCancelledReason(null);
    setCurrentRequestId(null);
    setRunning(false);
  }, []);

  const value = React.useMemo<AutoOptimizeState>(
    () => ({
      running,
      decision,
      error,
      cancelledReason,
      currentRequestId,
      notInTauri,
      start,
      cancel,
      reset,
    }),
    [running, decision, error, cancelledReason, currentRequestId, notInTauri, start, cancel, reset],
  );

  return (
    <AutoOptimizeContext.Provider value={value}>
      {children}
    </AutoOptimizeContext.Provider>
  );
};

export function useAutoOptimize(): AutoOptimizeState {
  const ctx = React.useContext(AutoOptimizeContext);
  if (!ctx) {
    throw new Error(
      "useAutoOptimize debe usarse dentro de <AutoOptimizeProvider>",
    );
  }
  return ctx;
}
