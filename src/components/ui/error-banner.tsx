import * as React from "react";
import { AlertCircle, AlertTriangle, Info, X } from "lucide-react";

import { cn } from "@/lib/utils";

/**
 * AppError — error global persistente: se muestra arriba, donde no
 * se puede ignorar, y solo desaparece cuando el usuario lo cierra.
 * Los Toasts son lo contrario: efímeros (auto-dismiss 4s) y en la
 * esquina inferior. Usar uno u otro según la gravedad.
 */
export interface AppError {
  id: string;
  title: string;
  description?: string;
  code?: string;
  severity?: "error" | "warning" | "info";
}

interface ErrorBannerContextValue {
  errors: AppError[];
  pushError: (error: AppError) => void;
  dismissError: (id: string) => void;
  clearErrors: () => void;
}

const ErrorBannerContext = React.createContext<ErrorBannerContextValue | null>(
  null,
);

export const ErrorBannerProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [errors, setErrors] = React.useState<AppError[]>([]);

  const pushError = React.useCallback((error: AppError) => {
    setErrors((prev) => {
      const without = prev.filter((e) => e.id !== error.id);
      return [...without, error];
    });
  }, []);

  const dismissError = React.useCallback((id: string) => {
    setErrors((prev) => prev.filter((e) => e.id !== id));
  }, []);

  const clearErrors = React.useCallback(() => {
    setErrors([]);
  }, []);

  const value = React.useMemo<ErrorBannerContextValue>(
    () => ({ errors, pushError, dismissError, clearErrors }),
    [errors, pushError, dismissError, clearErrors],
  );

  return (
    <ErrorBannerContext.Provider value={value}>
      {children}
    </ErrorBannerContext.Provider>
  );
};

export function useErrorBanner(): ErrorBannerContextValue {
  const ctx = React.useContext(ErrorBannerContext);
  if (!ctx) {
    throw new Error("useErrorBanner debe usarse dentro de <ErrorBannerProvider>");
  }
  return ctx;
}

const SEVERITY_CONFIG: Record<
  NonNullable<AppError["severity"]>,
  { className: string; icon: React.ElementType }
> = {
  error: {
    className: "border-error/30 bg-error-subtle text-error-foreground",
    icon: AlertCircle,
  },
  warning: {
    className: "border-warning/30 bg-warning-subtle text-warning-foreground",
    icon: AlertTriangle,
  },
  info: {
    className: "border-border bg-surface-elevated text-foreground",
    icon: Info,
  },
};

/** Contenedor de errores activos; se apilan bajo el topbar. */
export const ErrorBannerViewport: React.FC = () => {
  const { errors, dismissError } = useErrorBanner();
  if (errors.length === 0) return null;
  return (
    <div
      className="flex flex-col gap-1.5 border-b border-border bg-surface px-4 py-2.5"
      role="alert"
      aria-live="assertive"
      aria-atomic="true"
    >
      {errors.map((err) => {
        const severity = err.severity ?? "error";
        const cfg = SEVERITY_CONFIG[severity];
        const Icon = cfg.icon;
        return (
          <div
            key={err.id}
            className={cn(
              "flex items-start gap-2.5 rounded-md border px-3 py-2 fade-in",
              cfg.className,
            )}
          >
            <Icon className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium leading-snug">
                {err.title}
              </div>
              {err.description ? (
                <div className="mt-0.5 break-words text-xs leading-relaxed opacity-90">
                  {err.description}
                </div>
              ) : null}
              {err.code ? (
                <div className="mt-1 font-mono text-2xs opacity-60">
                  Código: {err.code}
                </div>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => dismissError(err.id)}
              className="shrink-0 rounded-xs p-1 opacity-60 transition-opacity hover:bg-black/5 hover:opacity-100 dark:hover:bg-white/10"
              aria-label="Cerrar error"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
        );
      })}
    </div>
  );
};
