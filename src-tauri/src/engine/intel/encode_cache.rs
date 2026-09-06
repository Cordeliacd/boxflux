//! Deduplicación de encodes AVIF dentro de un mismo run: el iterative
//! search y las anclas fijas comparten parámetros, así que hasta 3
//! encodes por imagen eran duplicados exactos. Cache por run con
//! protocolo productor/consumidor; los archivos viven en output_dir
//! con el prefijo que barre el sweep de temporales.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::engine::optimization::OptimizationProfile;

use super::cancel::CancellationToken;

/// Prefijo de los archivos del cache (compatible con el sweep de
/// `output_safety`: `.` + `.boxflux-` + `.tmp`).
const CACHE_FILE_PREFIX: &str = ".boxflux-dedup-";

/// Tope de espera por un productor que no publica. Pasado el tope, el
/// esperador "roba" la producción (codifica él mismo): es la válvula
/// contra un productor muerto por pánico sin `abandon`. Ambos caminos
/// producen bytes idénticos (encoder determinista), así que robar es
/// seguro para el resultado — solo duplica trabajo en el peor caso.
const PRODUCER_WAIT_CAP: Duration = Duration::from_secs(180);

/// Poll del condvar: además del despertar por notificación, se
/// re-chequea cancelación con este periodo.
const WAIT_POLL: Duration = Duration::from_millis(250);

/// Resultado de [`EncodeDedupCache::lookup`].
#[derive(Debug)]
pub enum DedupLookup {
    /// El encode ya existe: ruta al archivo cacheado + tamaño en B.
    Hit(PathBuf, u64),
    /// No existe (o el productor anterior falló): el llamador debe
    /// codificar y publicar con `store` (o liberar con `abandon`).
    Produce,
    /// Cancelación mientras se esperaba a otro productor.
    Cancelled,
}

#[derive(Debug)]
enum Entry {
    /// Encode disponible en `path` con `size` bytes.
    Ready { path: PathBuf, size: u64 },
    /// Alguien está codificando esta clave ahora mismo.
    InFlight,
}

