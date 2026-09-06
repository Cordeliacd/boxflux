/**
 * Tipos TypeScript compartidos: espejo 1:1 de los DTOs que serializa
 * el backend Rust (formatos, cola, settings, licensing, diagnostics,
 * history, statistics). Si cambia un DTO en Rust, toca cambiarlo aquí.
 */

// Engine

export type Format =
  | "PNG"
  | "JPEG"
  | "WebP"
  | "AVIF"
  | "GIF"
  | "BMP"
  | "TIFF"
  | "UNKNOWN";

/**
 * Ojo: los nombres solapan con OptimizationGoal, pero son cosas
 * distintas — perfil de usuario guardado vs goal del pipeline.
 */
export type ProfileKind =
  | "Web"
  | "Lossless"
  | "Custom";

export type MetadataMode = "Keep" | "RemoveSafe" | "RemoveAll";
export type ResizeMode = "Fit" | "Fill" | "Stretch";

export interface FormatCapabilities {
  can_read: boolean;
  can_write: boolean;
  can_optimize: boolean;
  can_convert: boolean;
}

export interface FormatInfo {
  format: Format;
  width: number;
  height: number;
  file_size: number;
  has_alpha: boolean;
  color_type: string;
  bit_depth: number;
}

export interface FormatCapabilityDto {
  format: string;
  can_read: boolean;
  can_write: boolean;
  can_optimize: boolean;
  can_convert: boolean;
}

export interface OptimizationProfile {
  name: string;
  kind: ProfileKind;
  jpeg_quality: number;
  jpeg_progressive: boolean;
  /** Si es true, JPEG usa chroma 4:4:4 (sin subsampling) para mayor fidelidad. */
  jpeg_chroma_444?: boolean;
  webp_quality: number;
  webp_lossless: boolean;
  avif_quality: number;
  avif_alpha_quality: number;
  png_optimization_level: number;
  metadata_mode: MetadataMode;
  /** Si es true, preserva el perfil de color ICC del original. */
  preserve_color_profile: boolean;
  resize?: ResizeOptions | null;
}

export interface ResizeOptions {
  target_width?: number | null;
  target_height?: number | null;
  percentage?: number | null;
  max_width?: number | null;
  max_height?: number | null;
  mode: ResizeMode;
}

export interface OptimizationResult {
  original_size: number;
  output_size: number;
  bytes_saved: number;
  percentage_saved: number;
  processing_time_ms: number;
  format: Format;
  success: boolean;
  error: string;
}

export interface ConversionResult extends OptimizationResult {
  input_format: Format;
  output_format: Format;
}

export interface EngineInfo {
  engine_version: string;
  intel_engine_version: string;
  max_workers: number;
}

// Queue / Jobs

export type JobStatus =
  | "Queued"
  | "Processing"
  | "Completed"
  | "Failed"
  | "Cancelled"
  | "Skipped";

export type JobOperation = "Optimize" | "Resize";

/**
 * Modo de optimización, solo hay dos:
 * - `Lossless` (default): codecs lossless, cero pérdida.
 * - `Compress`: mínimo tamaño con la menor pérdida perceptual
 *   (gate butteraugli medido, búsqueda iterativa JPEG/WebP/AVIF).
 */
export type OptimizationMode = "Lossless" | "Compress";

export interface Job {
  id: number;
  operation: JobOperation;
  source_path: string;
  destination_path: string;
  input_format: Format;
  output_format: Format;
  optimization_mode: OptimizationMode;
  resize?: ResizeOptions | null;
  original_size: number;
  final_size: number;
  progress: number;
  status: JobStatus;
  processing_time_ms: number;
  /** Formato elegido por el motor (p.ej. "WebP", "AVIF"). */
  selected_format: string;
  /** Calidad elegida por el motor (p.ej. "q80", "lossless"). */
  selected_quality: string;
  /** Explicación humana de la decisión del motor. */
  decision_explanation: string;
  error: string;
  /** Formato de salida forzado por el usuario (modo Manual). null = modo Auto. */
  force_format?: Format | null;
  /** Si es true, fuerza salida lossless sin importar el goal. */
  force_lossless: boolean;
  /** Si es true, elimina toda la metadata (EXIF, XMP, ICC). */
  strip_all_metadata: boolean;
  /** Si es true, preserva el perfil de color ICC del original. */
  preserve_color_profile: boolean;
}

export interface QueueStats {
  total_jobs: number;
  queued: number;
  processing: number;
  completed: number;
  failed: number;
  cancelled: number;
  skipped: number;
  original_bytes: number;
  final_bytes: number;
  total_processing_ms: number;
  active_workers: number;
}

// Settings

export type Theme = "Light" | "Dark" | "System";
export type OutputMode = "SameFolder" | "OutputSubfolder" | "CustomFolder";
export type MetadataBehavior = "Keep" | "RemoveSafe" | "RemoveAll";

