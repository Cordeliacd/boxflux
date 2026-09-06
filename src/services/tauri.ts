/**
 * Servicio de comunicación con el backend Rust vía Tauri 2.
 *
 * Cada función aquí definida corresponde 1:1 a un `#[tauri::command]` en
 * `src-tauri/src/commands/`. La firma de argumentos y retorno debe
 * coincidir con el tipo Rust serializado a JSON.
 *
 * En tests (sin Tauri), `safeInvoke` rechaza con `NotInTauriError` que
 * los context providers pueden capturar para mostrar un fallback.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { safeInvoke, isTauriEnvironment } from "@/lib/tauri-safe";

import type {
  AppSettings,
  EngineInfo,
  Format,
  FormatCapabilityDto,
  HistoryAggregate,
  HistoryEntry,
  Job,
  LicenseStatus,
  LicenseTier,
  LiveStats,
  LogEntry,
  OptimizationDecision,
  OptimizationProfile,
  ProductEntitlement,
  QueueStats,
  ValidationResult,
} from "@/types";

// Engine

export const getEngineInfo = (): Promise<EngineInfo> =>
  safeInvoke<EngineInfo>("get_engine_info");

export const getFormatList = (): Promise<FormatCapabilityDto[]> =>
  safeInvoke<FormatCapabilityDto[]>("get_format_list");

// Queue

export const getQueueSnapshot = (): Promise<Job[]> =>
  safeInvoke<Job[]>("get_queue_snapshot");

export const getQueueStats = (): Promise<QueueStats> =>
  safeInvoke<QueueStats>("get_queue_stats");

export const startQueue = (): Promise<void> => safeInvoke<void>("start_queue");

export const pauseQueue = (): Promise<void> => safeInvoke<void>("pause_queue");

export const resumeQueue = (): Promise<void> => safeInvoke<void>("resume_queue");

export const cancelJob = (jobId: number): Promise<boolean> =>
  safeInvoke<boolean>("cancel_job", { jobId });

export const retryJob = (jobId: number): Promise<number | null> =>
  safeInvoke<number | null>("retry_job", { jobId });

export const removeJob = (jobId: number): Promise<boolean> =>
  safeInvoke<boolean>("remove_job", { jobId });

export const clearCompleted = (): Promise<void> => safeInvoke<void>("clear_completed");

export const enqueueOptimizeJob = (
  source: string,
  mode: "Lossless" | "Compress", // Balanceado (lossless) / Comprimir al máximo
  options?: {
    force_format?: Format | null;
    force_lossless?: boolean;
    strip_all_metadata?: boolean;
    preserve_color_profile?: boolean;
  },
): Promise<number> =>
  safeInvoke<number>("enqueue_optimize_job", {
    args: {
      source,
      mode,
      force_format: options?.force_format ?? null,
      force_lossless: options?.force_lossless ?? false,
      strip_all_metadata: options?.strip_all_metadata ?? false,
      preserve_color_profile: options?.preserve_color_profile ?? true,
    },
  });

export const addDroppedPaths = (paths: string[]): Promise<number> =>
  safeInvoke<number>("add_dropped_paths", { paths });

// Profiles

export const getAllProfiles = (): Promise<OptimizationProfile[]> =>
  safeInvoke<OptimizationProfile[]>("get_all_profiles");

export const getDefaultProfile = (): Promise<string> =>
  safeInvoke<string>("get_default_profile");

export const setDefaultProfile = (name: string): Promise<boolean> =>
  safeInvoke<boolean>("set_default_profile", { name });

// History

export const getHistory = (): Promise<HistoryEntry[]> =>
  safeInvoke<HistoryEntry[]>("get_history");

export const removeHistoryEntry = (id: number): Promise<boolean> =>
  safeInvoke<boolean>("remove_history_entry", { id });

export const clearHistory = (): Promise<void> => safeInvoke<void>("clear_history");

export const getHistoryAggregate = (): Promise<HistoryAggregate> =>
  safeInvoke<HistoryAggregate>("get_history_aggregate");

// Settings

export const getSettings = (): Promise<AppSettings> =>
  safeInvoke<AppSettings>("get_settings");

export const updateSettings = (settings: AppSettings): Promise<boolean> =>
  safeInvoke<boolean>("update_settings", { settings });

export const listThemes = (): Promise<string[]> => safeInvoke<string[]>("list_themes");

// Diagnostics

export const getLogs = (): Promise<LogEntry[]> => safeInvoke<LogEntry[]>("get_logs");

export const clearLogs = (): Promise<void> => safeInvoke<void>("clear_logs");

export const exportLogs = (): Promise<string> => safeInvoke<string>("export_logs");

export const copyDiagnostics = (): Promise<string> =>
  safeInvoke<string>("copy_diagnostics");

export const logDebug = (message: string): Promise<void> =>
  safeInvoke<void>("log_debug", { message });

export const logInfo = (message: string): Promise<void> =>
  safeInvoke<void>("log_info", { message });

export const logWarn = (message: string): Promise<void> =>
  safeInvoke<void>("log_warn", { message });

export const logError = (message: string): Promise<void> =>
  safeInvoke<void>("log_error", { message });

// Licensing

export const getLicenseTier = (): Promise<LicenseTier> =>
  safeInvoke<LicenseTier>("get_license_tier");

export const getLicenseStatus = (): Promise<LicenseStatus> =>
  safeInvoke<LicenseStatus>("get_license_status");

export const getLicenseEntitlement = (): Promise<ProductEntitlement> =>
  safeInvoke<ProductEntitlement>("get_license_entitlement");

export const activateLicense = (key: string): Promise<ProductEntitlement> =>
  safeInvoke<ProductEntitlement>("activate_license", { key });

export const deactivateLicense = (): Promise<void> =>
  safeInvoke<void>("deactivate_license");

export const isEntitledTo = (feature: string): Promise<boolean> =>
  safeInvoke<boolean>("is_entitled_to", { feature });

export const listLicenseTiers = (): Promise<string[]> =>
  safeInvoke<string[]>("list_license_tiers");

export const listLicenseStatuses = (): Promise<string[]> =>
  safeInvoke<string[]>("list_license_statuses");

// Stats

export const getLiveStats = (): Promise<LiveStats> => safeInvoke<LiveStats>("get_live_stats");

export const getBatchSummary = (): Promise<string> => safeInvoke<string>("get_batch_summary");

// FS

export const validateInputFile = (
  path: string,
  maxSizeBytes = 0,
): Promise<ValidationResult> =>
  safeInvoke<ValidationResult>("validate_input_file", { path, maxSizeBytes });

export const validateOutputPath = (
  output: string,
  input?: string,
): Promise<ValidationResult> =>
  safeInvoke<ValidationResult>("validate_output_path", { output, input: input ?? null });

export const discoverSupportedFiles = (
  dir: string,
  recursive = true,
): Promise<string[]> => safeInvoke<string[]>("discover_supported_files", { dir, recursive });

export const isSupportedImageFile = (path: string): Promise<boolean> =>
  safeInvoke<boolean>("is_supported_image_file", { path });

export const computeOutputPath = (
  inputPath: string,
  mode: string,
  customFolder?: string,
  preserveStructure = false,
  filenameTemplate = "",
  targetExtension?: string,
): Promise<string> =>
  safeInvoke<string>("compute_output_path", {
    inputPath,
    mode,
    customFolder: customFolder ?? null,
    preserveStructure,
    filenameTemplate,
    targetExtension: targetExtension ?? null,
  });

// Auto-optimize

export interface AutoOptimizeArgs {
  input_path: string;
  output_dir: string;
  goal: string;
  force_lossless: boolean;
  strip_all_metadata: boolean;
  /** Formato de salida forzado (modo Manual). null = modo Auto. */
  force_format?: string | null;
  /** Si es true, preserva el perfil de color ICC del original. */
  preserve_color_profile?: boolean;
}

