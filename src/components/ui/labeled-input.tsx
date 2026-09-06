import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * LabeledInput — input de texto/número con etiqueta y ayuda.
 * helpText se conecta via aria-describedby; `error` es solo visual,
 * la validación vive fuera de este componente.
 */
export interface LabeledInputProps {
  label?: string;
  eyebrow?: string;
  helpText?: string;
  value: string;
  onEdited?: (value: string) => void;
  placeholder?: string;
  password?: boolean;
  disabled?: boolean;
  /** Estado de error visual (no valida — solo muestra borde rojo). */
  error?: boolean;
  type?: "text" | "number" | "password" | "email";
  className?: string;
  /** Id del input; si no se pasa, se genera uno estable para el htmlFor. */
  id?: string;
}

let inputIdCounter = 0;
const useStableInputId = (providedId?: string) => {
  const stableId = React.useRef<string | null>(null);
  if (stableId.current === null) {
    if (providedId) {
      stableId.current = providedId;
    } else {
      inputIdCounter += 1;
      stableId.current = `bf-input-${inputIdCounter}`;
    }
  }
  return stableId.current;
};

export const LabeledInput: React.FC<LabeledInputProps> = ({
  label,
  eyebrow,
  helpText,
  value,
  onEdited,
  placeholder,
  password,
  disabled,
  error,
  type = "text",
  className,
  id,
}) => {
  const inputId = useStableInputId(id);
  const helpId = React.useId();
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label || eyebrow ? (
        <div className="flex flex-col gap-0.5">
          {eyebrow ? <span className="caption">{eyebrow}</span> : null}
          {label ? (
            <label htmlFor={id ?? inputId} className="text-xs font-medium text-muted-foreground">
              {label}
            </label>
          ) : null}
        </div>
      ) : null}
      <input
        id={id ?? inputId}
        type={password ? "password" : type}
        value={value}
        onChange={(e) => onEdited?.(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        aria-describedby={helpText ? helpId : undefined}
        aria-invalid={error || undefined}
        className={cn(
          "h-8 w-full rounded-md border bg-surface-inset px-2.5 text-sm text-foreground",
          "transition-colors duration-fast ease-out-soft",
          "placeholder:text-muted-foreground/60",
          "hover:border-border-strong",
          "focus:border-border-focus focus:outline-none focus:ring-2 focus:ring-ring/20",
          "disabled:cursor-not-allowed disabled:opacity-50",
          error
            ? "border-error focus:border-error focus:ring-error/20"
            : "border-border",
        )}
      />
      {helpText ? (
        <p
          id={helpId}
          className={cn(
            "text-xs leading-relaxed",
            error ? "text-error-foreground" : "text-muted-foreground",
          )}
        >
          {helpText}
        </p>
      ) : null}
    </div>
  );
};
LabeledInput.displayName = "LabeledInput";
