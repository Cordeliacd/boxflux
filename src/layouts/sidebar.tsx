import * as React from "react";
import {
  Home as HomeIcon,
  WandSparkles,
  Sparkle,
  History as HistoryIcon,
  ListTodo,
  Settings as SettingsIcon,
  Info as InfoIcon,
  Boxes,
} from "lucide-react";
import { NavLink, useLocation } from "react-router-dom";

import { useEngine } from "@/state/engine-context";
import { cn } from "@/lib/utils";

interface NavItemDef {
  to: string;
  label: string;
  icon: React.ElementType;
  /** Descripción corta para tooltip / accesibilidad. */
  description?: string;
}

const WORKSPACE_NAV: ReadonlyArray<NavItemDef> = [
  { to: "/", label: "Panel", icon: HomeIcon, description: "Resumen y actividad reciente" },
  { to: "/auto-optimize", label: "Auto Optimize", icon: WandSparkles, description: "Pipeline inteligente para una imagen" },
  { to: "/optimize", label: "Optimizar", icon: Sparkle, description: "Optimización por lote" },
  { to: "/history", label: "Historial", icon: HistoryIcon, description: "Sesiones de optimización pasadas" },
];

const SYSTEM_NAV: ReadonlyArray<NavItemDef> = [
  { to: "/queue", label: "Cola", icon: ListTodo, description: "Trabajos en proceso y completados" },
  { to: "/settings", label: "Ajustes", icon: SettingsIcon, description: "Configuración de la aplicación" },
  { to: "/about", label: "Acerca de", icon: InfoIcon, description: "Versión y backends cargados" },
];

export const Sidebar: React.FC = () => {
  const { info } = useEngine();
  return (
    <aside className="app-shell__sidebar">
      {/* Marca */}
      <div className="flex items-center gap-2 px-4 pb-3">
        <div className="flex h-7 w-7 items-center justify-center rounded-md bg-primary text-primary-foreground">
          <Boxes className="h-4 w-4" aria-hidden="true" />
        </div>
        <div className="flex flex-col">
          <span className="text-sm font-semibold text-foreground">BoxFlux</span>
          <span className="text-2xs text-muted-foreground">
            v0.4.1
          </span>
        </div>
      </div>

      <nav className="flex flex-1 flex-col gap-5 px-3 pt-1">
        <section className="flex flex-col gap-0.5">
          <span className="caption px-2 pb-1">Espacio de trabajo</span>
          {WORKSPACE_NAV.map((item) => (
            <NavLink key={item.to} to={item.to} end={item.to === "/"}>
              {({ isActive }) => (
                <NavItemRow active={isActive} item={item} />
              )}
            </NavLink>
          ))}
        </section>
        <section className="flex flex-col gap-0.5">
          <span className="caption px-2 pb-1">Sistema</span>
          {SYSTEM_NAV.map((item) => (
            <NavLink key={item.to} to={item.to}>
              {({ isActive }) => (
                <NavItemRow active={isActive} item={item} />
              )}
            </NavLink>
          ))}
        </section>
      </nav>

      {/* Footer con estado del engine */}
      <footer className="mt-auto border-t border-sidebar-border px-3 py-2">
        {info ? (
          <div className="flex flex-col gap-0.5">
            <div className="flex items-center justify-between text-2xs">
              <span className="text-muted-foreground/80">Engine</span>
              <span className="font-mono text-muted-foreground">
                v{info.engine_version}
              </span>
            </div>
            <div className="flex items-center justify-between text-2xs">
              <span className="text-muted-foreground/80">Workers</span>
              <span className="font-mono text-muted-foreground">
                {info.max_workers}
              </span>
            </div>
          </div>
        ) : (
          <div className="flex items-center gap-1.5 text-2xs text-muted-foreground/80">
            <span className="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-warning" />
            <span>Backend no disponible</span>
          </div>
        )}
      </footer>
    </aside>
  );
};

const NavItemRow: React.FC<{ active: boolean; item: NavItemDef }> = ({
  active,
  item,
}) => {
  const location = useLocation();
  const isActive =
    active || (item.to !== "/" && location.pathname.startsWith(item.to));
  const Icon = item.icon;
  return (
    <button
      type="button"
      tabIndex={0}
      title={item.description}
      aria-current={isActive ? "page" : undefined}
      className={cn(
        "group relative flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-sm transition-colors duration-fast ease-out-soft",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-sidebar",
        isActive
          ? "bg-selected font-medium text-foreground"
          : "text-muted-foreground hover:bg-hover hover:text-foreground",
      )}
    >
      {isActive ? (
        <span
          className="absolute left-0 top-1/2 h-4 w-0.5 -translate-y-1/2 rounded-r-full bg-primary"
          aria-hidden="true"
        />
      ) : null}
      <Icon
        className={cn(
          "h-4 w-4 shrink-0 transition-colors",
          isActive ? "text-foreground" : "text-muted-foreground group-hover:text-foreground",
        )}
        aria-hidden="true"
      />
      <span className="truncate">{item.label}</span>
    </button>
  );
};
