import * as React from "react";

import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { LabeledInput } from "@/components/ui/labeled-input";
import { SelectField } from "@/components/ui/select-field";
import { Slider } from "@/components/ui/slider";
import { Toggle } from "@/components/ui/toggle";
import { useSettings } from "@/state/settings-context";
import { useProfiles } from "@/state/profiles-context";
import type { AppSettings, MetadataBehavior, OutputMode, Theme } from "@/types";
import { cn } from "@/lib/utils";

const SECTIONS = [
  { id: "general", label: "General" },
  { id: "optimization", label: "Optimización" },
  { id: "formats", label: "Formatos" },
  { id: "performance", label: "Rendimiento" },
  { id: "output", label: "Salida" },
  { id: "appearance", label: "Apariencia" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

export const SettingsPage: React.FC = () => {
  const { settings, update, updateDebounced } = useSettings();
  const { profiles, defaultName, setDefault } = useProfiles();
  const [activeSection, setActiveSection] = React.useState<SectionId>("appearance");

  if (!settings) {
    return <div className="text-sm text-muted-foreground">Cargando…</div>;
  }

  const patch = (next: Partial<AppSettings>) =>
    update({ ...settings, ...next });
  const patchDebounced = (next: Partial<AppSettings>) =>
    updateDebounced({ ...settings, ...next });

  const maxCores =
    typeof navigator !== "undefined" && navigator.hardwareConcurrency
      ? Math.max(4, navigator.hardwareConcurrency)
      : 8;

  return (
    <div className="flex gap-6">
      {/* Section nav */}
      <nav className="w-48 shrink-0 pt-1">
        <span className="caption px-2">Ajustes</span>
        <ul className="mt-2 flex flex-col gap-0.5">
          {SECTIONS.map((s) => (
            <li key={s.id}>
              <button
                type="button"
                onClick={() => setActiveSection(s.id)}
                className={cn(
                  "w-full rounded-md px-3 py-1.5 text-left text-sm transition-colors duration-fast ease-out-soft",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
                  activeSection === s.id
                    ? "bg-selected font-medium text-foreground"
                    : "text-muted-foreground hover:bg-hover hover:text-foreground",
                )}
              >
                {s.label}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      {/* Section content */}
      <div className="min-w-0 flex-1">
        <div className="flex w-full max-w-2xl flex-col gap-5">
          {activeSection === "appearance" ? (
          <Card>
            <CardHeader
              title="Apariencia"
              subtitle="Tema visual de la interfaz"
            />
            <CardContent className="flex flex-col gap-4">
              <SelectField<Theme>
                label="Tema"
                helpText="Elige Claro, Oscuro o sigue la preferencia del sistema operativo."
                value={settings.theme}
                options={[
                  { value: "Light", label: "Claro" },
                  { value: "Dark", label: "Oscuro" },
                  { value: "System", label: "Sistema" },
                ]}
                onChange={(v) => patch({ theme: v })}
              />
            </CardContent>
          </Card>
        ) : null}

        {activeSection === "general" ? (
          <Card>
            <CardHeader
              title="General"
              subtitle="Perfil por defecto e idioma"
            />
            <CardContent className="flex flex-col gap-4">
              <SelectField
                label="Perfil por defecto"
                helpText="El perfil aplicado cuando no se especifica uno manualmente."
                value={defaultName ?? "Web"}
                options={profiles.map((p) => ({
                  value: p.name,
                  label: p.name,
                }))}
                onChange={(v) => setDefault(v)}
              />
              <SelectField
                label="Idioma"
                helpText="Idioma de la interfaz. Solo español está implementado en esta versión."
                value={settings.language}
                options={[
                  { value: "es", label: "Español" },
                  { value: "en", label: "English (próximamente)" },
                ]}
                onChange={(v) => patch({ language: v })}
              />
            </CardContent>
          </Card>
        ) : null}

        {activeSection === "optimization" ? (
          <Card>
            <CardHeader
              title="Optimización"
              subtitle="Calidad por defecto y comportamiento de metadata"
            />
            <CardContent className="flex flex-col gap-5">
              <Slider
                label="Calidad por defecto"
                value={settings.default_quality}
                min={1}
                max={100}
                step={1}
                onChange={(v) => patchDebounced({ default_quality: v })}
                zeroLabel="1"
                helpText="Calidad objetivo (1-100) usada por el motor cuando el modo no es lossless. Valores más altos = mejor fidelidad, archivos más grandes."
              />
              <SelectField<MetadataBehavior>
                label="Comportamiento de metadata"
                helpText="Controla cómo se maneja la metadata EXIF/XMP/ICC al optimizar."
                value={settings.metadata_behavior}
                options={[
                  { value: "Keep", label: "Conservar todo" },
                  { value: "RemoveSafe", label: "Eliminar sensible" },
                  { value: "RemoveAll", label: "Eliminar todo" },
                ]}
                onChange={(v) => patch({ metadata_behavior: v })}
              />
            </CardContent>
          </Card>
        ) : null}

        {activeSection === "formats" ? (
          <Card>
            <CardHeader
              title="Formatos"
              subtitle="Formato de salida por defecto"
            />
            <CardContent className="flex flex-col gap-4">
              <SelectField
                label="Formato de salida por defecto"
                helpText="Auto preserva el formato original. Los demásfuerzan uno específico."
                value={settings.default_output_format || "auto"}
                options={[
                  { value: "auto", label: "Auto (preservar original)" },
                  { value: "PNG", label: "PNG" },
                  { value: "JPEG", label: "JPEG" },
                  { value: "WebP", label: "WebP" },
                  { value: "AVIF", label: "AVIF" },
                ]}
                onChange={(v) =>
                  patch({ default_output_format: v === "auto" ? "" : v })
                }
              />
            </CardContent>
          </Card>
        ) : null}

        {activeSection === "performance" ? (
          <Card>
            <CardHeader
              title="Rendimiento"
              subtitle="Workers, límite de CPU y memoria"
            />
            <CardContent className="flex flex-col gap-6">
              <Slider
                label="Workers concurrentes"
                value={settings.worker_count}
                min={0}
                max={maxCores}
                step={1}
                onChange={(v) => patchDebounced({ worker_count: v })}
                zeroLabel="automático"
                helpText={`0 = automático (usa todos los cores disponibles, hasta ${maxCores}). Valores más bajos dejan CPU libre para otras apps pero hacen la optimización más lenta.`}
              />
              <Slider
                label="Límite de CPU"
                value={settings.cpu_limit_percent}
                min={0}
                max={100}
                step={5}
                onChange={(v) => patchDebounced({ cpu_limit_percent: v })}
                unit="%"
                zeroLabel="sin límite"
                helpText="Porcentaje máximo de CPU que el motor puede usar. El backend ajusta el número de workers activos según este límite. 0% = sin límite."
              />
              <Slider
                label="Memoria máxima"
                value={Math.min(settings.max_memory_mb, 16384)}
                min={0}
                max={16384}
                step={256}
                onChange={(v) => patchDebounced({ max_memory_mb: v })}
                unit=" MB"
                zeroLabel="sin límite"
                helpText="Hint de memoria para el motor. Valores más bajos hacen que el motor elija presets de compresión más conservadores. 0 = sin límite."
              />
            </CardContent>
          </Card>
        ) : null}

        {activeSection === "output" ? (
          <Card>
            <CardHeader
              title="Salida"
              subtitle="Dónde se guardan los archivos optimizados"
            />
            <CardContent className="flex flex-col gap-4">
              <SelectField<OutputMode>
                label="Modo de salida"
                helpText="Misma carpeta, subcarpeta BoxFlux/, o carpeta personalizada."
                value={settings.output_mode}
                options={[
                  { value: "SameFolder", label: "Misma carpeta que el original" },
                  { value: "OutputSubfolder", label: "Subcarpeta BoxFlux/" },
                  { value: "CustomFolder", label: "Carpeta personalizada" },
                ]}
                onChange={(v) => patch({ output_mode: v })}
              />
              {settings.output_mode === "CustomFolder" ? (
                <LabeledInput
                  label="Carpeta personalizada"
                  value={settings.custom_output_folder}
                  onEdited={(v) => patch({ custom_output_folder: v })}
                  placeholder="/ruta/a/carpeta"
                  helpText="Ruta absoluta a la carpeta donde se guardarán los archivos optimizados."
                />
              ) : null}
              <LabeledInput
                label="Plantilla de nombre"
                value={settings.filename_template}
                onEdited={(v) => patch({ filename_template: v })}
                helpText="Variables disponibles: {name} (nombre sin extensión), {ext} (extensión original), {format} (formato de salida)."
              />
              <ToggleRow
                label="Preservar estructura de carpetas"
                description="Mantén la jerarquía de subcarpetas al optimizar una carpeta completa."
                checked={settings.preserve_folder_structure}
                onChange={(v) => patch({ preserve_folder_structure: v })}
              />
            </CardContent>
          </Card>
        ) : null}
        </div>
      </div>
    </div>
  );
};

const ToggleRow: React.FC<{
  label: string;
  description?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}> = ({ label, description, checked, onChange }) => (
  <div className="flex items-start justify-between gap-4 py-1">
    <div className="flex flex-col gap-0.5">
      <span className="text-sm text-foreground">{label}</span>
      {description ? (
        <p className="text-xs text-muted-foreground">{description}</p>
      ) : null}
    </div>
    <Toggle checked={checked} onCheckedChange={onChange} aria-label={label} />
  </div>
);

SettingsPage.displayName = "SettingsPage";
