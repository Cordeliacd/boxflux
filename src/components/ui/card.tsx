import * as React from "react";

import { cn } from "@/lib/utils";

/**
 * Card — superficie elevada para agrupar contenido relacionado.
 * Header/Content/Footer mantienen el ritmo vertical 12-16-12 px;
 * `flush` quita padding o borde para tablas y listas.
 */
export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  /** Si true, elimina la sombra (para cards anidados). */
  flat?: boolean;
}

export const Card = React.forwardRef<HTMLDivElement, CardProps>(
  ({ className, flat, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        "rounded-lg border border-border bg-surface-elevated",
        flat ? "" : "shadow-md",
        className,
      )}
      {...props}
    />
  ),
);
Card.displayName = "Card";

export interface CardHeaderProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** Eyebrow — etiqueta pequeña sobre el título (opcional). */
  eyebrow?: string;
  title: string;
  /** Subtítulo / descripción corta debajo del título. */
  subtitle?: string;
  /** Acción alineada a la derecha (botón, dropdown, etc.). */
  action?: React.ReactNode;
  /** Si true, elimina el border inferior (para headers con menos énfasis). */
  flush?: boolean;
}

export const CardHeader = React.forwardRef<HTMLDivElement, CardHeaderProps>(
  (
    { className, eyebrow, title, subtitle, action, flush, ...props },
    ref,
  ) => (
    <div
      ref={ref}
      className={cn(
        "flex items-start justify-between gap-4 px-4 py-3",
        !flush && "border-b border-border",
        className,
      )}
      {...props}
    >
      <div className="min-w-0 flex-1">
        {eyebrow ? (
          <p className="caption mb-1">{eyebrow}</p>
        ) : null}
        <h3 className="truncate text-lg font-semibold text-foreground">
          {title}
        </h3>
        {subtitle ? (
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
            {subtitle}
          </p>
        ) : null}
      </div>
      {action ? (
        <div className="flex shrink-0 items-center gap-1.5">{action}</div>
      ) : null}
    </div>
  ),
);
CardHeader.displayName = "CardHeader";

export const CardContent = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement> & { flush?: boolean }
>(({ className, flush, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(flush ? "" : "p-4", className)}
    {...props}
  />
));
CardContent.displayName = "CardContent";

export const CardFooter = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(
      "flex items-center gap-2 border-t border-border px-4 py-3",
      className,
    )}
    {...props}
  />
));
CardFooter.displayName = "CardFooter";