/// Cache de encodes AVIF deduplicables, con vida == un run del
/// pipeline. Ver documentación del módulo.
#[derive(Debug)]
pub struct EncodeDedupCache {
    /// `false` (modo `disabled`): `lookup` siempre `Produce`, `store`
    /// no-op — para goals sin search AVIF.
    enabled: bool,
    dir: PathBuf,
    state: Mutex<HashMap<String, Entry>>,
    changed: Condvar,
    seq: AtomicU32,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl EncodeDedupCache {
    /// Cache activo cuyos archivos viven en `output_dir`.
    pub fn new(output_dir: &Path) -> Self {
        Self {
            enabled: true,
            dir: output_dir.to_path_buf(),
            state: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
            seq: AtomicU32::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Cache inerte: para goals sin iterative-AVIF (no tienen
    /// duplicados).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            dir: PathBuf::new(),
            state: Mutex::new(HashMap::new()),
            changed: Condvar::new(),
            seq: AtomicU32::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// (hits, misses) — contadores de diagnóstico.
    pub fn stats(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }

    /// Busca un encode para `key`.
    ///
    /// - `Hit` → el llamador copia el archivo y ahorra el encode.
    /// - `Produce` → el llamador quedó registrado como productor:
    ///   DEBE llamar `store` (éxito) o `abandon` (fallo/cancelación).
    /// - `Cancelled` → el token se canceló mientras se esperaba.
    pub fn lookup(&self, key: &str, token: &CancellationToken) -> DedupLookup {
        if !self.enabled {
            return DedupLookup::Produce;
        }
        let mut guard = self.state.lock();
        let deadline = Instant::now() + PRODUCER_WAIT_CAP;
        loop {
            match guard.get(key) {
                Some(Entry::Ready { path, size }) => {
                    let hit = (path.clone(), *size);
                    drop(guard);
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return DedupLookup::Hit(hit.0, hit.1);
                }
                Some(Entry::InFlight) => {
                    if token.is_cancelled() {
                        return DedupLookup::Cancelled;
                    }
                    if Instant::now() >= deadline {
                        // Productor estancado (pánico sin abandon):
                        // robar la producción. Los bytes serán los
                        // mismos (encoder determinista).
                        guard.insert(key.to_string(), Entry::InFlight);
                        self.misses.fetch_add(1, Ordering::Relaxed);
                        return DedupLookup::Produce;
                    }
                    self.changed.wait_for(&mut guard, WAIT_POLL);
                }
                None => {
                    guard.insert(key.to_string(), Entry::InFlight);
                    self.misses.fetch_add(1, Ordering::Relaxed);
                    return DedupLookup::Produce;
                }
            }
        }
    }

    /// Publica el encode producido: copia el archivo al cache (copia
    /// PROPIA — sobrevive al cleanup del productor) y despierta a los
    /// esperadores. Si la copia falla (disco), la clave se libera para
    /// que otros codifiquen: el dedupe es una optimización, nunca una
    /// fuente de fallos.
    pub fn store(&self, key: &str, file: &Path) {
        if !self.enabled {
            return;
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let cached = self.dir.join(format!("{CACHE_FILE_PREFIX}{seq}.tmp"));
        let copied = std::fs::create_dir_all(&self.dir)
            .and_then(|_| std::fs::copy(file, &cached))
            .and_then(|_| std::fs::metadata(&cached))
            .map(|m| m.len());
        let mut guard = self.state.lock();
        match copied {
            Ok(actual_size) => {
                guard.insert(
                    key.to_string(),
                    Entry::Ready {
                        path: cached,
                        size: actual_size,
                    },
                );
            }
            Err(_) => {
                // Sin copia no hay dedupe: liberar la clave.
                guard.remove(key);
            }
        }
        drop(guard);
        self.changed.notify_all();
    }

    /// Libera una clave `InFlight` cuyo productor falló: el siguiente
    /// consumidor la codificará él mismo.
    pub fn abandon(&self, key: &str) {
        if !self.enabled {
            return;
        }
        let mut guard = self.state.lock();
        if matches!(guard.get(key), Some(Entry::InFlight)) {
            guard.remove(key);
        }
        drop(guard);
        self.changed.notify_all();
    }
}

impl Drop for EncodeDedupCache {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        // Borrar TODOS los archivos del cache del directorio (no solo
        // los trackeados: cubre también el caso de doble-store tras un
        // robo de producción). Best-effort.
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let name = e.file_name();
                if name
                    .to_str()
                    .map(|n| n.starts_with(CACHE_FILE_PREFIX))
                    .unwrap_or(false)
                {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

/// Clave de dedupe: TODOS los campos del `OptimizationProfile` que
/// determinan los bytes de un encode AVIF (más los que podrían pasar
/// a importar: metadata/ICC). El resto (name, jpeg_*, webp_*,
/// png_*) NO participa — diferir en ellos no cambia el AVIF y no
/// debe romper el dedupe (el label del search y el del ancla son
/// distintos por construcción).
///
/// El `input` no participa: el cache vive UN solo run (= una sola
/// imagen), así que el input es constante por construcción.
pub fn avif_encode_key(p: &OptimizationProfile) -> String {
    format!(
        "avif|q{}|a{}|b{}|m{:?}|icc{}|r{}",
        p.avif_quality,
        p.avif_alpha_quality,
        p.avif_time_budget_ms,
        p.metadata_mode,
        p.preserve_color_profile,
        resize_key(p.resize.as_ref()),
    )
}

fn resize_key(r: Option<&crate::engine::optimization::ResizeOptions>) -> String {
    match r {
        None => "none".to_string(),
        Some(r) => format!(
            "{:?}|{}|{}|{}|{}|{}",
            r.mode,
            r.target_width
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            r.target_height
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            r.percentage
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            r.max_width
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            r.max_height
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
        ),
    }
}
