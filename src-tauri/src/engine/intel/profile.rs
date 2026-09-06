//! FileProfile: el análisis normalizado de BoxFlux para un archivo de
//! entrada.
//!
//! Ni la UI ni el motor dependen de estructuras de librerías
//! externas: todo pasa por [`FileProfile`].

use crate::engine::formats::{Format, FormatCapabilities};
use serde::{Deserialize, Serialize};

/// Modelo de color detectado. Determina qué encoders aplican y qué
/// métricas de calidad tienen sentido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ColorModel {
    Rgb,
    Rgba,
    Gray,
    GrayAlpha,
    Indexed,
    Cmyk,
    Unknown,
}

impl ColorModel {
    pub fn has_alpha(&self) -> bool {
        matches!(self, ColorModel::Rgba | ColorModel::GrayAlpha)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ColorModel::Rgb => "RGB",
            ColorModel::Rgba => "RGBA",
            ColorModel::Gray => "Gray",
            ColorModel::GrayAlpha => "Gray+Alpha",
            ColorModel::Indexed => "Indexed",
            ColorModel::Cmyk => "CMYK",
            ColorModel::Unknown => "Unknown",
        }
    }
}

/// Clasificación gruesa del contenido. Dirige las decisiones de las
/// heurísticas (`crate::engine::intel::heuristics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ImageCategory {
    Photograph,
    Screenshot,
    GameTexture,
    Illustration,
    TransparentAsset,
    Icon,
    Gradient,
    TextHeavy,
    HighlyDetailed,
    Unknown,
}

impl ImageCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageCategory::Photograph => "Photograph",
            ImageCategory::Screenshot => "Screenshot",
            ImageCategory::GameTexture => "GameTexture",
            ImageCategory::Illustration => "Illustration",
            ImageCategory::TransparentAsset => "TransparentAsset",
            ImageCategory::Icon => "Icon",
            ImageCategory::Gradient => "Gradient",
            ImageCategory::TextHeavy => "TextHeavy",
            ImageCategory::HighlyDetailed => "HighlyDetailed",
            ImageCategory::Unknown => "Unknown",
        }
    }
}

