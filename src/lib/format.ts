/**
 * Formatters de la UI. Espejo de `util::format` del backend Rust:
 * mismo output para que números en pantalla y en logs coincidan.
 */

/**
 * Formatea bytes en formato binario (KiB, MiB, GiB, TiB, PiB).
 * Devuelve `"0 B"` para 0 bytes.
 */
export function formatBytes(bytes: number): string {
  const UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"] as const;
  if (bytes === 0) return "0 B";
  if (bytes < 0) return `-${formatBytes(-bytes)}`;
  let idx = 0;
  let value = bytes;
  while (value >= 1024 && idx < UNITS.length - 1) {
    value /= 1024;
    idx += 1;
  }
  if (idx === 0) return `${bytes} ${UNITS[0]}`;
  return `${value.toFixed(1)} ${UNITS[idx]}`;
}

/**
 * Formatea milisegundos como `"M:SS"` o `"H:MM:SS"`. Para valores < 1 s
 * devuelve `"N ms"`.
 */
export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  const seconds = Math.floor(milliseconds / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

/**
 * Formatea un valor 0..1 como porcentaje con 1 decimal.
 */
export function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

/**
 * Color de fondo sutil para un status de job (para badges).
 */
export function subtleForStatus(status: string): string {
  switch (status) {
    case "Queued":
      return "bg-muted text-muted-foreground";
    case "Processing":
      return "bg-warning-subtle text-warning-foreground";
    case "Completed":
      return "bg-success-subtle text-success-foreground";
    case "Failed":
      return "bg-error-subtle text-error-foreground";
    case "Cancelled":
    case "Skipped":
      return "bg-muted text-muted-foreground";
    default:
      return "bg-muted text-muted-foreground";
  }
}

/**
 * Color asociado al porcentaje de reducción:
 * - >= 40% → success
 * - >= 15% → primary (accento)
 * - > 0% → muted-foreground
 * - <= 0% → disabled
 */
export function colorForReduction(percentage: number): string {
  if (percentage >= 40) return "text-success";
  if (percentage >= 15) return "text-primary";
  if (percentage > 0) return "text-muted-foreground";
  return "text-disabled";
}

/**
 * Formatea un quality score 0..1 como 3 dígitos, p.ej. `"098"`.
 * Devuelve `"—"` si el valor es NaN.
 */
export function formatQuality(value: number): string {
  if (Number.isNaN(value)) return "—";
  const pct = Math.round(Math.max(0, Math.min(1, value)) * 100);
  return String(pct).padStart(3, "0");
}

/**
 * Trunca un string en el medio si es más largo que `maxLen`.
 * p.ej. `truncateMiddle("/very/long/path/to/file.png", 24)` → `/very/…/file.png`
 */
export function truncateMiddle(s: string, maxLen: number): string {
  if (s.length <= maxLen) return s;
  const sep = "…";
  const sepLen = sep.length;
  const front = Math.ceil((maxLen - sepLen) / 2);
  const back = Math.floor((maxLen - sepLen) / 2);
  return `${s.slice(0, front)}${sep}${s.slice(s.length - back)}`;
}

/**
 * Devuelve el basename de un path (último segmento después del separador).
 */
export function basename(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/**
 * Calcula el porcentaje ahorrado de un job. Útil en la UI porque el
 * backend Rust envía el job como struct plana (sin métodos).
 */
export function jobPercentageSaved(job: {
  original_size: number;
  final_size: number;
}): number {
  if (job.original_size === 0) return 0;
  return (
    (100 * (job.original_size - job.final_size)) / job.original_size
  );
}
