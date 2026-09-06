import * as React from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Play,
  Square,
  FolderOpen,
  FileOutput,
  RefreshCw,
  Shield,
  Zap,
  Sparkles,
  Brain,
  Palette,
  FileImage,
  AlertCircle,
  CheckCircle2,
  TrendingDown,
  Image as ImageIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { DropdownMenu, type DropdownMenuItem } from "@/components/ui/dropdown-menu";
import { ProgressBar } from "@/components/ui/progress-bar";
import { StatusBadge } from "@/components/ui/status-badge";
import { Toggle } from "@/components/ui/toggle";
import { useToast } from "@/components/ui/toast";
import { useAutoOptimize } from "@/state/auto-optimize-context";
import { useQueue } from "@/state/queue-context";
import { useNavigate } from "react-router-dom";
import type { Format } from "@/types";
import {
  formatBytes,
  formatDuration,
  formatPercent,
  formatQuality,
} from "@/lib/format";
import { basename } from "@/lib/format";
import * as tauri from "@/services/tauri";
import { cn } from "@/lib/utils";

type Mode = "Lossless" | "Compress";
type OutputFormat = "auto" | "png" | "jpeg" | "webp" | "avif";
type InputPickerKind = "auto" | "file" | "folder";

// El motor solo tiene 2 modos: "Balanceado" = goal "Lossless" (solo
// codecs sin pérdida) y "Comprimir al máximo" = MaximumCompression con
// gate perceptual butteraugli.
const MODE_TILES: Array<{
  id: Mode;
  label: string;
  description: string;
  icon: React.ElementType;
}> = [
  {
    id: "Lossless",
    label: "Balanceado",
    description: "Máxima compresión sin perder nada de calidad",
    icon: Shield,
  },
  {
    id: "Compress",
    label: "Comprimir al máximo",
    description: "El archivo más pequeño posible, cuidando la calidad",
    icon: Zap,
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
    description: "El motor decide el formato óptimo evaluando todos los disponibles.",
  },
  {
    id: "png",
    label: "PNG",
    description: "Sin pérdidas. Ideal para gráficos y logos con transparencia.",
  },
  {
    id: "jpeg",
    label: "JPEG",
    description: "Fotografías sin transparencia. Mejor compatibilidad universal.",
  },
  {
    id: "webp",
    label: "WebP",
    description: "Formato moderno. Soporta transparencia y animación.",
  },
  {
    id: "avif",
    label: "AVIF",
    description: "Mayor compresión que WebP. Ideal para web moderna.",
  },
];

const PIPELINE_STAGES = [
  "Analizar archivo de entrada",
  "Determinar estrategia",
  "Generar candidatos",
  "Procesar cada candidato",
  "Evaluar calidad (PSNR, SSIM)",
  "Puntuar y ordenar",
  "Seleccionar el mejor candidato",
  "Publicar atómicamente",
];

const IMAGE_EXTENSIONS = [
  "png", "jpg", "jpeg", "webp", "avif", "gif", "bmp", "tif", "tiff",
];

function isImagePath(path: string): boolean {
  const lower = path.toLowerCase();
  return IMAGE_EXTENSIONS.some((ext) => lower.endsWith(`.${ext}`));
}

