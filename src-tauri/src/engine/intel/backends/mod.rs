//! Adaptadores de backend: wrappers finos sobre los handlers de
//! `formats::*` que implementan el trait [`super::FormatBackend`].
//!
//! Cero lógica de codec aquí. Solo:
//! 1. Traducir un [`super::Candidate`] a un
//!    [`crate::engine::optimization::OptimizationProfile`].
//! 2. Delegar en el handler existente.
//! 3. Medir tiempos y tamaño de archivo.
//! 4. Devolver un [`super::BackendResult`].
//!
//! Si una librería de codec se reemplaza, solo cambia su adaptador
//! aquí — el motor no se toca.

pub mod avif;
pub mod jpeg;
pub mod other;
pub mod png;
pub mod webp;

pub use avif::AvifBackend;
pub use jpeg::JpegBackend;
pub use other::{BmpBackend, GifBackend, TiffBackend};
pub use png::PngBackend;
pub use webp::WebpBackend;

use std::collections::HashMap;
use std::sync::Arc;

use crate::engine::formats::Format;

use super::backend::FormatBackend;

/// Registro de backends por formato. El pipeline lo usa para
/// localizar el backend de cada candidato.
#[derive(Clone)]
pub struct BackendRegistry {
    backends: HashMap<Format, Arc<dyn FormatBackend>>,
}

impl BackendRegistry {
    /// Construye el registro por defecto con los adaptadores de
    /// codecs reales.
    pub fn with_real_codecs() -> Self {
        let mut map: HashMap<Format, Arc<dyn FormatBackend>> = HashMap::new();
        map.insert(Format::Png, Arc::new(PngBackend::new()));
        map.insert(Format::Jpeg, Arc::new(JpegBackend::new()));
        map.insert(Format::Webp, Arc::new(WebpBackend::new()));
        map.insert(Format::Avif, Arc::new(AvifBackend::new()));
        map.insert(Format::Gif, Arc::new(GifBackend::new()));
        map.insert(Format::Bmp, Arc::new(BmpBackend::new()));
        map.insert(Format::Tiff, Arc::new(TiffBackend::new()));
        Self { backends: map }
    }

    pub fn get(&self, format: Format) -> Option<Arc<dyn FormatBackend>> {
        self.backends.get(&format).cloned()
    }

    pub fn list(&self) -> Vec<(Format, String)> {
        let mut out: Vec<(Format, String)> = self
            .backends
            .iter()
            .map(|(f, b)| (*f, b.name().to_string()))
            .collect();
        out.sort_by_key(|(f, _)| *f as u8);
        out
    }

}
