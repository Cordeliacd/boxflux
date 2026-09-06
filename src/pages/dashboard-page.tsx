import * as React from "react";
import {
  CheckCircle2,
  Zap,
  Gauge,
  Cpu,
  ImageOff,
  FileImage,
  ArrowRight,
} from "lucide-react";
import { useNavigate } from "react-router-dom";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { StatCard } from "@/components/ui/stat-card";
import { StatusBadge } from "@/components/ui/status-badge";
import { Tag } from "@/components/ui/tag";
import { useEngine } from "@/state/engine-context";
import { useQueue } from "@/state/queue-context";
import {
  basename,
  colorForReduction,
  formatBytes,
  formatPercent,
  jobPercentageSaved,
} from "@/lib/format";
import { cn } from "@/lib/utils";

const COLUMN_CLASSES =
  "px-3 py-2 text-2xs font-semibold uppercase tracking-wider text-muted-foreground";
const CELL_CLASSES = "px-3 py-2 text-sm";

export const DashboardPage: React.FC = () => {
  const navigate = useNavigate();
  const { liveStats, formats } = useEngine();
  const { jobs } = useQueue();

  const processed = liveStats?.files_processed ?? 0;
  const spaceSaved = Math.max(0, liveStats?.space_saved_bytes ?? 0);
  const compression = liveStats?.compression_percentage ?? 0;
  const processing = liveStats?.files_processing ?? 0;
  const queued = liveStats?.files_queued ?? 0;

  const recentJobs = jobs.slice(-8).reverse();

  return (
    <div className="flex flex-col gap-5">
      {/* Hero stats */}
      <section className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          title="Archivos optimizados"
          value={String(processed)}
          subtitle="Histórico acumulado"
          icon={CheckCircle2}
          tone="success"
          valueClassName="text-success"
        />
        <StatCard
          title="Espacio ahorrado"
          value={formatBytes(spaceSaved)}
          subtitle={`${compression.toFixed(1)}% reducción media`}
          icon={Zap}
          tone="primary"
          valueClassName="text-primary"
        />
        <StatCard
          title="Compresión media"
          value={formatPercent(compression)}
          subtitle="Sobre todos los archivos"
          icon={Gauge}
        />
        <StatCard
          title="Trabajos actuales"
          value={String(processing + queued)}
          subtitle={`${processing} activos · ${queued} en cola`}
          icon={Cpu}
          tone={processing > 0 ? "warning" : "default"}
          highlighted={processing > 0}
          valueClassName={processing > 0 ? "text-warning" : undefined}
        />
      </section>

      {/* Recent activity */}
      <Card>
        <CardHeader
          title="Actividad reciente"
          subtitle="Últimos archivos procesados en esta sesión"
          action={
            <div className="flex items-center gap-3">
              <span className="text-xs text-muted-foreground">
                {recentJobs.length} de {jobs.length}
              </span>
              {jobs.length > 0 ? (
                <button
                  type="button"
                  onClick={() => navigate("/queue")}
                  className="inline-flex items-center gap-1 text-xs font-medium text-primary hover:text-primary-hover transition-colors"
                >
                  Ver cola
                  <ArrowRight className="h-3 w-3" aria-hidden="true" />
                </button>
              ) : null}
            </div>
          }
        />
        <CardContent flush>
          {jobs.length === 0 ? (
            <EmptyState
              icon={ImageOff}
              title="Sin optimizaciones todavía"
              subtitle="Arrastra imágenes a la ventana, o usa la página Optimizar para empezar."
              ctaText="Empezar a optimizar"
              onCtaClick={() => navigate("/optimize")}
            />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full table-fixed">
                <thead className="bg-surface-inset">
                  <tr>
                    <th className={cn(COLUMN_CLASSES, "w-[40%] text-left")}>
                      Archivo
                    </th>
                    <th className={cn(COLUMN_CLASSES, "w-[15%] text-right")}>
                      Original
                    </th>
                    <th className={cn(COLUMN_CLASSES, "w-[15%] text-right")}>
                      Optimizado
                    </th>
                    <th className={cn(COLUMN_CLASSES, "w-[15%] text-right")}>
                      Reducción
                    </th>
                    <th className={cn(COLUMN_CLASSES, "w-[15%] text-left")}>
                      Estado
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {recentJobs.map((job) => {
                    const pct = jobPercentageSaved(job);
                    const completed = job.status === "Completed";
                    return (
                      <tr
                        key={job.id}
                        className="border-t border-border transition-colors hover:bg-hover"
                      >
                        <td className={CELL_CLASSES}>
                          <div className="flex items-center gap-2.5">
                            <FileImage
                              className="h-4 w-4 shrink-0 text-muted-foreground"
                              aria-hidden="true"
                            />
                            <div className="min-w-0 flex-1">
                              <div className="truncate font-medium text-foreground">
                                {basename(job.source_path)}
                              </div>
                              <div className="truncate text-2xs text-muted-foreground">
                                {job.input_format}
                                {job.selected_format && completed
                                  ? ` → ${job.selected_format}${job.selected_quality ? ` ${job.selected_quality}` : ""}`
                                  : ""}
                              </div>
                            </div>
                          </div>
                        </td>
                        <td
                          className={cn(
                            CELL_CLASSES,
                            "text-right font-mono text-xs text-muted-foreground",
                          )}
                        >
                          {formatBytes(job.original_size)}
                        </td>
                        <td
                          className={cn(
                            CELL_CLASSES,
                            "text-right font-mono text-xs text-muted-foreground",
                          )}
                        >
                          {job.final_size > 0 ? formatBytes(job.final_size) : "—"}
                        </td>
                        <td
                          className={cn(
                            CELL_CLASSES,
                            "text-right font-mono text-xs",
                            colorForReduction(pct),
                          )}
                        >
                          {completed ? formatPercent(pct) : "—"}
                        </td>
                        <td className={CELL_CLASSES}>
                          <StatusBadge status={job.status} />
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

      {/* Format capabilities */}
      <Card>
        <CardHeader
          title="Formatos soportados"
          subtitle="Capacidades de los backends de imagen cargados"
        />
        <CardContent flush>
          <div className="grid grid-cols-1 divide-y divide-border sm:grid-cols-2 sm:divide-y-0 lg:grid-cols-4 lg:[&>*]:border-l lg:[&>*:first-child]:border-l-0">
            {formats.map((f) => (
              <div
                key={f.format}
                className="flex flex-col gap-2 px-4 py-3"
              >
                <div className="flex items-center gap-2">
                  <FileImage
                    className="h-3.5 w-3.5 text-muted-foreground"
                    aria-hidden="true"
                  />
                  <span className="font-mono text-sm font-medium text-foreground">
                    {f.format}
                  </span>
                </div>
                <div className="flex flex-wrap gap-1">
                  <Tag tone={f.can_read ? "primary" : "neutral"}>
                    read
                  </Tag>
                  <Tag tone={f.can_write ? "primary" : "neutral"}>
                    write
                  </Tag>
                  <Tag tone={f.can_optimize ? "primary" : "neutral"}>
                    optimize
                  </Tag>
                  <Tag tone={f.can_convert ? "primary" : "neutral"}>
                    convert
                  </Tag>
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

DashboardPage.displayName = "DashboardPage";
