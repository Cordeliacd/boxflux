import * as React from "react";
import {
  Cpu,
  CheckCircle2,
  Loader2,
} from "lucide-react";

import { useEngine } from "@/state/engine-context";
import { useQueue } from "@/state/queue-context";
import { formatBytes, formatDuration } from "@/lib/format";
import { cn } from "@/lib/utils";

// Sin actividad no se pintan métricas vacías ("0B (0%)"): cada grupo
// del footer solo aparece si tiene algo útil que decir.
export const Statusbar: React.FC = () => {
  const { liveStats, info } = useEngine();
  const { stats } = useQueue();

  const workersActive = stats?.active_workers ?? 0;
  const workersMax = liveStats?.max_workers ?? info?.max_workers ?? 0;
  const filesProcessed = liveStats?.files_processed ?? 0;
  const filesQueued = liveStats?.files_queued ?? 0;
  const filesProcessing = liveStats?.files_processing ?? 0;
  const spaceSaved = Math.max(0, liveStats?.space_saved_bytes ?? 0);
  const compression = liveStats?.compression_percentage ?? 0;
  const speed = liveStats?.processing_speed_files_per_sec ?? 0;
  const totalTime = liveStats?.total_processing_ms ?? 0;

  const hasActivity = filesProcessed > 0 || filesQueued > 0 || filesProcessing > 0;
  const isWorking = workersActive > 0 || filesProcessing > 0;

  return (
    <footer className="app-shell__statusbar">
      {/* Grupo izquierdo: engine + workers */}
      <div className="flex items-center gap-3">
        <span className="flex items-center gap-1.5">
          <Cpu className="h-3 w-3 text-muted-foreground" aria-hidden="true" />
          <span className="font-mono text-2xs text-muted-foreground">
            {workersActive}/{workersMax} workers
          </span>
          {isWorking ? (
            <span
              className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-warning"
              aria-hidden="true"
            />
          ) : null}
        </span>
        {hasActivity ? (
          <>
            <Separator />
            <span className="flex items-center gap-1.5 font-mono text-2xs">
              <CheckCircle2
                className="h-3 w-3 text-success"
                aria-hidden="true"
              />
              <span className="text-muted-foreground">
                {filesProcessed} procesados
              </span>
            </span>
            {filesProcessing > 0 ? (
              <span className="flex items-center gap-1.5 font-mono text-2xs">
                <Loader2
                  className="h-3 w-3 animate-spin text-warning"
                  aria-hidden="true"
                />
                <span className="text-muted-foreground">
                  {filesProcessing} activos
                </span>
              </span>
            ) : null}
            {filesQueued > 0 ? (
              <span className="font-mono text-2xs text-muted-foreground">
                {filesQueued} en cola
              </span>
            ) : null}
          </>
        ) : null}
      </div>

      {/* Grupo derecho: métricas de compresión + copyright */}
      <div className="flex items-center gap-3">
        {hasActivity ? (
          <>
            <span className="font-mono text-2xs">
              <span className="text-primary font-medium">
                {formatBytes(spaceSaved)}
              </span>
              <span className="text-muted-foreground"> ahorrados</span>
            </span>
            <Separator />
            <span className="font-mono text-2xs">
              <span className="text-primary font-medium">
                {compression.toFixed(1)}%
              </span>
              <span className="text-muted-foreground"> compresión</span>
            </span>
            {speed > 0 ? (
              <>
                <Separator />
                <span className="font-mono text-2xs text-muted-foreground">
                  {speed.toFixed(1)} f/s
                </span>
              </>
            ) : null}
            {totalTime > 0 ? (
              <>
                <Separator />
                <span className="font-mono text-2xs text-muted-foreground">
                  {formatDuration(totalTime)}
                </span>
              </>
            ) : null}
          </>
        ) : (
          <span className="text-2xs text-muted-foreground/60">
            © 2026 Box Studio
          </span>
        )}
      </div>
    </footer>
  );
};

const Separator: React.FC = () => (
  <span className={cn("h-3 w-px bg-border")} aria-hidden="true" />
);
