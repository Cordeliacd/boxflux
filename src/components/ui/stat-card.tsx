import * as React from "react";
import type { LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * StatCard — bloque de métrica individual: caption → valor → subtítulo.
 * `tone` colorea el valor (y el borde si `highlighted`); reservarlo
 * para datos que importan de verdad, no para decorar.
 */
export type StatCardTone = "default" | "success" | "warning" | "primary";

export interface StatCardProps {
  /** Etiqueta corta (eyebrow, uppercase). */
  title: string;
  /** Valor principal (string ya formateado). */
  value: string;
  /** Subtítulo / contexto adicional. */
  subtitle?: string;
  /** Indicador de cambio opcional, p.ej. "+12% esta semana". */
  delta?: string;
  /** Tono del valor. Default: foreground. */
  tone?: StatCardTone;
  /** Override explícito de la clase de color del valor (alternativa a `tone`). */
  valueClassName?: string;
  /** Icono opcional (Lucide). Tamaño 14px en el header. */
  icon?: LucideIcon;
  /** Si true, resalta el borde con el tono activo. */
  highlighted?: boolean;
  className?: string;
}

const TONE_VALUE: Record<StatCardTone, string> = {
  default: "text-foreground",
  success: "text-success",
  warning: "text-warning",
  primary: "text-primary",
};

const TONE_BORDER: Record<StatCardTone, string> = {
  default: "border-border",
  success: "border-success/30",
  warning: "border-warning/30",
  primary: "border-primary/30",
};

export const StatCard: React.FC<StatCardProps> = ({
  title,
  value,
  subtitle,
  delta,
  tone = "default",
  valueClassName,
  icon: Icon,
  highlighted,
  className,
}) => {
  return (
    <div
      className={cn(
        "rounded-lg border bg-surface-elevated p-3.5 transition-colors",
        highlighted ? TONE_BORDER[tone] : "border-border",
        className,
      )}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="caption truncate">{title}</span>
        {Icon ? (
          <Icon
            className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
            aria-hidden="true"
          />
        ) : null}
      </div>
      <div
        className={cn(
          "mt-1.5 font-mono text-2xl font-semibold tabular-nums leading-tight",
          valueClassName ?? TONE_VALUE[tone],
        )}
      >
        {value}
      </div>
      <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
        {subtitle ? <span className="truncate">{subtitle}</span> : null}
        {delta ? (
          <span className="ml-auto shrink-0 font-mono text-2xs">{delta}</span>
        ) : null}
      </div>
    </div>
  );
};
StatCard.displayName = "StatCard";
