import * as React from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Play,
  Pause,
  Plus,
  XCircle,
  RefreshCw,
  Trash2,
  ListTodo,
  Brain,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { IconButton } from "@/components/ui/icon-button";
import { StatusBadge } from "@/components/ui/status-badge";
import { Tag } from "@/components/ui/tag";
import { useQueue } from "@/state/queue-context";
import {
  basename,
  colorForReduction,
  formatBytes,
  formatDuration,
  formatPercent,
  jobPercentageSaved,
  truncateMiddle,
} from "@/lib/format";
import { cn } from "@/lib/utils";

export const QueuePage: React.FC = () => {
  const {
    jobs,
    stats,
    start,
    pause,
    resume,
    cancelJob,
    retryJob,
    removeJob,
    clearCompleted,
    addDroppedPaths,
  } = useQueue();

  const handleAddFiles = async () => {
    try {
      const result = await openDialog({
        multiple: true,
        filters: [
          {
            name: "Imágenes",
            extensions: ["png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff"],
          },
        ],
      });
      if (result) {
        const paths = Array.isArray(result) ? result : [result];
        await addDroppedPaths(paths);
      }
    } catch (err) {
      console.warn("Selector de archivos cancelado o falló:", err);
    }
  };

  return (
    <Card>
      <CardHeader
        title="Cola de trabajos"
        subtitle={
          stats
            ? `${stats.total_jobs} trabajos · ${stats.queued} en cola · ${stats.processing} procesando · ${stats.completed} completados`
            : "Cargando…"
        }
        action={
          <Button
            variant="ghost"
            size="sm"
            onClick={clearCompleted}
            disabled={stats?.completed === 0}
          >
            <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
            Limpiar completados
          </Button>
        }
      />
      <CardContent flush>
        {/* Toolbar */}
        <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
          <div className="flex items-center gap-1">
            <Button variant="primary" size="sm" onClick={start}>
              <Play className="h-3.5 w-3.5" aria-hidden="true" />
              Iniciar
            </Button>
            <Button variant="ghost" size="sm" onClick={pause}>
              <Pause className="h-3.5 w-3.5" aria-hidden="true" />
              Pausar
            </Button>
            <Button variant="ghost" size="sm" onClick={resume}>
              <Play className="h-3.5 w-3.5" aria-hidden="true" />
              Reanudar
            </Button>
          </div>
          <div className="ml-auto">
            <Button variant="secondary" size="sm" onClick={handleAddFiles}>
              <Plus className="h-3.5 w-3.5" aria-hidden="true" />
              Añadir archivos
            </Button>
          </div>
        </div>

        {jobs.length === 0 ? (
          <EmptyState
            icon={ListTodo}
            title="Sin trabajos en cola"
            subtitle="Arrastra archivos a la ventana o usa el botón 'Añadir archivos'."
            ctaText="Explorar archivos"
            onCtaClick={handleAddFiles}
          />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full table-fixed">
              <thead className="bg-surface-inset">
                <tr>
                  <th className="w-[8%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Estado
                  </th>
                  <th className="w-[22%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Archivo
                  </th>
                  <th className="w-[10%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Modo
                  </th>
                  <th className="w-[12%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Motor decidió
                  </th>
                  <th className="w-[10%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Original
                  </th>
                  <th className="w-[10%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Resultado
                  </th>
                  <th className="w-[9%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Ahorro
                  </th>
                  <th className="w-[7%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Tiempo
                  </th>
                  <th className="w-[7%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                    <span className="sr-only">Acciones</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {jobs.map((job) => {
                  const pct = jobPercentageSaved(job);
                  const isActive =
                    job.status === "Queued" || job.status === "Processing";
                  const canRetry =
                    job.status === "Failed" || job.status === "Cancelled";
                  // Fallbacks para jobs persistidos con modos antiguos.
                  const modeLabel =
                    job.optimization_mode === "Lossless"
                      ? "Balanceado"
                      : job.optimization_mode === "Compress"
                        ? "Comprimir al máximo"
                        : job.optimization_mode === "Quality"
                          ? "Calidad"
                          : job.optimization_mode === "ExtremeLightweight"
                            ? "Extremo"
                            : String(job.optimization_mode);
                  return (
                    <tr
                      key={job.id}
                      className="border-t border-border transition-colors hover:bg-hover"
                    >
                      <td className="px-3 py-2">
                        <StatusBadge status={job.status} />
                      </td>
                      <td className="px-3 py-2 text-sm">
                        <div className="truncate font-medium text-foreground">
                          {basename(job.source_path)}
                        </div>
                        <div className="truncate text-2xs text-muted-foreground">
                          {truncateMiddle(job.source_path, 50)}
                        </div>
                      </td>
                      <td className="px-3 py-2">
                        <Tag
                          tone={
                            job.optimization_mode === "Lossless"
                              ? "primary"
                              : job.optimization_mode === "Compress"
                                ? "warning"
                                : job.optimization_mode === "Quality"
                                  ? "primary"
                                  : "info"
                          }
                        >
                          {modeLabel}
                        </Tag>
                      </td>
                      <td className="px-3 py-2">
                        {job.selected_format ? (
                          <div className="flex flex-col gap-0.5">
                            <div className="flex items-center gap-1.5">
                              <Brain
                                className="h-3 w-3 text-muted-foreground"
                                aria-hidden="true"
                              />
                              <span className="text-xs font-medium text-foreground">
                                {job.selected_format}
                              </span>
                              {job.selected_quality ? (
                                <span className="font-mono text-2xs text-muted-foreground">
                                  {job.selected_quality}
                                </span>
                              ) : null}
                            </div>
                            {job.decision_explanation ? (
                              <div
                                className="truncate text-2xs text-muted-foreground/70"
                                title={job.decision_explanation}
                              >
                                {job.decision_explanation.slice(0, 60)}
                                {job.decision_explanation.length > 60 ? "…" : ""}
                              </div>
                            ) : null}
                          </div>
                        ) : (
                          <span className="text-xs text-muted-foreground">—</span>
                        )}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {job.original_size > 0 ? formatBytes(job.original_size) : "—"}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {job.final_size > 0 ? formatBytes(job.final_size) : "—"}
                      </td>
                      <td
                        className={cn(
                          "px-3 py-2 text-right font-mono text-xs",
                          job.status === "Completed" ? colorForReduction(pct) : "text-muted-foreground",
                        )}
                      >
                        {job.status === "Completed" ? formatPercent(pct) : "—"}
                      </td>
                      <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                        {job.processing_time_ms > 0
                          ? formatDuration(job.processing_time_ms)
                          : "—"}
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex items-center justify-end gap-1">
                          {isActive ? (
                            <IconButton
                              hoverColor="error"
                              size="sm"
                              onClick={() => cancelJob(job.id)}
                              aria-label="Cancelar"
                            >
                              <XCircle className="h-3.5 w-3.5" aria-hidden="true" />
                            </IconButton>
                          ) : null}
                          {canRetry ? (
                            <IconButton
                              hoverColor="primary"
                              size="sm"
                              onClick={() => retryJob(job.id)}
                              aria-label="Reintentar"
                            >
                              <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                            </IconButton>
                          ) : null}
                          <IconButton
                            hoverColor="error"
                            size="sm"
                            onClick={() => removeJob(job.id)}
                            aria-label="Eliminar"
                          >
                            <Trash2 className="h-3.5 w-3.5" aria-hidden="true" />
                          </IconButton>
                        </div>
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

QueuePage.displayName = "QueuePage";