export const AutoOptimizerPage: React.FC = () => {
  const {
    running,
    decision,
    error,
    cancelledReason,
    start,
    cancel,
    reset,
  } = useAutoOptimize();
  const { enqueueOptimize } = useQueue();
  const navigate = useNavigate();
  const { push } = useToast();

  const [inputPath, setInputPath] = React.useState("");
  const [inputFiles, setInputFiles] = React.useState<string[]>([]);
  const [outputDir, setOutputDir] = React.useState("");
  const [mode, setMode] = React.useState<Mode>("Lossless");
  const [outputFormat, setOutputFormat] = React.useState<OutputFormat>("auto");
  const [forceLossless, setForceLossless] = React.useState(false);
  const [stripMetadata, setStripMetadata] = React.useState(false);
  const [preserveColorProfile, setPreserveColorProfile] =
    React.useState(true);
  const [inputPicker, setInputPicker] = React.useState<InputPickerKind>("auto");

  const usingInputDir = !outputDir && inputPath.length > 0;
  const showConfig = !running && decision === null && !error && !cancelledReason;

  const handleBrowseInputFile = async () => {
    try {
      const result = await openDialog({
        multiple: false,
        filters: [
          { name: "Imágenes", extensions: IMAGE_EXTENSIONS },
        ],
      });
      if (typeof result === "string" && result.length > 0) {
        if (!isImagePath(result)) {
          push({
            variant: "error",
            title: "Archivo no soportado",
            description: "Solo se aceptan imágenes (PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF).",
            duration: 6000,
          });
          return;
        }
        setInputPath(result);
        setInputFiles([result]);
      }
    } catch (e) {
      console.warn("Input browse cancelado:", e);
    }
  };

  const handleBrowseInputFolder = async () => {
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
                "La carpeta seleccionada no contiene imágenes soportadas.",
              duration: 6000,
            });
            return;
          }
          setInputPath(result);
          setInputFiles(files);
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

  const handleInputPickerAction = (id: string) => {
    if (id === "file" || id === "auto") {
      void handleBrowseInputFile();
    } else if (id === "folder") {
      void handleBrowseInputFolder();
    }
  };

  const handleBrowseOutputDir = async () => {
    try {
      const result = await openDialog({ directory: true, multiple: false });
      if (typeof result !== "string" || result.length === 0) return;
      setOutputDir(result);
    } catch (e) {
      console.warn("Output dir browse cancelado:", e);
    }
  };

  const handleStart = async () => {
    if (!inputPath) {
      push({
        variant: "error",
        title: "Falta archivo de entrada",
        description: "Selecciona una imagen o carpeta para optimizar.",
      });
      return;
    }
    if (inputFiles.length > 1) {
      let enqueued = 0;
      let failed = 0;
      const fmt: Format | null =
        outputFormat === "auto" ? null : (outputFormat.toUpperCase() as Format);
      for (const file of inputFiles) {
        try {
          await enqueueOptimize(file, mode, {
            force_format: fmt,
            force_lossless: forceLossless,
            strip_all_metadata: stripMetadata,
            preserve_color_profile: preserveColorProfile,
          });
          enqueued += 1;
        } catch (e) {
          console.error("Error encolando", file, e);
          failed += 1;
        }
      }
      if (enqueued > 0) {
        push({
          variant: "success",
          title: `${enqueued} imagen(es) en la cola`,
          description:
            "El motor inteligente procesará cada archivo. Sigue el progreso en la página Cola.",
          duration: 6000,
        });
        navigate("/queue");
      } else {
        push({
          variant: "error",
          title: "No se pudo encolar ninguna imagen",
          description:
            "Verifica que la app de escritorio esté corriendo (pnpm tauri:dev).",
        });
      }
      if (failed > 0) {
        push({
          variant: "warning",
          title: `${failed} archivo(s) no se pudieron encolar`,
          description: "Revisa la consola para más detalles.",
        });
      }
      return;
    }
    await start({
      input_path: inputPath,
      output_dir: outputDir || "",
      goal: mode,
      force_lossless: forceLossless,
      strip_all_metadata: stripMetadata,
      force_format: outputFormat === "auto" ? null : outputFormat.toUpperCase(),
      preserve_color_profile: preserveColorProfile,
    });
  };

  const inputMenuItems: DropdownMenuItem[] = [
    {
      id: "auto",
      label: "Auto",
      hint: "Selector de imagen (recomendado)",
      icon: <Sparkles className="h-4 w-4" aria-hidden="true" />,
    },
    {
      id: "file",
      label: "Imagen",
      hint: "PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF",
      icon: <ImageIcon className="h-4 w-4" aria-hidden="true" />,
    },
    {
      id: "folder",
      label: "Carpeta",
      hint: "Descubrimiento recursivo de imágenes",
      icon: <FolderOpen className="h-4 w-4" aria-hidden="true" />,
    },
  ];

  return (
    <div className="flex flex-col gap-5">
      {showConfig ? (
        <Card>
          <CardHeader
            title="Configuración"
            subtitle="El motor inteligente analizará la imagen y decidirá la mejor optimización"
          />
          <CardContent className="flex flex-col gap-5">
            {/* Input picker */}
            <div className="flex flex-col gap-2">
              <label className="caption">Archivo o carpeta de entrada</label>
              <div className="flex items-center gap-2">
                <span
                  className="flex-1 truncate rounded-md border border-border bg-surface-inset px-3 py-2 text-sm"
                  title={inputPath || undefined}
                >
                  {inputPath ? (
                    <span className="flex items-center gap-2">
                      <span className="truncate text-foreground">
                        {basename(inputPath)}
                      </span>
                      {inputFiles.length > 1 ? (
                        <span className="shrink-0 rounded-xs bg-primary-subtle px-1.5 py-0.5 text-2xs text-primary">
                          {inputFiles.length} archivos
                        </span>
                      ) : null}
                    </span>
                  ) : (
                    <span className="text-muted-foreground">
                      Ningún archivo o carpeta seleccionado
                    </span>
                  )}
                </span>
                <DropdownMenu
                  triggerLabel="Examinar"
                  triggerIcon={
                    <FolderOpen className="h-4 w-4" aria-hidden="true" />
                  }
                  variant="secondary"
                  size="md"
                  items={inputMenuItems}
                  selectedId={inputPicker}
                  onSelect={(id) => {
                    setInputPicker(id as InputPickerKind);
                    handleInputPickerAction(id);
                  }}
                />
              </div>
              {inputFiles.length > 1 ? (
                <p className="text-xs text-muted-foreground">
                  Se procesarán {inputFiles.length} imágenes en orden.
                </p>
              ) : null}
            </div>

            {/* Output dir */}
            <div className="flex flex-col gap-2">
              <label className="caption">Directorio de salida</label>
              <div className="flex items-center gap-2">
                <span
                  className="flex-1 truncate rounded-md border border-border bg-surface-inset px-3 py-2 text-sm"
                  title={outputDir || undefined}
                >
                  {outputDir ? (
                    <span className="truncate text-foreground">{outputDir}</span>
                  ) : inputPath ? (
                    <span className="flex items-center gap-1.5 text-warning-foreground">
                      <AlertCircle className="h-3 w-3" aria-hidden="true" />
                      Se usará el directorio del archivo de entrada
                    </span>
                  ) : (
                    <span className="text-muted-foreground">
                      Se requiere un directorio de salida
                    </span>
                  )}
                </span>
                <Button
                  variant="ghost"
                  size="md"
                  onClick={handleBrowseOutputDir}
                >
                  <FolderOpen className="h-4 w-4" aria-hidden="true" />
                  Elegir
                </Button>
              </div>
              {usingInputDir ? (
                <p className="text-xs text-muted-foreground">
                  Si no seleccionas un directorio de salida, el motor guardará
                  cada archivo optimizado junto a su original.
                </p>
              ) : null}
            </div>

            {/* Mode */}
            <div>
              <span className="caption">Objetivo de optimización</span>
              <div className="mt-2 grid grid-cols-1 gap-2 sm:grid-cols-3">
                {MODE_TILES.map((tile) => {
                  const active = tile.id === mode;
                  const Icon = tile.icon;
                  return (
                    <button
                      key={tile.id}
                      type="button"
                      onClick={() => setMode(tile.id)}
                      className={cn(
                        "flex flex-col gap-1 rounded-md border px-3 py-2.5 text-left transition-all duration-fast ease-out-soft",
                        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                        active
                          ? "border-primary bg-primary-subtle"
                          : "border-border bg-surface-inset hover:bg-hover hover:border-border-strong",
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <Icon
                          className={cn(
                            "h-4 w-4 transition-colors",
                            active ? "text-primary" : "text-muted-foreground",
                          )}
                          aria-hidden="true"
                        />
                        <span
                          className={cn(
                            "text-sm font-medium transition-colors",
                            active ? "text-primary" : "text-foreground",
                          )}
                        >
                          {tile.label}
                        </span>
                      </div>
                      <span className="text-xs text-muted-foreground">
                        {tile.description}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Format */}
            <div>
              <div className="flex items-center gap-2">
                <FileImage className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                <span className="caption">Formato de salida</span>
              </div>
              <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-5">
                {FORMAT_TILES.map((tile) => {
                  const active = tile.id === outputFormat;
                  return (
                    <button
                      key={tile.id}
                      type="button"
                      onClick={() => setOutputFormat(tile.id)}
                      title={tile.description}
                      className={cn(
                        "flex flex-col items-center justify-center gap-1 rounded-md border px-2 py-2.5 text-center transition-all duration-fast ease-out-soft",
                        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background",
                        active
                          ? "border-primary bg-primary-subtle"
                          : "border-border bg-surface-inset hover:bg-hover hover:border-border-strong",
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
                  : FORMAT_TILES.find((t) => t.id === outputFormat)?.description}
              </p>
            </div>

            {/* Toggles */}
            <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
              <label className="flex items-center gap-2 text-sm text-foreground">
                <Toggle
                  checked={forceLossless}
                  onCheckedChange={setForceLossless}
                  aria-label="Forzar sin pérdida"
                />
                <span>Forzar sin pérdida</span>
              </label>
              <label className="flex items-center gap-2 text-sm text-foreground">
                <Toggle
                  checked={stripMetadata}
                  onCheckedChange={setStripMetadata}
                  aria-label="Eliminar metadata"
                />
                <span>Eliminar metadata</span>
              </label>
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
            </div>

            {preserveColorProfile ? (
              <div className="rounded-md border border-primary/15 bg-primary-subtle/40 px-3 py-2 text-xs leading-relaxed text-foreground">
                <span className="font-medium">Perfil de color preservado.</span>{" "}
                El motor mantendrá los perfiles ICC del original (Display P3,
                sRGB, Adobe RGB) para evitar el oscurecimiento de tonos negros
                y sombras. JPEG usará chroma 4:4:4 (sin subsampling) para mayor
                fidelidad.
              </div>
            ) : null}

            {/* Info banner */}
            <div className="flex items-start gap-3 rounded-md border border-primary/15 bg-primary-subtle/60 px-3 py-2.5">
              <Brain
                className="mt-0.5 h-4 w-4 shrink-0 text-primary"
                aria-hidden="true"
              />
              <div className="text-xs leading-relaxed text-foreground">
                <span className="font-medium">Motor inteligente.</span> El motor
                analizará la imagen y evaluará múltiples formatos (AVIF, WebP,
                JPEG) y calidades para elegir la mejor opción según el modo
                seleccionado.
              </div>
            </div>

            {/* Action */}
            <div className="flex items-center gap-3 border-t border-border pt-4">
              <Button
                variant="primary"
                size="lg"
                onClick={handleStart}
                disabled={!inputPath}
              >
                <Play className="h-4 w-4" aria-hidden="true" />
                Iniciar optimización
              </Button>
              {!inputPath ? (
                <p className="text-xs text-muted-foreground">
                  Selecciona un archivo o carpeta de entrada para comenzar.
                </p>
              ) : null}
            </div>
          </CardContent>
        </Card>
      ) : null}

      {running ? (
        <Card>
          <CardHeader
            title="Optimización en curso"
            subtitle="El pipeline inteligente está procesando el archivo"
            action={
              <Button variant="danger" size="sm" onClick={cancel}>
                <Square className="h-3.5 w-3.5" aria-hidden="true" />
                Cancelar
              </Button>
            }
          />
          <CardContent className="flex flex-col gap-5">
            <ProgressBar value={null} />
            <div>
              <span className="caption">Etapas del pipeline</span>
              <ul className="mt-2.5 flex flex-col gap-1.5">
                {PIPELINE_STAGES.map((stage, idx) => (
                  <li
                    key={stage}
                    className="flex items-center gap-2.5 text-sm text-muted-foreground"
                  >
                    <span
                      className="flex h-5 w-5 items-center justify-center rounded-full border border-border text-2xs font-mono text-muted-foreground"
                      aria-hidden="true"
                    >
                      {idx + 1}
                    </span>
                    {stage}
                  </li>
                ))}
              </ul>
            </div>
          </CardContent>
        </Card>
      ) : null}

      {!running && (decision || error || cancelledReason) ? (
        <Card>
          <CardHeader
            title={
              error
                ? "Optimización fallida"
                : cancelledReason
                  ? "Optimización cancelada"
                  : decision?.kept_original
                    ? "Se mantuvo el original"
                    : "Optimización completa"
            }
            subtitle={
              error
                ? undefined
                : cancelledReason
                  ? cancelledReason
                  : decision?.selected
                    ? `Seleccionado: ${decision.selected.candidate.label}`
                    : "Ningún candidato superó al original"
            }
          />
          <CardContent className="flex flex-col gap-5">
            {error ? (
              <div className="flex items-start gap-2.5 rounded-md border border-error/30 bg-error-subtle p-3 text-sm text-error-foreground fade-in">
                <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
                <span className="font-mono leading-relaxed">{error}</span>
              </div>
            ) : null}

            {decision ? (
              <>
                {/* Hero stat row */}
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                  <StatBox
                    label="Original"
                    value={formatBytes(decision.original_size)}
                  />
                  <StatBox
                    label="Optimizado"
                    value={formatBytes(decision.final_size)}
                  />
                  <StatBox
                    label="Ahorrado"
                    value={`${formatBytes(
                      decision.original_size - decision.final_size,
                    )} (${formatPercent(
                      decision.original_size > 0
                        ? (100 *
                            (decision.original_size - decision.final_size)) /
                            decision.original_size
                        : 0,
                    )})`}
                    valueClassName="text-primary"
                    icon={TrendingDown}
                  />
                </div>

                {/* Reason */}
                <div className="rounded-md border border-border bg-surface-inset px-3 py-2.5">
                  <span className="caption">Razón</span>
                  <p className="mt-1.5 text-sm leading-relaxed text-foreground">
                    {decision.explanation}
                  </p>
                </div>

                {/* Candidates */}
                <div>
                  <span className="caption">
                    Candidatos evaluados ({decision.all_results.length})
                  </span>
                  <div className="mt-2 overflow-x-auto rounded-md border border-border">
                    <table className="w-full table-fixed">
                      <thead className="bg-surface-inset">
                        <tr>
                          <th className="w-[30%] px-2 py-1.5 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            Candidato
                          </th>
                          <th className="w-[15%] px-2 py-1.5 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            Tamaño
                          </th>
                          <th className="w-[15%] px-2 py-1.5 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            Score
                          </th>
                          <th className="w-[12%] px-2 py-1.5 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            SSIM
                          </th>
                          <th className="w-[12%] px-2 py-1.5 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            BA
                          </th>
                          <th className="w-[12%] px-2 py-1.5 text-right text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            Tiempo
                          </th>
                          <th className="w-[10%] px-2 py-1.5 text-left text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                            Resultado
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {decision.all_results.map((r) => {
                          const isSelected =
                            decision.selected?.candidate.id ===
                            r.candidate.id;
                          return (
                            <tr
                              key={r.candidate.id}
                              className={cn(
                                "border-t border-border",
                                isSelected && "bg-primary-subtle/50",
                              )}
                            >
                              <td className="px-2 py-1.5 text-sm font-medium">
                                <div className="flex items-center gap-1.5">
                                  {isSelected ? (
                                    <CheckCircle2
                                      className="h-3 w-3 text-primary"
                                      aria-hidden="true"
                                    />
                                  ) : null}
                                  {r.candidate.label}
                                </div>
                              </td>
                              <td className="px-2 py-1.5 text-right font-mono text-xs">
                                {formatBytes(r.output_size)}
                              </td>
                              <td className="px-2 py-1.5 text-right font-mono text-xs">
                                {r.score !== null && r.score !== undefined
                                  ? r.score.toFixed(3)
                                  : "—"}
                              </td>
                              <td className="px-2 py-1.5 text-right font-mono text-xs">
                                {r.quality
                                  ? r.quality.is_lossless
                                    ? "lossless"
                                    : formatQuality(r.quality.ssim)
                                  : "—"}
                              </td>
                              <td className="px-2 py-1.5 text-right font-mono text-xs">
                                {r.quality?.butteraugli !== null &&
                                r.quality?.butteraugli !== undefined
                                  ? r.quality.butteraugli.toFixed(3)
                                  : "—"}
                              </td>
                              <td className="px-2 py-1.5 text-right font-mono text-xs">
                                {formatDuration(r.processing_time_ms)}
                              </td>
                              <td className="px-2 py-1.5">
                                <StatusBadge
                                  status={r.success ? "Completed" : "Failed"}
                                  dotOnly
                                />
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </div>
              </>
            ) : null}

            <div className="flex items-center justify-end gap-2 border-t border-border pt-4">
              <Button variant="ghost" size="md" disabled>
                <FileOutput className="h-4 w-4" aria-hidden="true" />
                Abrir archivo
              </Button>
              <Button variant="ghost" size="md" disabled>
                <FolderOpen className="h-4 w-4" aria-hidden="true" />
                Abrir carpeta
              </Button>
              <Button variant="primary" size="md" onClick={reset}>
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
                Optimizar otro
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : null}
    </div>
  );
};

const StatBox: React.FC<{
  label: string;
  value: string;
  valueClassName?: string;
  icon?: React.ElementType;
}> = ({ label, value, valueClassName, icon: Icon }) => (
  <div className="rounded-md border border-border bg-surface-inset p-3">
    <div className="flex items-center justify-between gap-2">
      <span className="caption">{label}</span>
      {Icon ? (
        <Icon className="h-3 w-3 text-muted-foreground" aria-hidden="true" />
      ) : null}
    </div>
    <div
      className={cn(
        "mt-1 font-mono text-xl font-semibold tabular-nums",
        valueClassName ?? "text-foreground",
      )}
    >
      {value}
    </div>
  </div>
);

AutoOptimizerPage.displayName = "AutoOptimizerPage";
