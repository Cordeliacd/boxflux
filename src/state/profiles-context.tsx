import * as React from "react";

import type { OptimizationProfile } from "@/types";
import * as tauri from "@/services/tauri";
import { isNotInTauriError } from "@/lib/tauri-safe";

/**
 * Solo lectura: la UI consume la lista de perfiles y el default
 * (selector "Perfil por defecto" en Ajustes).
 */
interface ProfilesContextValue {
  profiles: OptimizationProfile[];
  defaultName: string | null;
  loading: boolean;
  notInTauri: boolean;
  refresh: () => Promise<void>;
  setDefault: (name: string) => Promise<boolean>;
}

const ProfilesContext = React.createContext<ProfilesContextValue | null>(null);

// Perfiles de muestra para navegador sin Tauri.
const BROWSER_FALLBACK_PROFILES: OptimizationProfile[] = [
  {
    name: "Web",
    kind: "Web",
    jpeg_quality: 85,
    jpeg_progressive: true,
    webp_quality: 82,
    webp_lossless: false,
    avif_quality: 60,
    avif_alpha_quality: 80,
    png_optimization_level: 4,
    metadata_mode: "RemoveSafe",
    preserve_color_profile: true,
    jpeg_chroma_444: true,
    resize: null,
  },
  {
    name: "Lossless",
    kind: "Lossless",
    jpeg_quality: 100,
    jpeg_progressive: false,
    webp_quality: 100,
    webp_lossless: true,
    avif_quality: 100,
    avif_alpha_quality: 100,
    png_optimization_level: 5,
    metadata_mode: "Keep",
    preserve_color_profile: true,
    jpeg_chroma_444: true,
    resize: null,
  },
  {
    name: "Custom",
    kind: "Custom",
    jpeg_quality: 85,
    jpeg_progressive: true,
    webp_quality: 82,
    webp_lossless: false,
    avif_quality: 60,
    avif_alpha_quality: 80,
    png_optimization_level: 4,
    metadata_mode: "RemoveSafe",
    preserve_color_profile: true,
    jpeg_chroma_444: true,
    resize: null,
  },
];

export const ProfilesProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [profiles, setProfiles] = React.useState<OptimizationProfile[]>([]);
  const [defaultName, setDefaultName] = React.useState<string | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [notInTauri, setNotInTauri] = React.useState(false);

  const refresh = React.useCallback(async () => {
    try {
      const [p, d] = await Promise.all([
        tauri.getAllProfiles(),
        tauri.getDefaultProfile(),
      ]);
      setProfiles(p);
      setDefaultName(d);
      setNotInTauri(false);
    } catch (e) {
      if (isNotInTauriError(e)) {
        setNotInTauri(true);
        setProfiles(BROWSER_FALLBACK_PROFILES);
        setDefaultName("Web");
      } else {
        console.error("Profiles refresh failed:", e);
      }
    } finally {
      setLoading(false);
    }
  }, []);

  const setDefault = React.useCallback(
    async (name: string) => {
      try {
        const result = await tauri.setDefaultProfile(name);
        if (result) await refresh();
        return result;
      } catch (e) {
        if (isNotInTauriError(e)) {
          setNotInTauri(true);
          return false;
        }
        console.error("Profile setDefault failed:", e);
        return false;
      }
    },
    [refresh],
  );

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const value = React.useMemo<ProfilesContextValue>(
    () => ({
      profiles,
      defaultName,
      loading,
      notInTauri,
      refresh,
      setDefault,
    }),
    [profiles, defaultName, loading, notInTauri, refresh, setDefault],
  );

  return (
    <ProfilesContext.Provider value={value}>
      {children}
    </ProfilesContext.Provider>
  );
};

export function useProfiles(): ProfilesContextValue {
  const ctx = React.useContext(ProfilesContext);
  if (!ctx) {
    throw new Error("useProfiles debe usarse dentro de <ProfilesProvider>");
  }
  return ctx;
}
