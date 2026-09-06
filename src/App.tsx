import * as React from "react";

import { Routes, Route } from "react-router-dom";

import { ErrorBannerProvider } from "@/components/ui/error-banner";
import { ToastProvider } from "@/components/ui/toast";
import { AppShell } from "@/layouts/app-shell";
import { AutoOptimizeProvider } from "@/state/auto-optimize-context";
import { AppProviders } from "@/state/providers";
import { AboutPage } from "@/pages/about-page";
import { AutoOptimizerPage } from "@/pages/auto-optimizer-page";
import { DashboardPage } from "@/pages/dashboard-page";
import { HistoryPage } from "@/pages/history-page";
import { OptimizerPage } from "@/pages/optimizer-page";
import { QueuePage } from "@/pages/queue-page";
import { SettingsPage } from "@/pages/settings-page";

// Dos canales de feedback: Toasts (efímeros, acción OK/fallo) y el banner
// rojo persistente (errores que el usuario debe ver y descartar a mano).
export const App: React.FC = () => {
  return (
    <ToastProvider>
      <ErrorBannerProvider>
        <AppProviders>
          <AutoOptimizeProvider>
            <AppShell>
              <Routes>
                <Route path="/" element={<DashboardPage />} />
                <Route path="/auto-optimize" element={<AutoOptimizerPage />} />
                <Route path="/optimize" element={<OptimizerPage />} />
                <Route path="/queue" element={<QueuePage />} />
                <Route path="/history" element={<HistoryPage />} />
                <Route path="/settings" element={<SettingsPage />} />
                <Route path="/about" element={<AboutPage />} />
                <Route path="*" element={<DashboardPage />} />
              </Routes>
            </AppShell>
          </AutoOptimizeProvider>
        </AppProviders>
      </ErrorBannerProvider>
    </ToastProvider>
  );
};
