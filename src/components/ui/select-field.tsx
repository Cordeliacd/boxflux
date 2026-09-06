import { ChevronDown } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * SelectField — <select> nativo con apariencia personalizada.
 *
 * Nativo a propósito: teclado, foco y lectores de pantalla gratis, y
 * en Tauri abre el popup nativo del SO. El ChevronDown va superpuesto
 * porque appearance-none elimina la flecha del navegador.
 */
export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  /** Si true, se deshabilita individualmente. */
  disabled?: boolean;
}

export interface SelectFieldProps<T extends string = string> {
  label?: string;
  /** Etiqueta pequeña sobre el select (eyebrow). */
  eyebrow?: string;
  /** Texto de ayuda debajo. */
  helpText?: string;
  value: T;
  options: ReadonlyArray<SelectOption<T>>;
  onChange?: (value: T) => void;
  disabled?: boolean;
  /** Placeholder cuando no hay valor. Si se pasa, se añade como primera option. */
  placeholder?: string;
  className?: string;
}

export function SelectField<T extends string = string>({
  label,
  eyebrow,
  helpText,
  value,
  options,
  onChange,
  disabled,
  placeholder,
  className,
}: SelectFieldProps<T>) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label || eyebrow ? (
        <div className="flex flex-col gap-0.5">
          {eyebrow ? <span className="caption">{eyebrow}</span> : null}
          {label ? (
            <label className="text-xs font-medium text-muted-foreground">
              {label}
            </label>
          ) : null}
        </div>
      ) : null}
      <div className="relative">
        <select
          value={value}
          onChange={(e) => onChange?.(e.target.value as T)}
          disabled={disabled}
          className={cn(
            "h-8 w-full appearance-none rounded-md border border-border bg-surface-inset px-2.5 pr-8 text-sm text-foreground",
            "transition-colors duration-fast ease-out-soft",
            "hover:border-border-strong",
            "focus:border-border-focus focus:outline-none focus:ring-2 focus:ring-ring/20 focus:ring-offset-0",
            "disabled:cursor-not-allowed disabled:opacity-50",
            "cursor-pointer",
          )}
        >
          {placeholder ? (
            <option value="" disabled>
              {placeholder}
            </option>
          ) : null}
          {options.map((opt) => (
            <option key={opt.value} value={opt.value} disabled={opt.disabled}>
              {opt.label}
            </option>
          ))}
        </select>
        <ChevronDown
          className="pointer-events-none absolute right-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground transition-transform"
          aria-hidden="true"
        />
      </div>
      {helpText ? (
        <p className="text-xs leading-relaxed text-muted-foreground">
          {helpText}
        </p>
      ) : null}
    </div>
  );
}
