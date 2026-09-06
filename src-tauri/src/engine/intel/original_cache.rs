//! Cache de la imagen original decodificada, compartida entre todas
//! las evaluaciones de calidad de un mismo run del pipeline: sin ella
//! cada candidato lossy re-decodifica el original (cientos de ms por
//! decode en imágenes grandes).
//!
//! Vida == un run de `optimize` (sin estado global; runs concurrentes
//! con caches independientes). Guarda un único `Arc<DynamicImage>`:
//! acotado por construcción. Respeta el tope de 50 MP de
//! `QualityEvaluator`. Si dos tareas Rayon compiten, el mutex
//! serializa el decode: solo una decodifica y la otra observa el Arc
//! (lazy-init clásico).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::engine::formats::{Format, FormatError};

/// Defensa en profundidad: rechazar imágenes cuyo conteo de píxeles
/// arriesgaría OOM en la evaluación de calidad. Debe coincidir con el
/// tope de `QualityEvaluator` para que cache y evaluador acuerden.
const MAX_QUALITY_EVAL_PIXELS: u64 = 50_000_000;

/// Estado interno: `Empty` hasta que el primer candidato lossy dispara
/// el decode; `Decoded` después; `Failed` si el primer decode falló
/// (así los siguientes no reintentan el mismo fallo N veces).
enum CacheState {
    Empty,
    Decoded { image: Arc<image::DynamicImage> },
    Failed(FormatError),
}

impl Default for CacheState {
    fn default() -> Self {
        Self::Empty
    }
}

/// Cache por-run de la imagen original decodificada.
///
/// Se crea al inicio de una llamada a `optimize` y se destruye al
/// terminar. El primer candidato lossy dispara el decode; el resto
/// reutiliza el `Arc`.
///
/// Clonar un `OriginalImageCache` es barato: clona el
/// `Arc<Mutex<CacheState>>`; todos los clones comparten la misma
/// imagen.
#[derive(Clone)]
pub struct OriginalImageCache {
    inner: Arc<Mutex<CacheState>>,
    source_path: Arc<PathBuf>,
    source_format: Format,
}

impl OriginalImageCache {
    /// Crea un cache para la ruta fuente. El original NO se decodifica
    /// hasta llamar a `get_or_decode`.
    pub fn new(source_path: PathBuf, source_format: Format) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheState::Empty)),
            source_path: Arc::new(source_path),
            source_format,
        }
    }

    /// Devuelve la imagen original decodificada; decodifica en la
    /// primera llamada.
    ///
    /// Las siguientes devuelven el mismo `Arc<DynamicImage>` (sin
    /// re-decode). Si el primer decode falló, las siguientes devuelven
    /// el mismo error (sin tormenta de reintentos).
    ///
    /// Devuelve `Err` si la imagen no se puede decodificar o supera
    /// `MAX_QUALITY_EVAL_PIXELS`.
    pub fn get_or_decode(&self) -> Result<Arc<image::DynamicImage>, FormatError> {
        // Se retiene el mutex DURANTE el decode: serializa los primeros
        // llamados concurrentes (queremos exactamente un decode) y el
        // segundo llamador espera al primero. Soltar el lock durante el
        // decode exigiría double-checked locking y el decode tarda <1 s
        // en imágenes típicas.
        let mut state = self.inner.lock().expect("cache mutex poisoned");
        match &*state {
            CacheState::Decoded { image } => return Ok(Arc::clone(image)),
            CacheState::Failed(e) => {
                // `FormatError` no es Clone: convertir a Corrupted con
                // el mensaje.
                return Err(clone_error(e));
            }
            CacheState::Empty => {
                // Decodificar a continuación.
            }
        }
        let result = self.decode_inner();
        match result {
            Ok(img) => {
                let arc = Arc::new(img);
                *state = CacheState::Decoded {
                    image: Arc::clone(&arc),
                };
                Ok(arc)
            }
            Err(e) => {
                *state = CacheState::Failed(e.clone_for_fail_state());
                Err(e)
            }
        }
    }

    fn decode_inner(&self) -> Result<image::DynamicImage, FormatError> {
        let img = image::open(&*self.source_path)?;
        let pixels = u64::from(img.width()) * u64::from(img.height());
        if pixels > MAX_QUALITY_EVAL_PIXELS {
            return Err(FormatError::Corrupted(format!(
                "original too large for quality evaluation ({} px; max {})",
                pixels, MAX_QUALITY_EVAL_PIXELS
            )));
        }
        Ok(img)
    }

    /// Devuelve la ruta fuente a la que está ligado el cache.
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// ¿La original ya está decodificada y cacheada? Para diagnóstico.
    pub fn is_decoded(&self) -> bool {
        matches!(
            &*self.inner.lock().expect("cache mutex poisoned"),
            CacheState::Decoded { .. }
        )
    }
}

/// `FormatError` no es `Clone`: para guardar un fallo en el estado del
/// cache se convierte a `Corrupted` con el mensaje. Se pierde la
/// variante original pero se conserva el diagnóstico.
fn clone_error(e: &FormatError) -> FormatError {
    FormatError::Corrupted(e.to_string())
}

// Trait de extensión para guardar el error en el estado del cache
// sin tocar `FormatError`.
trait FormatErrorExt {
    fn clone_for_fail_state(&self) -> FormatError;
}

impl FormatErrorExt for FormatError {
    fn clone_for_fail_state(&self) -> FormatError {
        clone_error(self)
    }
}
