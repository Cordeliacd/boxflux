//! OptimizationParameter: la abstracción del espacio de búsqueda.
//!
//! Cada parámetro es una dimensión que el motor puede variar en un
//! candidato. El generador de candidatos recorre combinaciones de
//! estos parámetros, pero nunca por fuerza bruta: consulta los rangos
//! de calidad de la estrategia y las heurísticas para podar.

use crate::engine::formats::Format;
use crate::engine::optimization::{MetadataMode, ResizeOptions};

/// Una dimensión del espacio de búsqueda de optimización.
#[derive(Debug, Clone)]
pub enum OptimizationParameter {
    /// Formato de salida.
    Format(Format),
    /// Calidad del encoder (1..=100). None = lossless.
    Quality(Option<u8>),
    /// Preset de velocidad del encoder (0..=10, específico del codec).
    Speed(u8),
    /// Especificación de resize.
    Resize(ResizeOptions),
    /// Modo de manejo de metadata.
    Metadata(MetadataMode),
    /// Chroma subsampling (p.ej. 4:2:0). Específico de JPEG.
    ChromaSubsampling(ChromaSubsampling),
    /// Calidad del alpha por separado (AVIF/WebP).
    AlphaQuality(u8),
    /// Si el candidato es lossless de extremo a extremo.
    Lossless(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromaSubsampling {
    C444,
    C422,
    C420,
}

impl ChromaSubsampling {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChromaSubsampling::C444 => "4:4:4",
            ChromaSubsampling::C422 => "4:2:2",
            ChromaSubsampling::C420 => "4:2:0",
        }
    }
}

impl OptimizationParameter {
    /// Etiqueta legible para la UI.
    pub fn label(&self) -> String {
        match self {
            OptimizationParameter::Format(f) => format!("format={}", f.as_str()),
            OptimizationParameter::Quality(Some(q)) => format!("quality={}", q),
            OptimizationParameter::Quality(None) => "quality=lossless".into(),
            OptimizationParameter::Speed(s) => format!("speed={}", s),
            OptimizationParameter::Resize(r) => {
                if let Some(p) = r.percentage {
                    if p > 0.0 {
                        format!("resize={}%", p)
                    } else {
                        "resize=none".into()
                    }
                } else if let (Some(w), Some(h)) = (r.target_width, r.target_height) {
                    format!("resize={}x{}", w, h)
                } else if let Some(mw) = r.max_width {
                    format!("resize=max_width={}", mw)
                } else if let Some(mh) = r.max_height {
                    format!("resize=max_height={}", mh)
                } else {
                    "resize=none".into()
                }
            }
            OptimizationParameter::Metadata(m) => format!("metadata={:?}", m),
            OptimizationParameter::ChromaSubsampling(c) => format!("chroma={}", c.as_str()),
            OptimizationParameter::AlphaQuality(q) => format!("alpha_quality={}", q),
            OptimizationParameter::Lossless(true) => "lossless=true".into(),
            OptimizationParameter::Lossless(false) => "lossless=false".into(),
        }
    }
}
