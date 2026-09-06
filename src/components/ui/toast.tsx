import * as React from "react";
import { CheckCircle2, AlertCircle, Info, AlertTriangle, X } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * Toast — notificación efímera (4s por defecto). Máximo 3 simultáneos:
 * los más viejos se descartan al añadir uno nuevo. role="alert" solo
 * para error/warning (interrumpir); el resto va como "status" dentro
 * de un contenedor aria-live="polite".
 */
type ToastVariant = "success" | "error" | "info" | "warning";

export interface Toast {
  id: number;
  title: string;
  description?: string;
  variant: ToastVariant;
  /** Auto-dismiss after N ms. 0 = manual close only. Default: 4000. */
  duration?: number;
}

interface ToastContextValue {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id">) => number;
  dismiss: (id: number) => void;
}

const ToastContext = React.createContext<ToastContextValue | null>(null);

const MAX_VISIBLE_TOASTS = 3;

let nextToastId = 1;

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [toasts, setToasts] = React.useState<Toast[]>([]);

  const dismiss = React.useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const push = React.useCallback(
    (toast: Omit<Toast, "id">) => {
      const id = nextToastId++;
      const fullToast: Toast = { id, duration: 4000, ...toast };
      setToasts((prev) => {
        const next = [...prev, fullToast];
        return next.slice(Math.max(0, next.length - MAX_VISIBLE_TOASTS));
      });
      if (fullToast.duration && fullToast.duration > 0) {
        setTimeout(() => dismiss(id), fullToast.duration);
      }
      return id;
    },
    [dismiss],
  );

  const value = React.useMemo<ToastContextValue>(
    () => ({ toasts, push, dismiss }),
    [toasts, push, dismiss],
  );

  return (
    <ToastContext.Provider value={value}>
      {children}
      <ToastViewport toasts={toasts} dismiss={dismiss} />
    </ToastContext.Provider>
  );
};

export function useToast(): ToastContextValue {
  const ctx = React.useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast debe usarse dentro de <ToastProvider>");
  }
  return ctx;
}

const VARIANT_CONFIG: Record<
  ToastVariant,
  { icon: React.ElementType; className: string; role: string }
> = {
  success: {
    icon: CheckCircle2,
    className: "border-success/30 bg-success-subtle text-success-foreground",
    role: "status",
  },
  error: {
    icon: AlertCircle,
    className: "border-error/30 bg-error-subtle text-error-foreground",
    role: "alert",
  },
  info: {
    icon: Info,
    className: "border-border bg-surface-elevated text-foreground shadow-lg",
    role: "status",
  },
  warning: {
    icon: AlertTriangle,
    className: "border-warning/30 bg-warning-subtle text-warning-foreground",
    role: "alert",
  },
};

const ToastViewport: React.FC<{
  toasts: Toast[];
  dismiss: (id: number) => void;
}> = ({ toasts, dismiss }) => {
  if (toasts.length === 0) return null;
  return (
    <div
      className="pointer-events-none fixed bottom-6 right-6 z-50 flex w-[22rem] flex-col gap-2"
      aria-live="polite"
      aria-atomic="true"
    >
      {toasts.map((t) => {
        const cfg = VARIANT_CONFIG[t.variant];
        const Icon = cfg.icon;
        return (
          <div
            key={t.id}
            role={cfg.role}
            className={cn(
              "pointer-events-auto flex items-start gap-3 rounded-md border px-3.5 py-3 shadow-lg",
              "toast-enter",
              cfg.className,
            )}
          >
            <Icon className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium leading-snug">{t.title}</div>
              {t.description ? (
                <div className="mt-0.5 text-xs leading-relaxed opacity-80">
                  {t.description}
                </div>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => dismiss(t.id)}
              className="shrink-0 rounded-xs p-1 text-current opacity-60 transition-opacity hover:bg-black/5 hover:opacity-100 dark:hover:bg-white/10"
              aria-label="Cerrar notificación"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
        );
      })}
    </div>
  );
};
