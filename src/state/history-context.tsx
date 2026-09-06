import * as React from "react";

import type {
  HistoryAggregate,
  HistoryEntry,
} from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError } from "@/lib/tauri-safe";

interface HistoryContextValue {
  entries: HistoryEntry[];
  aggregate: HistoryAggregate | null;
  loading: boolean;
  notInTauri: boolean;
  refresh: () => Promise<void>;
  remove: (id: number) => Promise<boolean>;
  clear: () => Promise<void>;
}

const HistoryContext = React.createContext<HistoryContextValue | null>(null);

const EMPTY_AGGREGATE: HistoryAggregate = {
  total_runs: 0,
  total_files_processed: 0,
  total_files_failed: 0,
  total_original_bytes: 0,
  total_output_bytes: 0,
  total_duration_ms: 0,
};

export const HistoryProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [entries, setEntries] = React.useState<HistoryEntry[]>([]);
  const [aggregate, setAggregate] = React.useState<HistoryAggregate | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [notInTauri, setNotInTauri] = React.useState(false);

  const refresh = React.useCallback(async () => {
    try {
      const [e, a] = await Promise.all([
        tauri.getHistory(),
        tauri.getHistoryAggregate(),
      ]);
      setEntries(e);
      setAggregate(a);
      setNotInTauri(false);
    } catch (err) {
      if (isNotInTauriError(err)) {
        setNotInTauri(true);
        setEntries([]);
        setAggregate(EMPTY_AGGREGATE);
      } else {
        console.error("History refresh failed:", err);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const remove = React.useCallback(
    async (id: number) => {
      try {
        const result = await tauri.removeHistoryEntry(id);
        if (result) await refresh();
        return result;
      } catch (e) {
        if (isNotInTauriError(e)) {
          setNotInTauri(true);
          return false;
        }
        console.error("History remove failed:", e);
        return false;
      }
    },
    [refresh],
  );

  const clear = React.useCallback(async () => {
    try {
      await tauri.clearHistory();
      await refresh();
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
        setEntries([]);
        setAggregate(EMPTY_AGGREGATE);
      }
    }
  }, [refresh]);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const value = React.useMemo<HistoryContextValue>(
    () => ({ entries, aggregate, loading, notInTauri, refresh, remove, clear }),
    [entries, aggregate, loading, notInTauri, refresh, remove, clear],
  );

  return (
    <HistoryContext.Provider value={value}>{children}</HistoryContext.Provider>
  );
};

export function useHistory(): HistoryContextValue {
  const ctx = React.useContext(HistoryContext);
  if (!ctx) {
    throw new Error("useHistory debe usarse dentro de <HistoryProvider>");
  }
  return ctx;
}
