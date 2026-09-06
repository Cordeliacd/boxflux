import * as React from "react";

import type { Format, Job, QueueStats } from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError, isTauriEnvironment } from "@/lib/tauri-safe";
import { useErrorBanner } from "@/components/ui/error-banner";

interface EnqueueOptions {
  force_format?: Format | null;
  force_lossless?: boolean;
  strip_all_metadata?: boolean;
  preserve_color_profile?: boolean;
}

interface QueueContextValue {
  jobs: Job[];
  stats: QueueStats | null;
  loading: boolean;
  notInTauri: boolean;
  refresh: () => Promise<void>;
  start: () => Promise<void>;
  pause: () => Promise<void>;
  resume: () => Promise<void>;
  cancelJob: (id: number) => Promise<boolean>;
  retryJob: (id: number) => Promise<number | null>;
  removeJob: (id: number) => Promise<boolean>;
  clearCompleted: () => Promise<void>;
  /**
   * A diferencia del resto de acciones, enqueue LANZA los errores del
   * backend (salida == entrada, archivo inválido): la página que
   * encola decide cómo mostrarlos.
   */
  enqueueOptimize: (
    source: string,
    mode: "Lossless" | "Compress",
    options?: EnqueueOptions,
  ) => Promise<number>;
  addDroppedPaths: (paths: string[]) => Promise<number>;
}

const QueueContext = React.createContext<QueueContextValue | null>(null);

const EMPTY_STATS: QueueStats = {
  total_jobs: 0,
  queued: 0,
  processing: 0,
  completed: 0,
  failed: 0,
  cancelled: 0,
  skipped: 0,
  original_bytes: 0,
  final_bytes: 0,
  total_processing_ms: 0,
  active_workers: 0,
};

export const QueueProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [jobs, setJobs] = React.useState<Job[]>([]);
  const [stats, setStats] = React.useState<QueueStats | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [notInTauri, setNotInTauri] = React.useState(false);
  // Banner de errores globales — para mostrar errores de jobs en la
  // parte superior de la app, visibles y persistentes.
  const { pushError } = useErrorBanner();
  const pushErrorRef = React.useRef(pushError);
  pushErrorRef.current = pushError;
  // Track de jobs que ya anunciamos como fallidos — para no re-spamear
  // el mismo error en cada refresh de la cola.
  const announcedFailedJobsRef = React.useRef<Set<number>>(new Set());

  const refresh = React.useCallback(async () => {
    try {
      const [j, s] = await Promise.all([
        tauri.getQueueSnapshot(),
        tauri.getQueueStats(),
      ]);
      setJobs(j);
      setStats(s);
      setNotInTauri(false);
      // Detectar jobs que acaban de fallar: el evento job-updated no
      // siempre llega, así que comparamos la snapshot con el estado
      // previo. Cada fallo se anuncia en el banner una sola vez.
      for (const job of j) {
        if (job.status === "Failed" && job.error) {
          if (!announcedFailedJobsRef.current.has(job.id)) {
            announcedFailedJobsRef.current.add(job.id);
            pushErrorRef.current({
              id: `job-failed-${job.id}`,
              title: `Error en: ${job.source_path.split(/[\\/]/).pop() ?? "archivo"}`,
              description: job.error,
              severity: "error",
            });
          }
        } else if (job.status === "Completed" || job.status === "Cancelled") {
          // Si el job se recuperó o canceló, permitir re-anunciar
          // si vuelve a fallar en el futuro.
          announcedFailedJobsRef.current.delete(job.id);
        }
      }
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
        setJobs([]);
        setStats(EMPTY_STATS);
      } else {
        console.error("Queue refresh failed:", e);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  // Detección barata de "navegador sin Tauri" para no arrancar
  // polling inútil.
  const isBrowserOnly = React.useMemo(() => !isTauriEnvironment(), []);

  React.useEffect(() => {
    void refresh();
    // unlisten async: si el effect se limpia antes de que listen()
    // resuelva, hay que invocar el UnlistenFn de inmediato o queda
    // registrado para siempre (leak por mount/unmount en StrictMode).
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
    track(tauri.onJobUpdated(() => void refresh()));
    track(tauri.onStatsChanged(() => void refresh()));
    // Polling de respaldo solo en Tauri: en navegador cada tick
    // resetearía los jobs y re-renderizaría todos los consumidores.
    const interval = isBrowserOnly
      ? undefined
      : setInterval(() => void refresh(), 1500);
    return () => {
      cancelled = true;
      if (interval) clearInterval(interval);
      unsubs.forEach((u) => u());
    };
  }, [refresh, isBrowserOnly]);

  const safeCall = React.useCallback(
    async <T,>(fn: () => Promise<T>, fallback: T): Promise<T> => {
      try {
        return await fn();
      } catch (e) {
        if (isNotInTauriError(e)) return fallback;
        console.error("Queue action failed:", e);
        return fallback;
      }
    },
    [],
  );

  // enqueue SIN safeCall: los errores del backend deben llegar a la
  // UI que los muestra.
  const enqueueOptimize = React.useCallback(
    (source: string, mode: "Lossless" | "Compress", options?: EnqueueOptions) =>
      tauri.enqueueOptimizeJob(source, mode, options),
    [],
  );

  const value = React.useMemo<QueueContextValue>(
    () => ({
      jobs,
      stats,
      loading,
      notInTauri,
      refresh,
      start: () => safeCall(tauri.startQueue, undefined),
      pause: () => safeCall(tauri.pauseQueue, undefined),
      resume: () => safeCall(tauri.resumeQueue, undefined),
      cancelJob: (id) => safeCall(() => tauri.cancelJob(id), false),
      retryJob: (id) => safeCall(() => tauri.retryJob(id), null),
      removeJob: (id) => safeCall(() => tauri.removeJob(id), false),
      clearCompleted: () => safeCall(tauri.clearCompleted, undefined),
      enqueueOptimize,
      addDroppedPaths: (paths) => safeCall(() => tauri.addDroppedPaths(paths), 0),
    }),
    [jobs, stats, loading, notInTauri, refresh, safeCall, enqueueOptimize],
  );

  return (
    <QueueContext.Provider value={value}>{children}</QueueContext.Provider>
  );
};

export function useQueue(): QueueContextValue {
  const ctx = React.useContext(QueueContext);
  if (!ctx) {
    throw new Error("useQueue debe usarse dentro de <QueueProvider>");
  }
  return ctx;
}
