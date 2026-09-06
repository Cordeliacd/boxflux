import * as React from "react";

import type {
  EngineInfo,
  FormatCapabilityDto,
  LiveStats,
} from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError } from "@/lib/tauri-safe";

interface EngineContextValue {
  info: EngineInfo | null;
  formats: FormatCapabilityDto[];
  liveStats: LiveStats | null;
  loading: boolean;
  notInTauri: boolean;
  refresh: () => Promise<void>;
}

const EngineContext = React.createContext<EngineContextValue | null>(null);

// Defaults para navegador sin Tauri: así el dashboard muestra algo en
// la card "Formatos soportados" en vez de un vacío.
const BROWSER_FALLBACK_FORMATS: FormatCapabilityDto[] = [
  { format: "PNG",  can_read: true,  can_write: true,  can_optimize: true,  can_convert: true  },
  { format: "JPEG", can_read: true,  can_write: true,  can_optimize: true,  can_convert: true  },
  { format: "WebP", can_read: true,  can_write: true,  can_optimize: true,  can_convert: true  },
  { format: "AVIF", can_read: false, can_write: true,  can_optimize: false, can_convert: true  },
  { format: "GIF",  can_read: true,  can_write: true,  can_optimize: false, can_convert: true  },
  { format: "BMP",  can_read: true,  can_write: true,  can_optimize: false, can_convert: true  },
  { format: "TIFF", can_read: true,  can_write: true,  can_optimize: false, can_convert: true  },
];

const BROWSER_FALLBACK_INFO: EngineInfo = {
  engine_version: "0.2.0",
  intel_engine_version: "1.0.0",
  max_workers:
    typeof navigator !== "undefined" && navigator.hardwareConcurrency
      ? navigator.hardwareConcurrency
      : 4,
};

const BROWSER_FALLBACK_LIVE: LiveStats = {
  files_processed: 0,
  files_failed: 0,
  bytes_processed_in: 0,
  bytes_processed_out: 0,
  total_processing_ms: 0,
  files_queued: 0,
  files_processing: 0,
  active_workers: 0,
  max_workers: BROWSER_FALLBACK_INFO.max_workers,
  space_saved_bytes: 0,
  compression_percentage: 0,
  processing_speed_files_per_sec: 0,
};

export const EngineProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [info, setInfo] = React.useState<EngineInfo | null>(null);
  const [formats, setFormats] = React.useState<FormatCapabilityDto[]>([]);
  // liveStats (StatisticsManager → estado de la cola) es la única
  // fuente de estadísticas del motor.
  const [liveStats, setLiveStats] = React.useState<LiveStats | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [notInTauri, setNotInTauri] = React.useState(false);

  const refresh = React.useCallback(async () => {
    try {
      const [i, f, ls] = await Promise.all([
        tauri.getEngineInfo(),
        tauri.getFormatList(),
        tauri.getLiveStats(),
      ]);
      setInfo(i);
      setFormats(f);
      setLiveStats(ls);
      setNotInTauri(false);
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
        setInfo(BROWSER_FALLBACK_INFO);
        setFormats(BROWSER_FALLBACK_FORMATS);
        setLiveStats(BROWSER_FALLBACK_LIVE);
      } else {
        console.error("Engine refresh failed:", e);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
    // Polling solo en Tauri: en navegador los datos no cambian.
    if (!notInTauri) {
      const interval = setInterval(() => void refresh(), 2000);
      return () => clearInterval(interval);
    }
    return undefined;
  }, [refresh, notInTauri]);

  const value = React.useMemo<EngineContextValue>(
    () => ({ info, formats, liveStats, loading, notInTauri, refresh }),
    [info, formats, liveStats, loading, notInTauri, refresh],
  );

  return (
    <EngineContext.Provider value={value}>{children}</EngineContext.Provider>
  );
};

export function useEngine(): EngineContextValue {
  const ctx = React.useContext(EngineContext);
  if (!ctx) {
    throw new Error("useEngine debe usarse dentro de <EngineProvider>");
  }
  return ctx;
}
