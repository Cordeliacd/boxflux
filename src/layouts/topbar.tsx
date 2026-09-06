import * as React from "react";
import {
  Sun,
  Moon,
  Monitor,
} from "lucide-react";
import { useLocation } from "react-router-dom";

import { cn } from "@/lib/utils";
import { DropdownMenu } from "@/components/ui/dropdown-menu";
import { BrowserModeBadge } from "@/components/ui/browser-mode-badge";
import type { Theme } from "@/types";
import { useSettings } from "@/state/settings-context";

interface TopbarProps {
  className?: string;
}

interface PageMeta {
  title: string;
  subtitle?: string;
}

const ROUTE_META: Record<string, PageMeta> = {
  "/": {
    title: "Panel",
    subtitle: "Resumen de actividad y métricas",
  },
  "/auto-optimize": {
    title: "Auto Optimize",
    subtitle: "Pipeline inteligente para una imagen",
  },
  "/optimize": {
    title: "Optimizar",
    subtitle: "Optimización por lote con motor inteligente",
  },
  "/queue": {
    title: "Cola",
    subtitle: "Trabajos en proceso y completados",
  },
  "/history": {
    title: "Historial",
    subtitle: "Sesiones pasadas de optimización",
  },
  "/settings": {
    title: "Ajustes",
    subtitle: "Configuración de la aplicación",
  },
  "/about": {
    title: "Acerca de",
    subtitle: "Versión y backends cargados",
  },
};

const THEME_ITEMS = [
  {
    id: "Light",
    label: "Claro",
    hint: "Tema claro",
    icon: <Sun className="h-4 w-4" aria-hidden="true" />,
  },
  {
    id: "Dark",
    label: "Oscuro",
    hint: "Tema oscuro",
    icon: <Moon className="h-4 w-4" aria-hidden="true" />,
  },
  {
    id: "System",
    label: "Sistema",
    hint: "Seguir la preferencia del SO",
    icon: <Monitor className="h-4 w-4" aria-hidden="true" />,
  },
];

const THEME_ICON: Record<Theme, React.ElementType> = {
  Light: Sun,
  Dark: Moon,
  System: Monitor,
};

export const Topbar: React.FC<TopbarProps> = ({ className }) => {
  const location = useLocation();
  const { settings, update } = useSettings();

  const meta =
    ROUTE_META[location.pathname] ??
    (location.pathname === "/" ? ROUTE_META["/"] : undefined);

  const currentTheme: Theme = settings?.theme ?? "Light";
  const CurrentThemeIcon = THEME_ICON[currentTheme];

  const handleThemeChange = (id: string) => {
    if (!settings) return;
    void update({ ...settings, theme: id as Theme });
  };

  return (
    <header className={cn("app-shell__topbar", className)}>
      {meta ? (
        <div className="flex min-w-0 flex-col">
          <h1 className="truncate text-sm font-semibold text-foreground">
            {meta.title}
          </h1>
          {meta.subtitle ? (
            <span className="truncate text-xs text-muted-foreground">
              {meta.subtitle}
            </span>
          ) : null}
        </div>
      ) : null}
      <div className="ml-auto flex items-center gap-2">
        <BrowserModeBadge />
        <div className="shrink-0">
          <DropdownMenu
            triggerLabel={
              currentTheme === "System"
                ? "Sistema"
                : currentTheme === "Dark"
                  ? "Oscuro"
                  : "Claro"
            }
            triggerIcon={
              <CurrentThemeIcon className="h-4 w-4" aria-hidden="true" />
            }
            variant="ghost"
            size="sm"
            items={THEME_ITEMS}
            selectedId={currentTheme}
            onSelect={handleThemeChange}
          />
        </div>
      </div>
    </header>
  );
};
