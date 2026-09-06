import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Toggle — switch binario. Botón con role="switch" + aria-checked:
 * semántica WAI-ARIA de switch sin input oculto ni checkbox estilizado.
 */
export interface ToggleProps {
  checked: boolean;
  onCheckedChange?: (checked: boolean) => void;
  disabled?: boolean;
  /** Etiqueta accesible — necesaria cuando no hay <label> visible. */
  "aria-label"?: string;
}

export const Toggle: React.FC<ToggleProps> = ({
  checked,
  onCheckedChange,
  disabled,
  "aria-label": ariaLabel,
}) => {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      disabled={disabled}
      onClick={() => onCheckedChange?.(!checked)}
      className={cn(
        "relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full",
        "transition-colors duration-fast ease-in-out-soft",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        "disabled:cursor-not-allowed disabled:opacity-50",
        checked ? "bg-primary" : "bg-active",
      )}
    >
      <span
        className={cn(
          "inline-block h-3.5 w-3.5 transform rounded-full bg-white shadow-sm transition-transform duration-fast ease-in-out-soft",
          checked ? "translate-x-[1.125rem]" : "translate-x-[0.125rem]",
        )}
      />
    </button>
  );
};
Toggle.displayName = "Toggle";