export const startAutoOptimize = (args: AutoOptimizeArgs): Promise<number> =>
  safeInvoke<number>("start_auto_optimize", { args });

export const cancelAutoOptimize = (): Promise<void> =>
  safeInvoke<void>("cancel_auto_optimize");

export const isAutoOptimizeRunning = (): Promise<boolean> =>
  safeInvoke<boolean>("is_auto_optimize_running");

export const listIntelBackends = (): Promise<Array<[string, string]>> =>
  safeInvoke<Array<[string, string]>>("list_intel_backends");

// Events
//
// listen() también requiere Tauri presente; lo blindamos igual que invoke.

function safeListen<T = unknown>(
  eventName: string,
  cb: (payload: T) => void,
): Promise<UnlistenFn> {
  // Sin Tauri devolvemos un no-op, así los callers pueden hacer
  // `await onJobUpdated(...)` sin explotar.
  if (!isTauriEnvironment()) {
    return Promise.resolve((() => {}) as UnlistenFn);
  }
  return listen<T>(eventName, (e) => cb(e.payload));
}

export { isNotInTauriError, NotInTauriError } from "@/lib/tauri-safe";
// Re-export del helper de entorno para contexts/topbar.
export { isTauriEnvironment };

// Event listeners

export function onJobUpdated(cb: (job: Job) => void): Promise<UnlistenFn> {
  return safeListen<Job>("boxflux://job-updated", cb);
}

export function onStatsChanged(cb: (stats: QueueStats) => void): Promise<UnlistenFn> {
  return safeListen<QueueStats>("boxflux://stats-changed", cb);
}

export function onAutoOptimizeStarted(
  cb: (requestId: number) => void,
): Promise<UnlistenFn> {
  return safeListen<{ request_id: number }>("boxflux://auto-optimize-started", (p) =>
    cb(p.request_id),
  );
}

export function onAutoOptimizeFinished(
  cb: (requestId: number, decision: OptimizationDecision) => void,
): Promise<UnlistenFn> {
  return safeListen<{ request_id: number; decision: OptimizationDecision }>(
    "boxflux://auto-optimize-finished",
    (p) => cb(p.request_id, p.decision),
  );
}

export function onAutoOptimizeFailed(
  cb: (requestId: number, error: string) => void,
): Promise<UnlistenFn> {
  return safeListen<{ request_id: number; error: string }>(
    "boxflux://auto-optimize-failed",
    (p) => cb(p.request_id, p.error),
  );
}

export function onAutoOptimizeCancelled(
  cb: (requestId: number, reason: string) => void,
): Promise<UnlistenFn> {
  return safeListen<{ request_id: number; reason: string }>(
    "boxflux://auto-optimize-cancelled",
    (p) => cb(p.request_id, p.reason),
  );
}

export function onAutoOptimizeStateChanged(
  cb: (running: boolean) => void,
): Promise<UnlistenFn> {
  return safeListen<{ running: boolean }>("boxflux://auto-optimize-state-changed", (p) =>
    cb(p.running),
  );
}
