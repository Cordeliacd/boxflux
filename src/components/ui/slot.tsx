/**
 * Slot — soporte mínimo de `asChild`: mergea su className en el hijo
 * y lo renderiza directamente (p.ej., un `<a>` con estilos de Button).
 * Versión reducida del patrón de Radix; solo admite un hijo.
 */
import * as React from "react";

export interface SlotProps extends React.HTMLAttributes<HTMLElement> {
  children?: React.ReactNode;
}

export const Slot = React.forwardRef<HTMLElement, SlotProps>(
  ({ children, ...props }, ref) => {
    if (React.isValidElement(children)) {
      const child = children as React.ReactElement<Record<string, unknown>>;
      const childProps = child.props;
      const mergedClassName = [props.className, childProps.className as string | undefined]
        .filter(Boolean)
        .join(" ");
      return React.cloneElement(child, {
        ...props,
        ...childProps,
        className: mergedClassName,
        ref,
      });
    }
    if (React.Children.count(children) > 1) {
      throw new Error("Slot solo soporta un único hijo");
    }
    return null;
  },
);
Slot.displayName = "Slot";
