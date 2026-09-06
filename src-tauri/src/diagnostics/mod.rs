//! Diagnostics: logs en memoria y system info.
//!
//! Ring buffer thread-safe (`RwLock` + `VecDeque`) con cap de 5000 entradas.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Nivel de log. Orden: Debug < Info < Warning < Error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

/// Origen del log. Ayuda a distinguir entradas del backend Rust vs el
/// frontend React vs otros módulos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    Rust,
    Frontend,
    System,
}

impl LogSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogSource::Rust => "rust",
            LogSource::Frontend => "frontend",
            LogSource::System => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

const MAX_ENTRIES: usize = 5000;

/// Diagnostics ring buffer. Thread-safe, capped at `MAX_ENTRIES`.
#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<RwLock<VecDeque<LogEntry>>>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(VecDeque::with_capacity(MAX_ENTRIES))),
        }
    }

    pub fn log(&self, level: LogLevel, source: &str, message: &str) {
        let entry = LogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            source: source.to_string(),
            message: message.to_string(),
        };
        // Mirror to `tracing` for production observability.
        match level {
            LogLevel::Debug => tracing::debug!(source = source, "{}", message),
            LogLevel::Info => tracing::info!(source = source, "{}", message),
            LogLevel::Warning => tracing::warn!(source = source, "{}", message),
            LogLevel::Error => tracing::error!(source = source, "{}", message),
        }
        let mut buf = self.inner.write();
        if buf.len() >= MAX_ENTRIES {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    pub fn debug(&self, msg: &str) {
        self.log(LogLevel::Debug, "rust", msg);
    }
    pub fn info(&self, msg: &str) {
        self.log(LogLevel::Info, "rust", msg);
    }
    pub fn warn(&self, msg: &str) {
        self.log(LogLevel::Warning, "rust", msg);
    }
    pub fn error(&self, msg: &str) {
        self.log(LogLevel::Error, "rust", msg);
    }

    /// Devuelve todas las entradas (de más vieja a más nueva).
    pub fn all(&self) -> Vec<LogEntry> {
        self.inner.read().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.inner.write().clear();
    }

    /// Exporta como texto plano: `YYYY-MM-DD HH:MM:SS [LEVEL] [source] message`.
    pub fn export_as_text(&self) -> String {
        let buf = self.inner.read();
        let mut out = String::with_capacity(buf.len() * 80);
        for e in buf.iter() {
            let ts = chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|_| e.timestamp.clone());
            out.push_str(&format!(
                "{} [{}] [{}] {}\n",
                ts,
                e.level.as_str(),
                e.source,
                e.message
            ));
        }
        out
    }

    /// Diagnostics summary para la UI: versión del engine, max workers,
    /// formatos soportados, total entries.
    pub fn diagnostics_summary(&self, engine: &crate::engine::Engine) -> String {
        let buf = self.inner.read();
        let formats = engine.list_formats();
        let formats_str = formats
            .iter()
            .map(|(f, _)| f.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "BoxFlux v{}\nIntel engine v{}\nMax workers: {}\nFormats: {}\nLog entries: {}",
            crate::engine::ENGINE_VERSION,
            crate::engine::INTEL_ENGINE_VERSION,
            engine.max_workers(),
            formats_str,
            buf.len()
        )
    }
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}
