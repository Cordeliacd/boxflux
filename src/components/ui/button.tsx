import * as React from "react";
import { Slot } from "@/components/ui/slot";
import { cn } from "@/lib/utils";

/**
 * Button — primitive de acción.
 *
 * La jerarquía visual baja de primary (acción principal) a secondary,
 * ghost y subtle (acciones de baja prioridad en filas/tablas); danger
 * es transparente con borde error. type="button" por defecto para no
 * disparar submits accidentales dentro de un form.
 */
type Variant = "primary" | "secondary" | "ghost" | "subtle" | "danger";
type Size = "sm" | "md" | "lg";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  /** Si true, el contenido se trata como un elemento hijo único (Slot). */
  asChild?: boolean;
  /** Si true, muestra estado de carga (spinner). */
  loading?: boolean;
}

const VARIANT_CLASSES: Record<Variant, string> = {
  primary:
    "bg-primary text-primary-foreground hover:bg-primary-hover active:bg-primary-active shadow-sm",
  secondary:
    "bg-surface-elevated text-foreground border border-border hover:bg-hover hover:border-border-strong active:bg-active",
  ghost:
    "bg-transparent text-muted-foreground hover:bg-hover hover:text-foreground active:bg-active",
  subtle:
    "bg-primary-subtle text-foreground hover:bg-selected active:bg-active",
  danger:
    "bg-transparent text-error border border-error/40 hover:bg-error-subtle hover:border-error/60 active:bg-error-subtle",
};

const SIZE_CLASSES: Record<Size, string> = {
  sm: "h-7 px-2.5 text-xs gap-1.5 rounded-sm",
  md: "h-8 px-3 text-sm gap-2 rounded-md",
  lg: "h-9 px-4 text-sm gap-2 rounded-md",
};

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant = "secondary",
      size = "md",
      asChild = false,
      loading = false,
      disabled,
      type = "button",
      children,
      ...props
    },
    ref,
  ) => {
    const classes = cn(
      "inline-flex select-none items-center justify-center whitespace-nowrap font-medium",
      "transition-colors duration-fast ease-out-soft",
      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
      "disabled:pointer-events-none disabled:opacity-50 disabled:cursor-not-allowed",
      VARIANT_CLASSES[variant],
      SIZE_CLASSES[size],
      className,
    );
    const content = (
      <>
        {loading ? (
          <LoaderIcon className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        ) : null}
        {children}
      </>
    );
    if (asChild) {
      return (
        <Slot className={classes} ref={ref} {...props}>
          {content}
        </Slot>
      );
    }
    return (
      <button
        ref={ref}
        className={classes}
        aria-disabled={disabled || loading || undefined}
        disabled={disabled || loading}
        type={type}
        {...props}
      >
        {content}
      </button>
    );
  },
);
Button.displayName = "Button";

/**
 * LoaderIcon — spinner simple sin dependencia externa.
 * SVG inline para que herede `currentColor` y sea animable via Tailwind.
 */
const LoaderIcon: React.FC<{ className?: string }> = ({ className }) => (
  <svg
    className={className}
    viewBox="0 0 24 24"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <circle
      cx="12"
      cy="12"
      r="10"
      stroke="currentColor"
      strokeWidth="3"
      strokeOpacity="0.25"
    />
    <path
      d="M22 12a10 10 0 0 0-10-10"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
    />
  </svg>
);
