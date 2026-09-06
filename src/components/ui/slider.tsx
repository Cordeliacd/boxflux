import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Slider — barra arrastrable para valores numéricos. El valor numérico
 * es clicable para escribir un número exacto.
 */
export interface SliderProps {
  /** Etiqueta visible arriba del slider. */
  label?: string;
  /** Valor actual. */
  value: number;
  /** Valor mínimo (default 0). */
  min?: number;
  /** Valor máximo (default 100). */
  max?: number;
  /** Incremento al arrastrar / flechas (default 1). */
  step?: number;
  /** Llamado cuando el valor cambia. */
  onChange?: (value: number) => void;
  /** Texto opcional al lado del valor, p.ej. "%" o "MB" o "workers". */
  unit?: string;
  /** Texto que se muestra cuando value === min (p.ej. "automático"). */
  zeroLabel?: string;
  /** Deshabilita el slider. */
  disabled?: boolean;
  /** Texto de ayuda mostrado debajo. */
  helpText?: string;
  /** className extra para el contenedor. */
  className?: string;
}

export const Slider: React.FC<SliderProps> = ({
  label,
  value,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  unit,
  zeroLabel,
  disabled,
  helpText,
  className,
}) => {
  const [editing, setEditing] = React.useState(false);
  const [editValue, setEditValue] = React.useState(String(value));
  const inputRef = React.useRef<HTMLInputElement | null>(null);

  // No pisar lo que teclea el usuario mientras edita.
  React.useEffect(() => {
    if (!editing) {
      setEditValue(String(value));
    }
  }, [value, editing]);

  React.useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  const clamp = (v: number) => Math.max(min, Math.min(max, v));
  const pct = ((value - min) / Math.max(1, max - min)) * 100;

  const displayValue =
    value === min && zeroLabel ? zeroLabel : `${value}${unit ?? ""}`;

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
      e.preventDefault();
      onChange?.(clamp(value - step));
    } else if (e.key === "ArrowRight" || e.key === "ArrowUp") {
      e.preventDefault();
      onChange?.(clamp(value + step));
    } else if (e.key === "Home") {
      e.preventDefault();
      onChange?.(min);
    } else if (e.key === "End") {
      e.preventDefault();
      onChange?.(max);
    }
  };

  const handleEditSubmit = () => {
    const parsed = Number(editValue);
    if (!Number.isNaN(parsed)) {
      onChange?.(clamp(parsed));
    } else {
      setEditValue(String(value));
    }
    setEditing(false);
  };

  const handleEditKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleEditSubmit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      setEditValue(String(value));
      setEditing(false);
    }
  };

  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label || editing ? (
        <div className="flex items-center justify-between gap-2">
          {label ? (
            <label className="text-xs font-medium text-muted-foreground">
              {label}
            </label>
          ) : <span />}
          {editing ? (
            <input
              ref={inputRef}
              type="number"
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              onBlur={handleEditSubmit}
              onKeyDown={handleEditKeyDown}
              className="h-5 w-20 rounded-sm border border-border-focus bg-surface px-1.5 text-xs text-foreground focus:outline-none"
              min={min}
              max={max}
              step={step}
            />
          ) : (
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="rounded-xs px-1.5 py-0.5 font-mono text-xs text-foreground hover:bg-hover transition-colors"
              title="Click para escribir un valor exacto"
            >
              {displayValue}
            </button>
          )}
        </div>
      ) : null}
      <div className="relative flex items-center h-4">
        {/* Track de fondo */}
        <div
          className={cn(
            "absolute inset-x-0 h-1 rounded-full bg-active",
            disabled && "opacity-50",
          )}
          aria-hidden="true"
        />
        {/* Track de progreso */}
        <div
          className="absolute h-1 rounded-full bg-primary transition-[width] duration-fast ease-out-soft"
          style={{ width: `${pct}%` }}
          aria-hidden="true"
        />
        {/* Thumb visual */}
        <div
          className={cn(
            "pointer-events-none absolute h-3.5 w-3.5 -translate-x-1/2 rounded-full border-2 border-primary bg-surface shadow-sm transition-transform duration-fast ease-out-soft",
            "hover:scale-110",
            disabled && "opacity-50",
          )}
          style={{ left: `${pct}%` }}
          aria-hidden="true"
        />
        {/* El input range real va invisible encima del track: captura drag, teclado y foco. El resto es decorativo. */}
        <input
          type="range"
          value={value}
          min={min}
          max={max}
          step={step}
          onChange={(e) => onChange?.(Number(e.target.value))}
          onKeyDown={handleKeyDown}
          disabled={disabled}
          className={cn(
            "absolute inset-0 h-full w-full cursor-pointer opacity-0",
            "disabled:cursor-not-allowed",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
          )}
          aria-label={label}
          role="slider"
          aria-valuemin={min}
          aria-valuemax={max}
          aria-valuenow={value}
          aria-valuetext={displayValue}
        />
      </div>
      {helpText ? (
        <p className="text-xs leading-relaxed text-muted-foreground">
          {helpText}
        </p>
      ) : null}
    </div>
  );
};
Slider.displayName = "Slider";