/// Resultado normalizado del análisis de un archivo.
///
/// Lo produce [`crate::engine::intel::analyzer::Analyzer`]. Todos los
/// componentes del motor (strategy, generador de candidatos,
/// heurísticas) leen de aquí, nunca del output crudo del decoder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileProfile {
    /// Formato fuente, detectado por extensión y cabeceras.
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub file_size: u64,

    pub bit_depth: u8,
    pub color_model: ColorModel,
    pub has_alpha: bool,
    pub has_metadata: bool,
    pub metadata_has_gps: bool,

    pub is_animated: bool,
    pub frame_count: u32,

    /// 0.0..=1.0. Mayor = más detalle de alta frecuencia. Estimación
    /// por varianza de diferencias de píxeles en un preview reducido.
    pub complexity: f32,

    /// 0.0..=1.0. Mayor = se espera que comprima mejor. Inversa de la
    /// complejidad, modulada por factores del formato.
    pub estimated_compressibility: f32,

    /// Clasificación gruesa del contenido.
    pub category: ImageCategory,

    /// Capabilities del codec del formato fuente.
    pub capabilities: FormatCapabilities,

    /// Tags de características libres, p.ej. "flat_regions",
    /// "high_noise", "sharp_edges". Las usa el motor de heurísticas.
    pub characteristics: Vec<String>,

    /// Aspect ratio (ancho / alto). 0 si la altura es 0.
    pub aspect_ratio: f32,

    // Métricas numéricas del analizador: viajan a
    // strategy/candidate/iterative para decisiones adaptativas reales
    // (no thresholds booleanos). El uso principal está en
    // `intel/iterative.rs`: ajustar el target de butteraugli según la
    // "tolerancia a pérdida" que predice el analizador para esta
    // imagen concreta.
    /// Luma media (0..255). Las imágenes oscuras muestran más banding
    /// en JPEG → requieren quality más alto.
    #[serde(default)]
    pub mean_luma: f32,

    /// Desviación típica del luma (0..255). Contraste alto =
    /// transiciones afiladas que los códecs manejan distinto.
    #[serde(default)]
    pub contrast: f32,

    /// Saturación media (0..1). Saturación alta → 4:4:4 compensa.
    #[serde(default)]
    pub mean_saturation: f32,

    /// Fracción de píxeles con luma < 30. Alta = imagen oscura.
    #[serde(default)]
    pub dark_region_ratio: f32,

    /// Fracción de píxeles con luma > 225. Alta = imagen brillante.
    #[serde(default)]
    pub bright_region_ratio: f32,

    /// Fracción de pares con gradiente fuerte. Alta = texto/UI.
    #[serde(default)]
    pub edge_density: f32,

    /// Fracción en áreas planas. Alta = comprime muy bien lossless.
    #[serde(default)]
    pub flat_region_ratio: f32,

    /// Estimación de ruido (0..1). Alto = el lossy puede denoisear y
    /// bajar tamaño sin perder calidad percibida.
    #[serde(default)]
    pub noise_estimate: f32,

    /// SSIM del roundtrip WebP q75 sobre el preview ("sonda de
    /// sensibilidad lossy"). Predice la reacción del contenido a
    /// lossy: ≥ 0.955 = gráfico estable, < 0.955 = fotográfico con
    /// acantilado de calidad. None = el probe falló (el motor cae a
    /// la clasificación por categoría).
    ///
    /// Ver `analyzer::probe_lossy_sensitivity` para la metodología.
    #[serde(default)]
    pub lossy_probe_ssim: Option<f32>,

    /// Bits por píxel del output WebP q75 del preview (la MISMA
    /// codificación del probe de sensibilidad — coste marginal cero).
    /// Mejor predictor medido del coste de rav1e: el trabajo de un
    /// encoder con RDO por bloque escala con los bits que emite, no
    /// con la "dificultad perceptual".
    ///
    /// Lo consume `candidate::static_cost_ms` para hacer el coste
    /// estático AVIF consciente del contenido (función PURA: mismo
    /// píxel → mismo bpp → mismo coste).
    #[serde(default)]
    pub lossy_probe_bpp: Option<f32>,
}

impl FileProfile {
    /// Total de píxeles.
    pub fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Bytes por píxel de la imagen decodificada.
    pub fn bytes_per_pixel(&self) -> f32 {
        self.file_size as f32 / self.pixel_count().max(1) as f32
    }

    /// ¿Archivo tan pequeño que optimizar no compensa el tiempo de
    /// proceso?
    pub fn is_trivially_small(&self) -> bool {
        self.file_size < 4 * 1024
    }

    /// ¿Archivo muy grande que puede requerir streaming/resize?
    pub fn is_very_large(&self) -> bool {
        self.file_size > 50 * 1024 * 1024
    }

    /// ¿La imagen tiene transparencia que preservar?
    pub fn requires_alpha_preservation(&self) -> bool {
        self.has_alpha
    }

    /// ¿La categoría sugiere contenido fotográfico?
    ///
    /// Fuente única de verdad para el chroma subsampling JPEG
    /// adaptativo: las fotografías (y las desconocidas, que suelen ser
    /// fotos) comprimen imperceptiblemente con 4:2:0 (−10-15% de
    /// tamaño); screenshots/texto/ilustraciones necesitan 4:4:4 para
    /// evitar fringe de color en bordes saturados.
    ///
    /// La usan `JpegBackend::build_profile` y el iterative search
    /// (`intel::iterative::build_encode_profile`): una sola decisión
    /// para todas las rutas, sin contradicciones entre backend y
    /// búsqueda iterativa.
    pub fn is_photo_like(&self) -> bool {
        matches!(
            self.category,
            ImageCategory::Photograph | ImageCategory::HighlyDetailed | ImageCategory::Unknown
        )
    }

    /// Resumen legible para la UI.
    pub fn summary(&self) -> String {
        format!(
            "{} {}x{} {} {}bpp {} complexity={:.2} compressibility={:.2}",
            self.format,
            self.width,
            self.height,
            self.color_model.as_str(),
            self.bit_depth,
            self.category.as_str(),
            self.complexity,
            self.estimated_compressibility,
        )
    }
}
