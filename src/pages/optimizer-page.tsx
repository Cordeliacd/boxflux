import * as React from "react";
import { useNavigate } from "react-router-dom";
import {
  Play,
  Plus,
  Shield,
  Zap,
  CheckCircle2,
  Brain,
  Palette,
  FileImage,
  Sparkles,
  FolderOpen,
  Image as ImageIcon,
  ArrowRight,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { DropZone } from "@/components/ui/drop-zone";
import { useErrorBanner } from "@/components/ui/error-banner";
import { StatusBadge } from "@/components/ui/status-badge";
import { Toggle } from "@/components/ui/toggle";
import { useToast } from "@/components/ui/toast";
import { useQueue } from "@/state/queue-context";
import { basename } from "@/lib/format";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import * as tauri from "@/services/tauri";
import type { Format } from "@/types";
import { cn } from "@/lib/utils";

type Mode = "Lossless" | "Compress";
type OutputFormat = "auto" | "png" | "jpeg" | "webp" | "avif";
type PickerKind = "auto" | "files" | "folder";

// Mismos 2 modos que expone el motor (ver auto-optimizer-page).
const MODE_TILES: Array<{
  id: Mode;
  label: string;
  description: string;
  icon: React.ElementType;
  detail: string;
}> = [
  {
    id: "Lossless",
    label: "Balanceado",
    description: "Máxima compresión sin perder calidad",
    icon: Shield,
    detail:
      "Solo codecs lossless (PNG oxipng+zopfli, WebP lossless, transcode JPEG). Calidad idéntica al original.",
  },
  {
    id: "Compress",
    label: "Comprimir al máximo",
    description: "El archivo más pequeño posible",
    icon: Zap,
    detail:
      "Compresión extrema con la menor pérdida perceptual (gate butteraugli medido real, búsqueda iterativa JPEG/WebP/AVIF).",
  },
];

const FORMAT_TILES: Array<{
  id: OutputFormat;
  label: string;
  description: string;
}> = [
  {
    id: "auto",
    label: "Auto",
    description:
      "El motor evalúa todos los formatos y elige el mejor según la imagen.",
  },
  {
    id: "png",
    label: "PNG",
    description:
      "Sin pérdidas. Ideal para gráficos, capturas y logos con transparencia.",
  },
  {
    id: "jpeg",
    label: "JPEG",
    description:
      "Fotografías sin transparencia. Mejor compatibilidad universal.",
  },
  {
    id: "webp",
    label: "WebP",
    description:
      "Formato moderno. Soporta transparencia y animación. Más pequeño que JPEG/PNG.",
  },
  {
    id: "avif",
    label: "AVIF",
    description:
      "Mayor compresión que WebP. Ideal para web moderna. Sin soporte para entrada AVIF en esta versión.",
  },
];

const FORMAT_TO_API: Record<Exclude<OutputFormat, "auto">, Format> = {
  png: "PNG",
  jpeg: "JPEG",
  webp: "WebP",
  avif: "AVIF",
};

const IMAGE_EXTENSIONS = [
  "png",
  "jpg",
  "jpeg",
  "webp",
  "avif",
  "gif",
  "bmp",
  "tif",
  "tiff",
];

function isImagePath(path: string): boolean {
  const lower = path.toLowerCase();
  return IMAGE_EXTENSIONS.some((ext) => lower.endsWith(`.${ext}`));
}

export const OptimizerPage: React.FC = () => {
  const navigate = useNavigate();
  const { enqueueOptimize, addDroppedPaths } = useQueue();
  const { push } = useToast();
  const { pushError, dismissError } = useErrorBanner();

  const [selectedFiles, setSelectedFiles] = React.useState<string[]>([]);
  const [mode, setMode] = React.useState<Mode>("Lossless");
  const [outputFormat, setOutputFormat] = React.useState<OutputFormat>("auto");
  const [preserveColorProfile, setPreserveColorProfile] =
    React.useState(true);
  const [stripMetadata, setStripMetadata] = React.useState(false);
  const [submitState, setSubmitState] = React.useState<
    "idle" | "processing" | "done"
  >("idle");
  const [picker, setPicker] = React.useState<PickerKind>("auto");

  const handleBrowseFiles = async () => {
    try {
      const result = await openDialog({
        multiple: true,
        filters: [
          {
            name: "Imágenes",
            extensions: IMAGE_EXTENSIONS,
          },
        ],
      });
      if (result) {
        const paths = Array.isArray(result) ? result : [result];
        const valid = paths.filter(isImagePath);
        if (valid.length !== paths.length) {
          push({
            variant: "warning",
            title: "Algunos archivos fueron ignorados",
            description: `${paths.length - valid.length} archivo(s) no son imágenes soportadas.`,
          });
        }
        if (valid.length > 0) {
          setSelectedFiles((prev) => Array.from(new Set([...prev, ...valid])));
          setSubmitState("idle");
        }
      }
    } catch (e) {
      console.warn("Browse cancelado:", e);
    }
  };

  const handleBrowseFolder = async () => {
    try {
      const result = await openDialog({ directory: true, multiple: false });
      if (typeof result === "string" && result.length > 0) {
        try {
          const files = await tauri.discoverSupportedFiles(result, true);
          if (files.length === 0) {
            push({
              variant: "warning",
              title: "Carpeta sin imágenes",
              description:
                "La carpeta seleccionada no contiene imágenes soportadas (PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF).",
              duration: 6000,
            });
            return;
          }
          setSelectedFiles((prev) =>
            Array.from(new Set([...prev, ...files])),
          );
          setSubmitState("idle");
          push({
            variant: "success",
            title: `${files.length} imágenes encontradas`,
            description: `Descubrimiento recursivo en ${basename(result)}.`,
          });
        } catch (err) {
          push({
            variant: "error",
            title: "No se pudo explorar la carpeta",
            description:
              "Esta función requiere la app de escritorio (pnpm tauri:dev).",
          });
          console.warn("discoverSupportedFiles falló:", err);
        }
      }
    } catch (err) {
      console.warn("Folder browse cancelado:", err);
    }
  };

  const handlePickerAction = (id: string) => {
    if (id === "files" || id === "auto") {
      void handleBrowseFiles();
    } else if (id === "folder") {
      void handleBrowseFolder();
    }
  };

  const handleOptimize = async () => {
    if (selectedFiles.length === 0) return;
    setSubmitState("processing");
    let enqueued = 0;
    let failed = 0;
    const firstError: string[] = [];
    for (const path of selectedFiles) {
      try {
        await enqueueOptimize(path, mode, {
          force_format:
            outputFormat === "auto" ? null : FORMAT_TO_API[outputFormat],
          preserve_color_profile: preserveColorProfile,
          strip_all_metadata: stripMetadata,
        });
        enqueued += 1;
      } catch (e) {
        console.error("Error encolando", path, e);
        failed += 1;
        if (firstError.length === 0) {
          firstError.push(e instanceof Error ? e.message : String(e));
        }
      }
    }
    if (enqueued > 0 && failed === 0) {
      setSubmitState("done");
      const fmtLabel =
        outputFormat === "auto"
          ? "Auto"
          : FORMAT_TILES.find((t) => t.id === outputFormat)?.label ??
            outputFormat;
      push({
        variant: "success",
        title: `${enqueued} archivo${enqueued > 1 ? "s" : ""} en cola`,
        description: `El motor inteligente procesará cada imagen (modo ${mode === "Lossless" ? "Balanceado" : "Comprimir al máximo"}, formato ${fmtLabel}).`,
      });
      dismissError("enqueue-failed");
    } else if (enqueued > 0 && failed > 0) {
      setSubmitState("done");
      push({
        variant: "info",
        title: `${enqueued} encolados, ${failed} fallidos`,
        description: firstError[0] ?? "Algunos archivos no se pudieron encolar.",
      });
    } else {
      setSubmitState("idle");
      const reason = firstError[0] ?? "";
      push({
        variant: "error",
        title: "No se pudo encolar",
        description:
          reason ||
          "Verifica que la app de escritorio Tauri esté corriendo (pnpm tauri:dev).",
        duration: 6000,
      });
      pushError({
        id: "enqueue-failed",
        title: `No se pudo encolar ningún archivo${failed > 1 ? ` (${failed} intentos)` : ""}`,
        description:
          reason ||
          "Verifica que la app de escritorio Tauri esté corriendo (pnpm tauri:dev). El backend Rust es necesario para procesar imágenes.",
        severity: "error",
      });
    }
  };

  const handleRemoveFile = (path: string) => {
    setSelectedFiles((prev) => prev.filter((p) => p !== path));
  };

  const handleClear = () => {
    setSelectedFiles([]);
    setSubmitState("idle");
  };

  const handleGoToQueue = () => {
    navigate("/queue");
  };

  const pickerMenuItems = [
    {
      id: "auto" as const,
      label: "Auto",
      hint: "Selector de imágenes (recomendado)",
      icon: <Sparkles className="h-4 w-4" aria-hidden="true" />,
    },
    {
      id: "files" as const,
      label: "Imágenes",
      hint: "PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF",
      icon: <ImageIcon className="h-4 w-4" aria-hidden="true" />,
    },
    {
      id: "folder" as const,
      label: "Carpeta",
      hint: "Descubrimiento recursivo de imágenes",
      icon: <FolderOpen className="h-4 w-4" aria-hidden="true" />,
    },
  ];

  const isProcessing = submitState === "processing";

  return (
    <div className="flex flex-col gap-5">
      {/* Drop zone o lista de archivos seleccionados */}
      {selectedFiles.length === 0 ? (
        <DropZone
          onPathsDropped={(paths) => {
            const files = paths.filter((p) => p.includes("."));
            const folders = paths.filter((p) => !p.includes("."));
            if (files.length > 0) {
              setSelectedFiles((prev) =>
                Array.from(new Set([...prev, ...files])),
              );
            }
            if (folders.length > 0) {
              void addDroppedPaths(folders).then((n) => {
                if (n > 0) {
                  push({
                    variant: "success",
                    title: `${n} imagen(es) descubiertas en ${folders.length} carpeta(s)`,
                    description: "Enviadas a la cola para procesamiento.",
                  });
                }
              });
            }
          }}
          onBrowse={handleBrowseFiles}
          title="Arrastra imágenes para optimizar"
          subtitle="O usa el botón para explorar archivos y carpetas"
          menuItems={pickerMenuItems}
          menuSelectedId={picker}
          onMenuSelect={(id) => setPicker(id as PickerKind)}
          onMenuTrigger={handlePickerAction}
          menuTriggerLabel="Añadir"
        />
      ) : (
        <Card>
          <CardHeader
            title="Archivos seleccionados"
            subtitle={`${selectedFiles.length} archivo${selectedFiles.length > 1 ? "s" : ""} encolado${selectedFiles.length > 1 ? "s" : ""} para optimización`}
            action={
              submitState !== "processing" ? (
                <Button variant="ghost" size="sm" onClick={handleClear}>
                  Limpiar
                </Button>
              ) : null
            }
          />
          <CardContent flush>
            <div className="max-h-[22rem] overflow-y-auto">
              <table className="w-full table-fixed">
                <thead className="bg-surface-inset sticky top-0">
                  <tr>
                    <th className="w-[40%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                      Archivo
                    </th>
                    <th className="w-[15%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                      Formato
                    </th>
                    <th className="w-[15%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                      Tamaño
                    </th>
                    <th className="w-[20%] px-3 py-2 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                      Estado
                    </th>
                    <th className="w-[10%] px-3 py-2 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                      <span className="sr-only">Acciones</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {selectedFiles.map((path) => {
                    const name = basename(path);
                    const ext =
                      name.split(".").pop()?.toUpperCase() ?? "?";
                    return (
                      <tr
                        key={path}
                        className="border-t border-border transition-colors hover:bg-hover"
                      >
                        <td className="px-3 py-2 text-sm">
                          <div className="flex items-center gap-2">
                            <FileImage
                              className="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                              aria-hidden="true"
                            />
                            <span className="truncate" title={path}>
                              {name}
                            </span>
                          </div>
                        </td>
                        <td className="px-3 py-2 font-mono text-xs text-muted-foreground">
                          {ext}
                        </td>
                        <td className="px-3 py-2 text-right font-mono text-xs text-muted-foreground">
                          —
                        </td>
                        <td className="px-3 py-2">
                          <StatusBadge
                            status={
                              submitState === "done"
                                ? "Queued"
                                : submitState === "processing"
                                  ? "Processing"
                                  : "Queued"
                            }
                            label={
                              submitState === "done"
                                ? "En cola"
                                : submitState === "processing"
                                  ? "Encolando…"
                                  : "Listo"
                            }
                          />
                        </td>
                        <td className="px-3 py-2 text-right">
                          {submitState === "idle" ? (
                            <button
                              type="button"
                              onClick={() => handleRemoveFile(path)}
                              className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-hover hover:text-error focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background"
                              aria-label={`Eliminar ${name}`}
                            >
                              <X
                                className="h-3.5 w-3.5"
                                aria-hidden="true"
                              />
                            </button>
                          ) : null}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Settings card */}
      <Card>
        <CardHeader
          title="Modo de optimización"
          subtitle="El motor inteligente analizará cada imagen y decidirá la mejor optimización"
        />
        <CardContent className="flex flex-col gap-5">
          {/* Info banner */}
          <div className="flex items-start gap-3 rounded-md border border-primary/15 bg-primary-subtle/60 px-3 py-2.5">
            <Brain
              className="mt-0.5 h-4 w-4 shrink-0 text-primary"
              aria-hidden="true"
            />
            <div className="text-xs leading-relaxed text-foreground">
              <span className="font-medium">Motor inteligente activo.</span>{" "}
              El motor analiza cada imagen (complejidad, categoría, alpha) y
              evalúa múltiples formatos y calidades. No aplica un perfil fijo:
              decide qué optimización es mejor según el análisis.
            </div>
          </div>

          {/* Mode tiles */}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {MODE_TILES.map((tile) => {
              const active = tile.id === mode;
              const Icon = tile.icon;
              return (
                <button
                  key={tile.id}
                  type="button"
                  disabled={isProcessing}
                  onClick={() => setMode(tile.id)}
                  className={cn(
                    "flex flex-col gap-2 rounded-md border p-4 text-left transition-all duration-fast ease-out-soft",
                    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                    "disabled:cursor-not-allowed disabled:opacity-50",
                    active
                      ? "border-primary bg-primary-subtle"
                      : "border-border bg-surface-inset hover:bg-hover hover:border-border-strong",
                  )}
                >
                  <div className="flex items-center gap-2.5">
                    <Icon
                      className={cn(
                        "h-5 w-5 transition-colors",
                        active ? "text-primary" : "text-muted-foreground",
                      )}
                      aria-hidden="true"
                    />
                    <span
                      className={cn(
                        "text-base font-semibold transition-colors",
                        active ? "text-primary" : "text-foreground",
                      )}
                    >
                      {tile.label}
                    </span>
                  </div>
                  <p className="text-sm text-muted-foreground">
                    {tile.description}
                  </p>
                  <p className="text-xs leading-relaxed text-muted-foreground/80">
                    {tile.detail}
                  </p>
                </button>
              );
            })}
          </div>

          {/* Output format */}
          <div>
            <div className="flex items-center gap-2">
              <FileImage
                className="h-4 w-4 text-muted-foreground"
                aria-hidden="true"
              />
              <span className="caption">Formato de salida</span>
            </div>
            <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-5">
              {FORMAT_TILES.map((tile) => {
                const active = tile.id === outputFormat;
                return (
                  <button
                    key={tile.id}
                    type="button"
                    disabled={isProcessing}
                    onClick={() => setOutputFormat(tile.id)}
                    title={tile.description}
                    className={cn(
                      "flex flex-col items-center justify-center gap-1 rounded-md border px-2 py-2.5 text-center transition-all duration-fast ease-out-soft",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                      "disabled:cursor-not-allowed disabled:opacity-50",
                      active
                        ? "border-primary bg-primary-subtle text-primary"
                        : "border-border bg-surface-inset text-muted-foreground hover:bg-hover hover:border-border-strong",
                    )}
                  >
                    <span
                      className={cn(
                        "text-xs font-semibold transition-colors",
                        active ? "text-primary" : "text-foreground",
                      )}
                    >
                      {tile.label}
                    </span>
                  </button>
                );
              })}
            </div>
            <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">
              {outputFormat === "auto"
                ? "Auto: el motor decide el formato óptimo evaluando todos los disponibles y eligiendo el de menor peso en bytes."
                : FORMAT_TILES.find((t) => t.id === outputFormat)
                    ?.description}
            </p>
          </div>

          {/* Metadata toggles */}
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3 pt-1">
            <label className="flex items-center gap-2 text-sm text-foreground">
              <Toggle
                checked={preserveColorProfile}
                onCheckedChange={setPreserveColorProfile}
                aria-label="Preservar perfil de color"
              />
              <Palette
                className="h-3.5 w-3.5 text-muted-foreground"
                aria-hidden="true"
              />
              <span>Preservar perfil de color</span>
            </label>
            <label className="flex items-center gap-2 text-sm text-foreground">
              <Toggle
                checked={stripMetadata}
                onCheckedChange={setStripMetadata}
                aria-label="Eliminar metadata"
              />
              <span>Eliminar metadata EXIF</span>
            </label>
          </div>
          {preserveColorProfile ? (
            <div className="rounded-md border border-primary/15 bg-primary-subtle/40 px-3 py-2 text-xs leading-relaxed text-foreground">
              <span className="font-medium">Perfil de color preservado.</span>{" "}
              El motor mantendrá los perfiles ICC del original (Display P3,
              sRGB, Adobe RGB) para evitar el oscurecimiento de tonos negros y
              sombras. JPEG usará chroma 4:4:4 (sin subsampling) para mayor
              fidelidad.
            </div>
          ) : null}

          {/* Success state */}
          {submitState === "done" ? (
            <div
              className="flex items-start gap-2.5 rounded-md border border-success/30 bg-success-subtle px-3 py-2.5 text-sm text-success-foreground fade-in"
              role="status"
            >
              <CheckCircle2
                className="mt-0.5 h-4 w-4 shrink-0"
                aria-hidden="true"
              />
              <div className="flex-1">
                <span className="font-medium">
                  {selectedFiles.length} archivo
                  {selectedFiles.length > 1 ? "s" : ""} encolado
                  {selectedFiles.length > 1 ? "s" : ""}.
                </span>{" "}
                El motor inteligente los procesará y elegirá la mejor
                optimización para cada uno. Se guardarán en{" "}
                <code className="font-mono">~/BoxFlux/optimized/</code>.
              </div>
            </div>
          ) : null}

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 border-t border-border pt-4">
            {submitState === "done" ? (
              <Button
                variant="primary"
                size="md"
                onClick={handleGoToQueue}
              >
                Ver en Cola
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Button>
            ) : null}
            {submitState !== "done" ? (
              <>
                <Button
                  variant="ghost"
                  size="md"
                  onClick={handleBrowseFiles}
                  disabled={isProcessing}
                >
                  <Plus className="h-4 w-4" aria-hidden="true" />
                  Añadir archivos
                </Button>
                <Button
                  variant="primary"
                  size="md"
                  onClick={handleOptimize}
                  loading={isProcessing}
                  disabled={selectedFiles.length === 0}
                >
                  {!isProcessing ? (
                    <Play className="h-4 w-4" aria-hidden="true" />
                  ) : null}
                  {isProcessing
                    ? "Encolando…"
                    : `Optimizar ${selectedFiles.length > 0 ? `(${selectedFiles.length})` : ""}`}
                </Button>
              </>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  );
};

OptimizerPage.displayName = "OptimizerPage";
