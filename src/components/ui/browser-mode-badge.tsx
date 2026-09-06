import * as React from "react";
import { Globe } from "lucide-react";

import { cn } from "@/lib/utils";
import { isTauriEnvironment } from "@/lib/tauri-safe";

/**
 * BrowserModeBadge — indicador permanente y sutil de que la app corre
 * en navegador, sin backend Rust. Informativo (tono neutral), no es
 * un error ni pide acción — para eso está el ErrorBanner.
 */
export const BrowserModeBadge: React.FC<{ className?: string }> = ({
  className,
}) => {
  if (isTauriEnvironment()) return null;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-xs border border-border bg-surface-inset px-2 py-0.5 text-2xs text-muted-foreground",
        className,
      )}
      title="Modo navegador — el backend Rust no está disponible. Ejecuta `pnpm tauri:dev` para activar todas las funciones."
    >
      <Globe className="h-2.5 w-2.5" aria-hidden="true" />
      Modo navegador
    </span>
  );
};
