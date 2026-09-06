//! Formateadores de bytes, duración, porcentajes y quality scores.
//!
//! Sin locale y sin separador de miles: la salida es determinista.

/// Formatea bytes en formato binario (KiB, MiB, GiB, TiB, PiB).
/// Devuelve `"0 B"` para 0 bytes.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut idx = 0;
    let mut value = bytes as f64;
    while value >= 1024.0 && idx < UNITS.len() - 1 {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", value, UNITS[idx])
    }
}

/// Formatea milisegundos como `"M:SS"` o `"H:MM:SS"`. Para valores < 1 s
/// devuelve `"N ms"`.
pub fn format_duration(milliseconds: u64) -> String {
    if milliseconds < 1000 {
        return format!("{} ms", milliseconds);
    }
    let seconds = milliseconds / 1000;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}

/// Formatea un valor 0..1 como porcentaje con 1 decimal.
pub fn format_percent(value: f64) -> String {
    format!("{:.1}%", value)
}

/// Color de status asociado a un JobStatus string.
/// Usado por el frontend para pintar badges.
pub fn color_for_status(status: &str) -> &'static str {
    match status {
        "Queued" | "EnCola" => "queued",
        "Processing" | "Procesando" => "processing",
        "Completed" | "Completado" => "completed",
        "Failed" | "Fallido" => "failed",
        "Cancelled" | "Cancelado" => "cancelled",
        "Skipped" | "Omitido" => "skipped",
        _ => "unknown",
    }
}

/// Color de reduction (porcentaje ahorrado):
/// - >= 40% → success
/// - >= 15% → accent
/// - > 0% → muted
/// - <= 0% → disabled
pub fn color_for_reduction(percentage: f64) -> &'static str {
    if percentage >= 40.0 {
        "success"
    } else if percentage >= 15.0 {
        "accent"
    } else if percentage > 0.0 {
        "muted"
    } else {
        "disabled"
    }
}

/// Formatea un quality score 0..1 a 3 dígitos, p.ej. `"098"`.
/// Devuelve `"—"` si el valor es NaN.
pub fn format_quality(value: f64) -> String {
    if value.is_nan() {
        return "—".to_string();
    }
    let pct = (value.clamp(0.0, 1.0) * 100.0).round() as u32;
    format!("{:03}", pct)
}
