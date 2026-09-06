import * as React from "react";

import { cn } from "@/lib/utils";
import { subtleForStatus } from "@/lib/format";

/**
 * StatusBadge — badge de estado de trabajo: punto de color + label.
 * El color se deriva del status vía los tokens CSS --status-*
 * (`subtleForStatus`). `dotOnly` para tablas densas donde no cabe
 * el texto.
 */
export interface StatusBadgeProps
  extends React.HTMLAttributes<HTMLSpanElement> {
  status: string;
  /** Etiqueta custom (sobrescribe la traducción). */
  label?: string;
  /** Si true, solo muestra el punto (para tablas densas). */
  dotOnly?: boolean;
}

const STATUS_LABELS_ES: Record<string, string> = {
  Queued: "En cola",
  Processing: "Procesando",
  Completed: "Completado",
  Failed: "Fallido",
  Cancelled: "Cancelado",
  Skipped: "Omitido",
};

const STATUS_DOT_CLASS: Record<string, string> = {
  Queued: "bg-status-queued",
  Processing: "bg-status-processing",
  Completed: "bg-status-completed",
  Failed: "bg-status-failed",
  Cancelled: "bg-status-cancelled",
  Skipped: "bg-status-skipped",
};

export const StatusBadge = React.forwardRef<HTMLSpanElement, StatusBadgeProps>(
  ({ status, label, dotOnly, className, ...props }, ref) => {
    const displayLabel = label ?? STATUS_LABELS_ES[status] ?? status;
    const subtleClass = subtleForStatus(status);
    const dotClass =
      STATUS_DOT_CLASS[status] ?? "bg-status-queued";

    if (dotOnly) {
      return (
        <span
          ref={ref}
          className={cn("inline-flex h-2 w-2 rounded-full", dotClass, className)}
          aria-label={displayLabel}
          {...props}
        />
      );
    }

    return (
      <span
        ref={ref}
        className={cn(
          "inline-flex h-5 items-center gap-1.5 rounded-xs px-2 text-2xs font-medium",
          subtleClass,
          className,
        )}
        {...props}
      >
        <span className={cn("h-1.5 w-1.5 rounded-full", dotClass)} />
        {displayLabel}
      </span>
    );
  },
);
StatusBadge.displayName = "StatusBadge";