export interface AppSettings {
  theme: Theme;
  language: string;
  start_minimized: boolean;
  notifications_enabled: boolean;
  worker_count: number;
  automatic_processing: boolean;
  default_profile: string;
  default_quality: number;
  metadata_behavior: MetadataBehavior;
  default_output_format: string;
  output_mode: OutputMode;
  custom_output_folder: string;
  preserve_folder_structure: boolean;
  filename_template: string;
  cpu_limit_percent: number;
  max_memory_mb: number;
  logging_enabled: boolean;
  diagnostics_enabled: boolean;
  experimental_features: boolean;
}

// History

export interface HistoryEntry {
  id: number;
  timestamp: string;
  files_processed: number;
  files_failed: number;
  profile_name: string;
  original_size: number;
  output_size: number;
  duration_ms: number;
}

export interface HistoryAggregate {
  total_runs: number;
  total_files_processed: number;
  total_files_failed: number;
  total_original_bytes: number;
  total_output_bytes: number;
  total_duration_ms: number;
}

// Statistics

export interface LiveStats {
  files_processed: number;
  files_failed: number;
  bytes_processed_in: number;
  bytes_processed_out: number;
  total_processing_ms: number;
  files_queued: number;
  files_processing: number;
  active_workers: number;
  max_workers: number;
  space_saved_bytes: number;
  compression_percentage: number;
  processing_speed_files_per_sec: number;
}

// Diagnostics

export type LogLevel = "debug" | "info" | "warning" | "error";
export type LogSource = "rust" | "frontend" | "system";

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
}

// Licensing

export type LicenseTier = "Free" | "Standard" | "Pro" | "Team";
export type LicenseStatus =
  | "Unknown"
  | "Valid"
  | "Expired"
  | "Revoked"
  | "Invalid";

export interface ProductEntitlement {
  product_id: string;
  product_name: string;
  tier: LicenseTier;
  status: LicenseStatus;
  owner_email: string;
  issued_at: string;
  expires_at: string;
  is_trial: boolean;
  is_commercial_use_allowed: boolean;
}

// Intel / Auto-optimize

export type OptimizationGoal =
  | "Lossless"
  | "Quality"
  | "Balanced"
  | "MaximumCompression"
  | "Web"
  | "ExtremeLightweight"
  | "Custom";

export interface UserConstraints {
  max_output_size?: number | null;
  force_format?: Format | null;
  force_lossless: boolean;
  max_width?: number | null;
  max_height?: number | null;
  strip_all_metadata: boolean;
  max_processing_time_ms?: number | null;
  /** Si es true, preserva el perfil de color ICC del original. */
  preserve_color_profile: boolean;
  /** Presupuesto de memoria en MB (0 = sin límite). */
  memory_budget_mb?: number;
}

export interface GoalWeights {
  quality_weight: number;
  compression_weight: number;
  speed_weight: number;
  compatibility_weight: number;
  min_quality_threshold: number;
  allow_lossy: boolean;
  preferred_formats: Format[];
  max_candidates: number;
  max_processing_time_ms: number;
}

export interface QualityMetrics {
  mse: number;
  psnr: number;
  ssim: number;
  is_lossless: boolean;
  /** butteraugli perceptual score (null = no calculado). < 1.0 = idéntico. */
  butteraugli?: number | null;
}

export interface Candidate {
  id: number;
  format: Format;
  quality?: number | null;
  lossless: boolean;
  resize?: ResizeOptions | null;
  metadata_mode: MetadataMode;
  estimated_effort: number;
  label: string;
  /** Presupuesto de memoria en MB (0 = sin límite). */
  memory_budget_mb?: number;
}

export interface CandidateResult {
  candidate: Candidate;
  success: boolean;
  error?: string | null;
  output_path?: string | null;
  output_size: number;
  processing_time_ms: number;
  backend_name: string;
  score?: number | null;
  quality?: QualityMetrics | null;
}

export interface RejectedCandidate {
  candidate: Candidate;
  reason: string;
}

export interface OptimizationDecision {
  input_path: string;
  selected?: CandidateResult | null;
  all_results: CandidateResult[];
  rejected: RejectedCandidate[];
  strategy_summary: string;
  profile_summary: string;
  explanation: string;
  original_size: number;
  final_size: number;
  kept_original: boolean;
}

// FS validation

export type ValidationIssue =
  | "Ok"
  | "EmptyPath"
  | "PathTraversal"
  | "DoesNotExist"
  | "NotAFile"
  | "NotADirectory"
  | "UnsupportedExtension"
  | "OutputSameAsInput"
  | "OutputWouldOverwriteOriginal"
  | "PermissionDenied"
  | "InvalidUnicode"
  | "TooLarge";

export interface ValidationResult {
  issue: ValidationIssue;
  message: string;
}
