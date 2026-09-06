import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * IconButton — botón cuadrado solo con icono. type="button" por
 * defecto para evitar submits accidentales; el color de hover se
 * elige con `hoverColor` según la acción (p.ej. error para borrar).
 */
export interface IconButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** Color al hacer hover. Default: muted-foreground → foreground. */
  hoverColor?: "default" | "primary" | "error" | "warning";
  /** Tamaño. Default: md (32px). */
  size?: "sm" | "md";
}

const HOVER_CLASSES: Record<
  NonNullable<IconButtonProps["hoverColor"]>,
  string
> = {
  default: "hover:text-foreground",
  primary: "hover:text-primary",
  error: "hover:text-error",
  warning: "hover:text-warning",
};

const SIZE_CLASSES: Record<NonNullable<IconButtonProps["size"]>, string> = {
  sm: "h-7 w-7",
  md: "h-8 w-8",
};

export const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(
  (
    { className, hoverColor = "default", size = "md", type = "button", ...props },
    ref,
  ) => (
    <button
      ref={ref}
      type={type}
      className={cn(
        "inline-flex items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast ease-out-soft",
        SIZE_CLASSES[size],
        "hover:bg-hover",
        HOVER_CLASSES[hoverColor],
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
        "disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  ),
);
IconButton.displayName = "IconButton";
