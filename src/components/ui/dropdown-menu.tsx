import * as React from "react";
import { ChevronDown } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * DropdownMenu — menú desplegable simple, sin dependencias externas.
 *
 * Teclado: flechas arriba/abajo mueven el resaltado, Enter/Espacio
 * selecciona, Escape cierra y devuelve el foco al trigger.
 */
export interface DropdownMenuItem {
  id: string;
  label: string;
  hint?: string;
  icon?: React.ReactNode;
  disabled?: boolean;
}

export interface DropdownMenuProps {
  triggerLabel: string;
  triggerIcon?: React.ReactNode;
  variant?: "primary" | "secondary" | "ghost" | "subtle" | "danger";
  size?: "sm" | "md" | "lg";
  items: DropdownMenuItem[];
  selectedId?: string;
  onSelect: (id: string) => void;
  placeholder?: string;
  className?: string;
  disabled?: boolean;
  align?: "start" | "end";
}

const VARIANT_CLASSES: Record<
  NonNullable<DropdownMenuProps["variant"]>,
  string
> = {
  primary:
    "bg-primary text-primary-foreground hover:bg-primary-hover active:bg-primary-active shadow-sm",
  secondary:
    "bg-surface-elevated text-foreground border border-border hover:bg-hover hover:border-border-strong active:bg-active",
  ghost:
    "bg-transparent text-muted-foreground hover:bg-hover hover:text-foreground active:bg-active",
  subtle:
    "bg-primary-subtle text-foreground hover:bg-selected active:bg-active",
  danger:
    "bg-transparent text-error border border-error/40 hover:bg-error-subtle active:bg-error-subtle",
};

const SIZE_CLASSES: Record<NonNullable<DropdownMenuProps["size"]>, string> = {
  sm: "h-7 px-2.5 text-xs gap-1.5 rounded-sm",
  md: "h-8 px-3 text-sm gap-2 rounded-md",
  lg: "h-9 px-4 text-sm gap-2 rounded-md",
};

export const DropdownMenu: React.FC<DropdownMenuProps> = ({
  triggerLabel,
  triggerIcon,
  variant = "secondary",
  size = "md",
  items,
  selectedId,
  onSelect,
  placeholder,
  className,
  disabled,
  align = "start",
}) => {
  const [open, setOpen] = React.useState(false);
  const [focusedIndex, setFocusedIndex] = React.useState(-1);
  const triggerRef = React.useRef<HTMLButtonElement | null>(null);
  const menuRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const target = e.target as Node;
      if (
        menuRef.current &&
        !menuRef.current.contains(target) &&
        triggerRef.current &&
        !triggerRef.current.contains(target)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  React.useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  React.useEffect(() => {
    if (open) {
      const initial = items.findIndex((i) => i.id === selectedId);
      setFocusedIndex(initial >= 0 ? initial : 0);
    }
  }, [open, items, selectedId]);

  const handleTriggerKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
      e.preventDefault();
      setOpen(true);
    }
  };

  const handleMenuKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setFocusedIndex((i) => Math.min(i + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setFocusedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      const item = items[focusedIndex];
      if (item && !item.disabled) {
        onSelect(item.id);
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
  };

  const handleItemClick = (id: string) => {
    onSelect(id);
    setOpen(false);
    triggerRef.current?.focus();
  };

  const selected = items.find((i) => i.id === selectedId);
  const label = selected ? selected.label : (placeholder ?? triggerLabel);

  return (
    <div className={cn("relative inline-block", className)} ref={menuRef}>
      <button
        ref={triggerRef}
        type="button"
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={handleTriggerKey}
        className={cn(
          "inline-flex select-none items-center justify-center whitespace-nowrap font-medium",
          "transition-colors duration-fast ease-out-soft",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
          "disabled:pointer-events-none disabled:opacity-50",
          VARIANT_CLASSES[variant],
          SIZE_CLASSES[size],
        )}
      >
        {triggerIcon ? (
          <span
            className="flex h-4 w-4 items-center justify-center"
            aria-hidden="true"
          >
            {triggerIcon}
          </span>
        ) : null}
        <span className="max-w-[14rem] truncate">{label}</span>
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 transition-transform duration-fast ease-out-soft",
            open && "rotate-180",
          )}
          aria-hidden="true"
        />
      </button>
      {open ? (
        <div
          role="menu"
          onKeyDown={handleMenuKey}
          className={cn(
            "absolute z-50 mt-1.5 min-w-[16rem] max-w-[22rem] overflow-hidden rounded-md border border-border bg-surface-elevated shadow-lg",
            "fade-in slide-up",
            align === "end" ? "right-0" : "left-0",
          )}
        >
          <ul className="max-h-[20rem] overflow-y-auto py-1">
            {items.map((item, idx) => {
              const isSel = item.id === selectedId;
              const isFocused = idx === focusedIndex;
              return (
                <li key={item.id} role="none">
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={isSel}
                    disabled={item.disabled}
                    onMouseEnter={() => setFocusedIndex(idx)}
                    onClick={() => handleItemClick(item.id)}
                    className={cn(
                      "flex w-full items-start gap-2.5 px-3 py-2 text-left text-sm transition-colors",
                      "disabled:cursor-not-allowed disabled:opacity-50",
                      isFocused && !item.disabled && "bg-hover",
                      isSel ? "text-primary" : "text-foreground",
                    )}
                  >
                    {item.icon ? (
                      <span
                        className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center"
                        aria-hidden="true"
                      >
                        {item.icon}
                      </span>
                    ) : null}
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">
                        {item.label}
                      </span>
                      {item.hint ? (
                        <span className="mt-0.5 block text-xs leading-relaxed text-muted-foreground">
                          {item.hint}
                        </span>
                      ) : null}
                    </span>
                    {isSel ? (
                      <span
                        className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
                        aria-hidden="true"
                      />
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}
    </div>
  );
};
DropdownMenu.displayName = "DropdownMenu";
