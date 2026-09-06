//! Historial de ejecuciones persistido en JSONL (una línea por entrada).

use std::io::Write;
use std::path::{Path, PathBuf};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::util::id;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    pub files_processed: u32,
    pub files_failed: u32,
    pub profile_name: String,
    pub original_size: u64,
    pub output_size: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Aggregate {
    pub total_runs: u64,
    pub total_files_processed: u64,
    pub total_files_failed: u64,
    pub total_original_bytes: u64,
    pub total_output_bytes: u64,
    pub total_duration_ms: u64,
}

pub struct HistoryManager {
    entries: RwLock<Vec<HistoryEntry>>,
    path: PathBuf,
}

impl HistoryManager {
    pub fn new(path: PathBuf) -> Self {
        let entries = Self::load(&path);
        Self {
            entries: RwLock::new(entries),
            path,
        }
    }

    fn load(path: &Path) -> Vec<HistoryEntry> {
        let mut entries = Vec::new();
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<HistoryEntry>(line) {
                        Ok(entry) => entries.push(entry),
                        Err(e) => {
                            tracing::warn!("línea inválida en history.jsonl: {e}");
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!("error leyendo history.jsonl: {e}");
            }
        }
        // Sort newest first.
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        // Cap también al cargar: el archivo físico puede traer más
        // entradas de las que caben en memoria.
        entries.truncate(1000);
        entries
    }

    fn persist_entry(entry: &HistoryEntry, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("error creando dir para history.jsonl: {e}");
                return;
            }
        }
        let mut json = match serde_json::to_string(entry) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("error serializando entry: {e}");
                return;
            }
        };
        json.push('\n');
        let mut file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("error abriendo history.jsonl: {e}");
                return;
            }
        };
        if let Err(e) = file.write_all(json.as_bytes()) {
            tracing::error!("error escribiendo history.jsonl: {e}");
        }
    }

    /// Añade una entrada. Le asigna ID y timestamp automáticamente.
    pub fn append(&self, mut entry: HistoryEntry) -> u64 {
        if entry.id == 0 {
            entry.id = id::next_id();
        }
        if entry.timestamp.is_empty() {
            entry.timestamp = chrono::Utc::now().to_rfc3339();
        }
        let id = entry.id;
        // La escritura a disco va DENTRO del lock: fuera de él, dos
        // appends concurrentes podrían persistir en orden inverso al
        // de memoria.
        let mut entries = self.entries.write();
        Self::persist_entry(&entry, &self.path);
        entries.insert(0, entry);
        // Cap de 1000 entradas: al superarlo reescribimos el archivo
        // físico truncado para que disco y memoria coincidan.
        if entries.len() > 1000 {
            entries.truncate(1000);
            Self::rewrite_all(&entries, &self.path);
        }
        id
    }

    /// Devuelve todas las entradas (de más nueva a más vieja).
    pub fn all(&self) -> Vec<HistoryEntry> {
        self.entries.read().clone()
    }

    /// Elimina una entrada por ID. Devuelve `true` si se eliminó.
    pub fn remove(&self, id: u64) -> bool {
        let mut entries = self.entries.write();
        let before = entries.len();
        entries.retain(|e| e.id != id);
        let removed = entries.len() < before;
        if removed {
            Self::rewrite_all(&entries, &self.path);
        }
        removed
    }

    /// Elimina todo el historial.
    pub fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();
        Self::rewrite_all(&entries, &self.path);
    }

    fn rewrite_all(entries: &[HistoryEntry], path: &Path) {
        // Escritura atómica (tmp+rename): o queda el historial anterior
        // completo, o el nuevo — nunca un archivo truncado a mitad de
        // un rewrite fallido.
        let mut buf: Vec<u8> = Vec::new();
        for entry in entries.iter().rev() {
            // entries está ordenado newest-first; queremos append oldest-first
            // para mantener el orden cronológico natural.
            if let Ok(json) = serde_json::to_string(entry) {
                buf.extend_from_slice(json.as_bytes());
                buf.push(b'\n');
            }
        }
        if let Err(e) = crate::util::fs_io::write_atomic(path, &buf) {
            tracing::error!("error reescribiendo history.jsonl: {e}");
        }
    }

    /// Agregados de todo el historial.
    pub fn aggregate(&self) -> Aggregate {
        let entries = self.entries.read();
        let mut agg = Aggregate::default();
        agg.total_runs = entries.len() as u64;
        for e in entries.iter() {
            agg.total_files_processed += e.files_processed as u64;
            agg.total_files_failed += e.files_failed as u64;
            agg.total_original_bytes += e.original_size;
            agg.total_output_bytes += e.output_size;
            agg.total_duration_ms += e.duration_ms;
        }
        agg
    }
}
