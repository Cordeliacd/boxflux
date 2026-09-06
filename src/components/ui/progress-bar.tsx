import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * ProgressBar — barra de progreso con estado indeterminado. El modo
 * indeterminado usa la animación boxflux-progress-indeterminate
 * definida en globals.css.
 */
export interface ProgressBarProps {
  /** 0..1. Si es null o negativo, indeterminado. */
  value?: number | null;
  /** Tono del fill. Default: primary. */
  tone?: "primary" | "success" | "warning" | "error";
  className?: string;
}

const TONE_FILL: Record<NonNullable<ProgressBarProps["tone"]>, string> = {
  primary: "bg-primary",
  success: "bg-success",
  warning: "bg-warning",
  error: "bg-error",
};

export const ProgressBar: React.FC<ProgressBarProps> = ({
  value,
  tone = "primary",
  className,
}) => {
  const indeterminate = value === null || value === undefined || value < 0;
  const pct = indeterminate ? 0 : Math.max(0, Math.min(1, value));
  return (
    <div
      className={cn(
        "relative h-1 w-full overflow-hidden rounded-full bg-surface-inset",
        className,
      )}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={indeterminate ? undefined : pct}
    >
      {indeterminate ? (
        <div
          className={cn(
            "absolute h-full w-1/3 rounded-full boxflux-progress-indeterminate",
            TONE_FILL[tone],
          )}
        />
      ) : (
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-base ease-out-soft",
            TONE_FILL[tone],
          )}
          style={{ width: `${pct * 100}%` }}
        />
      )}
    </div>
  );
};
ProgressBar.displayName = "ProgressBar";
