import * as React from "react";

import { ErrorBannerViewport } from "@/components/ui/error-banner";
import { Sidebar } from "@/layouts/sidebar";
import { Statusbar } from "@/layouts/statusbar";
import { Topbar } from "@/layouts/topbar";

interface AppShellProps {
  children: React.ReactNode;
}

// Grid con áreas definidas en globals.css: topbar arriba, banner de
// errores debajo, sidebar a la izquierda, contenido y statusbar abajo.
// max-w-content solo en el wrapper interior para que las tablas
// full-width respiren.
export const AppShell: React.FC<AppShellProps> = ({ children }) => {
  return (
    <div className="app-shell">
      <Topbar />
      <Sidebar />
      <main className="app-shell__main">
        <ErrorBannerViewport />
        <div className="mx-auto max-w-content px-7 py-5">{children}</div>
      </main>
      <Statusbar />
    </div>
  );
};
