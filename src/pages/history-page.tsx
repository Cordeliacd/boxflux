import * as React from "react";
import { History as HistoryIcon, Trash2 } from "lucide-react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { IconButton } from "@/components/ui/icon-button";
import { useHistory } from "@/state/history-context";
import { formatBytes, formatDuration } from "@/lib/format";

export const HistoryPage: React.FC = () => {
  const { entries, aggregate, remove, clear } = useHistory();

  return (
    <Card>
      <CardHeader
        title="Historial"
        subtitle={
          aggregate
            ? `${aggregate.total_runs} sesiones · ${aggregate.total_files_processed} archivos procesados · ${formatBytes(
                aggregate.total_original_bytes - aggregate.total_output_bytes,
              )} ahorrados`
            : "Cargando…"
        }
        action={
          <button
            type="button"
            onClick={clear}
            disabled={entries.length === 0}
            className="inline-flex h-7 items-center gap-1.5 rounded-md border border-error/30 px-2.5 text-xs font-medium text-error transition-colors hover:bg-error-subtle focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Trash2 className="h-3 w-3" aria-hidden="true" />
            Vaciar historial
          </button>
        }
      />
      <CardContent flush>
        {entries.length === 0 ? (
          <EmptyState
            icon={HistoryIcon}
            title="Sin historial todavía"
            subtitle="Las sesiones de optimización aparecerán aquí tras procesar archivos. Cada entrada incluye archivos procesados, tamaño original y resultado."
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full table-fixed">
              <thead className="bg-surface-inset">
                <tr>
                  <th className="w-[18%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Fecha
                  </th>
                  <th className="w-[18%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Perfil
                  </th>
                  <th className="w-[10%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Archivos
                  </th>
                  <th className="w-[18%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Original
                  </th>
                  <th className="w-[18%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Salida
                  </th>
                  <th className="w-[10%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Duración
                  </th>
                  <th className="w-[8%] px-3 py-2 text-right">
                    <span className="sr-only">Acciones</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {entries.map((e) => {
                  const dt = (() => {
                    try {
                      const d = new Date(e.timestamp);
                      return d.toLocaleString("es-ES", {
                        dateStyle: "short",
                        timeStyle: "short",
                      });
                    } catch {
                      return e.timestamp;
                    }
                  })();
                  const total = e.files_processed + e.files_failed;
                  return (
                    <tr
                      key={e.id}
                      className="border-t border-border transition-colors hover:bg-hover"
                    >
                      <td className="px-3 py-2 font-mono text-xs text-foreground">
                        {dt}
                      </td>
                      <td className="px-3 py-2 text-sm text-foreground">
                        {e.profile_name}
                      </td>
                      <td
                        className={
                          "px-3 py-2 text-right font-mono text-xs " +
                          (e.files_failed > 0 ? "text-warning" : "text-muted-foreground")
                        }
                      >
                        {e.files_processed}/{total}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {formatBytes(e.original_size)}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {formatBytes(e.output_size)}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {formatDuration(e.duration_ms)}
                      </td>
                      <td className="px-3 py-2 text-right">
                        <IconButton
                          hoverColor="error"
                          size="sm"
                          onClick={() => remove(e.id)}
                          aria-label="Eliminar entrada"
                        >
                          <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                        </IconButton>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
};

HistoryPage.displayName = "HistoryPage";
