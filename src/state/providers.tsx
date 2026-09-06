import * as React from "react";

import { EngineProvider } from "@/state/engine-context";
import { HistoryProvider } from "@/state/history-context";
import { ProfilesProvider } from "@/state/profiles-context";
import { QueueProvider } from "@/state/queue-context";
import { SettingsProvider } from "@/state/settings-context";

/**
 * Cada context detecta por su cuenta si corre dentro de Tauri o en
 * navegador (`pnpm dev`) y usa fallbacks en vez de quedarse colgado
 * en "Cargando…".
 */
export const AppProviders: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  return (
    <SettingsProvider>
      <EngineProvider>
        <ProfilesProvider>
          <QueueProvider>
            <HistoryProvider>{children}</HistoryProvider>
          </QueueProvider>
        </ProfilesProvider>
      </EngineProvider>
    </SettingsProvider>
  );
};
