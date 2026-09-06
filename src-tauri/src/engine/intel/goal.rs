//! Goals de optimización y los pesos que imponen al scoring.
//!
//! Cada goal produce unos [`GoalWeights`] distintos que consultan el
//! `StrategyEngine` y el `CandidateScoringEngine` al planificar
//! candidatos y ordenar resultados. No son etiquetas de UI: cambian el
//! comportamiento del motor.

use crate::engine::formats::Format;
use serde::{Deserialize, Serialize};

/// Goal de optimización de primer nivel. Mapea 1:1 con la selección de
/// la UI y lo consume el motor de estrategia para dirigir la
/// generación de candidatos y el scoring.
///
/// El motor expone dos modos de usuario; el pipeline adaptativo ya
/// busca el óptimo por sí mismo y más modos solo fragmentaban la
/// inteligencia:
///
/// * **Balanceado** (default) → [`OptimizationGoal::Lossless`]:
///   comprimir al máximo SIN perder nada de calidad. Solo códecs
///   lossless (PNG oxipng+zopfli, WebP lossless, transcode DCT de
///   JPEG fuente).
///
/// * **Comprimir al Máximo** → [`OptimizationGoal::MaximumCompression`]:
///   compresión extrema con el MENOR daño perceptual posible: gate
///   butteraugli real, iterative search en JPEG/WebP/AVIF y escaleras
///   con décadas bajas.
///
/// Los demás goals (Quality/Balanced/Web/ExtremeLightweight) quedan
/// deprecados: siguen funcionando por compatibilidad con configs
/// guardadas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum OptimizationGoal {
    /// Modo "Balanceado" (default): comprimir al máximo SIN perder nada
    /// de calidad. Lossless estricto: PNG (oxipng+zopfli), WebP lossless
    /// y transcode DCT de JPEG fuente. Ninguna transformación lossy.
    Lossless,

    /// Deprecado. Usar `Lossless` (moderado) o `MaximumCompression`
    /// (extremo con gate perceptual).
    Quality,

    /// Deprecado. El "punto medio" lossy lo cubre `MaximumCompression`
    /// con su gate perceptual butteraugli (encuentra el q exacto de
    /// daño tolerable).
    Balanced,

    /// Modo "Comprimir al Máximo": compresión extrema con el menor
    /// daño perceptual posible. Gate perceptual (butteraugli medido
    /// real, no SSIM genérico), iterative search en JPEG/WebP/AVIF
    /// (escalera AVIF hasta q20), calidad medida con dav1d. Si un
    /// lossless resultara más pequeño, gana (el scorer siempre
    /// prefiere calidad gratis).
    MaximumCompression,

    /// Deprecado. La compatibilidad web es una restricción ortogonal,
    /// no un modo (los constraints de formato siguen disponibles).
    Web,

    /// Deprecado. Su semántica (lo más ligero sin perder calidad
    /// visual) vive en el gate perceptual de `MaximumCompression`.
    ExtremeLightweight,

    /// Pesos definidos por el usuario: control fino.
    Custom(GoalWeights),
}

impl OptimizationGoal {
    /// Resuelve a pesos concretos. Lo usan el motor de estrategia y el
    /// de scoring.
    ///
    /// Los pesos están calibrados para que cada goal produzca
    /// resultados distintos:
    /// - `Lossless` solo permite candidatos sin pérdida.
    /// - `Quality` prefiere lossless si está cerca del tamaño lossy;
    ///   si no, elige el lossy con mejor PSNR/SSIM.
    /// - `Balanced` busca el mejor trade-off calidad/compresión.
    /// - `MaximumCompression` prefiere candidatos muy pequeños (AVIF,
    ///   WebP q60) aceptando calidad menor.
    /// - `Web` prefiere WebP/JPEG por compatibilidad con navegadores.
    pub fn weights(&self) -> GoalWeights {
        match self {
            OptimizationGoal::Lossless => GoalWeights {
                quality_weight: 1.0,
                compression_weight: 0.2,
                speed_weight: 0.2,
                compatibility_weight: 0.5,
                min_quality_threshold: 1.0, // 100% — tiene que ser lossless
                allow_lossy: false,
                preferred_formats: vec![Format::Png, Format::Webp],
                max_candidates: 4,
                max_processing_time_ms: 30_000,
            },
            // Quality: alta exigencia de calidad. Lossless gana si está
            // dentro del 30% del tamaño del mejor lossy. Si no, se queda
            // con el lossy de mayor PSNR/SSIM.
            OptimizationGoal::Quality => GoalWeights {
                quality_weight: 0.85,
                compression_weight: 0.30,
                speed_weight: 0.20,
                compatibility_weight: 0.40,
                min_quality_threshold: 0.92, // SSIM >= 0.92 equivale a PSNR >= ~40dB
                allow_lossy: true,
                preferred_formats: vec![Format::Webp, Format::Avif, Format::Png],
                max_candidates: 6,
                max_processing_time_ms: 60_000,
            },
            // Balanced: pesos equilibrados; el scorer normaliza bien y
            // premia la compresión real. max_candidates=10 cubre los
            // rangos con step fino (3 niveles por formato × 3 formatos
            // + 1 lossless).
            OptimizationGoal::Balanced => GoalWeights {
                quality_weight: 0.50,
                compression_weight: 0.70,
                speed_weight: 0.40,
                compatibility_weight: 0.40,
                min_quality_threshold: 0.80, // SSIM >= 0.80
                allow_lossy: true,
                preferred_formats: vec![Format::Webp, Format::Avif, Format::Png, Format::Jpeg],
                max_candidates: 10,
                max_processing_time_ms: 60_000,
            },
            // MaximumCompression: pesos extremos (compression >> quality)
            // para que un AVIF q45 con 95% de ahorro le gane a un WebP
            // q80 con 85% aunque el WebP tenga mejor calidad. El
            // threshold 0.75 es el más permisivo de la paleta sin
            // cruzar la línea de "imagen visiblemente degradada"
            // (un SSIM de 0.60 sí la cruza: banding, bordes
            // plastificados). max_candidates=12 cubre los 4 niveles de
            // calidad AVIF + WebP + JPEG.
            OptimizationGoal::MaximumCompression => GoalWeights {
                quality_weight: 0.10,
                compression_weight: 1.0,
                speed_weight: 0.20,
                compatibility_weight: 0.30,
                min_quality_threshold: 0.75, // SSIM >= 0.75: menos se ve degradado
                allow_lossy: true,
                preferred_formats: vec![Format::Avif, Format::Webp, Format::Jpeg],
                max_candidates: 12,
                max_processing_time_ms: 120_000,
            },
            // Web: optimizado para carga web. JPEG/WebP ligeros y
            // compatibles con todos los navegadores modernos.
            OptimizationGoal::Web => GoalWeights {
                quality_weight: 0.40,
                compression_weight: 0.85,
                speed_weight: 0.70,
                compatibility_weight: 0.95, // los navegadores prefieren WebP/JPEG
                min_quality_threshold: 0.75,
                allow_lossy: true,
                preferred_formats: vec![Format::Webp, Format::Jpeg, Format::Png],
                max_candidates: 5,
                max_processing_time_ms: 30_000,
            },
            // ExtremeLightweight: pesos que priorizan tamaño con un
            // threshold de calidad PERCEPTUAL alto. La magia no está
            // aquí en los pesos — está en `intel/iterative.rs`, donde
            // la búsqueda guiada por butteraugli encuentra el quality
            // óptimo por imagen.
            //
            // Estos pesos solo entran en juego para los candidatos que
            // NO pasan por el iterative search (PNG lossless). Para
            // JPEG/WebP lossy, el iterative search produce un único
            // candidato óptimo por formato y el scorer lo compara con
            // los demás.
            OptimizationGoal::ExtremeLightweight => GoalWeights {
                quality_weight: 0.40, // la calidad importa, pero el gate real es butteraugli
                compression_weight: 1.0,
                speed_weight: 0.10, // aceptamos lentitud a cambio de menos bytes
                compatibility_weight: 0.20,
                min_quality_threshold: 0.90, // suelo perceptual (equivalente SSIM)
                allow_lossy: true,
                preferred_formats: vec![Format::Webp, Format::Avif, Format::Jpeg, Format::Png],
                max_candidates: 16, // hueco para las variantes del iterative search
                max_processing_time_ms: 180_000, // 3 min: el iterative search tarda más
            },
            OptimizationGoal::Custom(w) => w.clone(),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            OptimizationGoal::Lossless => "Lossless",
            OptimizationGoal::Quality => "Quality",
            OptimizationGoal::Balanced => "Balanced",
            OptimizationGoal::MaximumCompression => "MaximumCompression",
            OptimizationGoal::Web => "Web",
            OptimizationGoal::ExtremeLightweight => "ExtremeLightweight",
            OptimizationGoal::Custom(_) => "Custom",
        }
    }

    /// Target de butteraugli para este goal; relevante solo para el
    /// iterative search (JPEG/WebP lossy).
    ///
    /// - < 1.0 = visualmente idéntico (estándar JXL/Squoosh)
    /// - 1.0..=2.0 = diferencia apenas perceptible
    /// - > 2.0 = visiblemente diferente
    pub fn butteraugli_target(&self) -> f64 {
        match self {
            OptimizationGoal::Lossless => 0.0,           // sin lossy
            OptimizationGoal::Quality => 0.75,           // más estricto que "visualmente idéntico"
            OptimizationGoal::ExtremeLightweight => 1.0, // visualmente idéntico (estándar JXL de Google)
            OptimizationGoal::Balanced => 1.5,
            OptimizationGoal::Web => 1.5,
            // 3.0 en cap-1024 ≈ 1.5 en full-res: "diferencia visible,
            // calidad aún claramente útil" — el territorio que pidió el
            // usuario para el modo extremo.
            //
            // La métrica del engine es butteraugli cap-1024, que
            // magnifica ~2× el daño respecto del butteraugli full-res
            // con el que se calibró la escala estándar: un q95 en fotos
            // de textura da 0.8-1.1 en cap-1024 donde el full-res
            // daría ~0.4-0.5. Aplicar el target estándar (1.5) con la
            // métrica cap-1024 rechazaría AVIF q20-q40 con SSIM 0.85+,
            // tirando el 70%+ de ahorro que promete el modo extremo.
            //
            // El guardarrail SSIM 0.55 del scorer sigue protegiendo
            // contra lo catastrófico.
            OptimizationGoal::MaximumCompression => 3.0,
            OptimizationGoal::Custom(_) => 1.5,
        }
    }

    /// ¿Este goal dispara la búsqueda iterativa guiada por butteraugli
    /// para JPEG/WebP lossy?
    pub fn uses_iterative_search(&self) -> bool {
        matches!(
            self,
            // MaximumCompression dispara el iterative search: es su
            // herramienta central — encuentra el q exacto de daño
            // perceptual tolerable por imagen, en JPEG/WebP
            // y AVIF.
            OptimizationGoal::ExtremeLightweight
                | OptimizationGoal::Quality
                | OptimizationGoal::MaximumCompression
        )
    }

    /// ¿El gate de calidad de este goal es PERCEPTUAL (butteraugli
    /// medido real ≤ [`Self::butteraugli_target`]) en vez del
    /// threshold SSIM genérico?
    ///
    /// Solo el modo "Comprimir al Máximo" (MaximumCompression): el daño
    /// se mide con butteraugli (la misma métrica que JPEG XL usa para
    /// "visually lossless"), no con SSIM (que ni ve banding en sombras
    /// ni aprueba el denoising legítimo). El objetivo del modo es
    /// "compresión extrema con el menor daño posible" y SSIM es un
    /// mal proxy para eso.
    ///
    /// Con el gate activo, el pipeline evalúa butteraugli SIEMPRE (no
    /// solo en la zona gris de SSIM) y el scorer acepta candidatos
    /// por daño perceptual real.
    pub fn uses_perceptual_gate(&self) -> bool {
        matches!(self, OptimizationGoal::MaximumCompression)
    }
}

/// Pesos concretos que usa el motor de scoring.
///
/// `quality_weight + compression_weight + speed_weight` no tienen por
/// qué sumar 1.0 — el scorer normaliza internamente. La compatibilidad
/// es un desempate, no parte del score principal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalWeights {
    /// 0.0..=1.0. Importancia de preservar la calidad de imagen.
    pub quality_weight: f32,
    /// 0.0..=1.0. Importancia de minimizar el tamaño de salida.
    pub compression_weight: f32,
    /// 0.0..=1.0. Importancia del procesado rápido.
    pub speed_weight: f32,
    /// 0.0..=1.0. Importancia de la compatibilidad amplia de formatos.
    pub compatibility_weight: f32,

    /// 0.0..=1.0. Calidad mínima aceptable; los candidatos por debajo
    /// de este umbral se rechazan.
    pub min_quality_threshold: f32,

    /// Si se permiten transformaciones lossy.
    pub allow_lossy: bool,

    /// Formatos preferentes al generar candidatos. El orden es el de
    /// preferencia.
    pub preferred_formats: Vec<Format>,

    /// Tope duro de candidatos por archivo: evita búsquedas desbocadas.
    pub max_candidates: usize,

    /// Tope duro de tiempo de proceso por archivo (ms).
    pub max_processing_time_ms: u64,
}

/// Restricciones del usuario que acotan aún más la estrategia.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserConstraints {
    /// Si está fijado, solo se prueban candidatos cuya salida quede en
    /// estos bytes como máximo. 0 = sin límite.
    pub max_output_size: Option<u64>,

    /// Si está fijado, fuerza el formato de salida (salta la búsqueda de formato).
    ///
    /// Modo **Manual**: cuando el usuario quiere un formato específico
    /// (PNG, JPEG, WebP, AVIF), el motor restringe los candidatos a ese
    /// formato y no prueba conversiones cruzadas.
    ///
    /// Modo **Auto** (None): el motor evalúa todos los formatos
    /// disponibles y selecciona el de menor peso en bytes según el goal.
    pub force_format: Option<Format>,

    /// Si está activado, fuerza salida lossless sin importar el goal.
    pub force_lossless: bool,

    /// Si están fijados, escala la imagen para caber en estas dimensiones.
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,

    /// Si está activado, elimina toda la metadata sin importar el perfil.
    pub strip_all_metadata: bool,

    /// Si está fijado, tope duro de tiempo de proceso (pisa al del goal).
    pub max_processing_time_ms: Option<u64>,

    /// Si es `true`, el motor preserva los perfiles de color ICC del
    /// archivo original (Display P3, sRGB) en la salida. Evita el
    /// oscurecimiento / desplazamiento de tonos negros y sombras que se
    /// produce cuando se elimina el perfil y se asume sRGB.
    ///
    /// Si es `false`, el motor descarta los perfiles ICC y aplica
    /// reducción de paleta / chroma subsampling agresivo para lograr el
    /// menor tamaño posible (modo compresión máxima).
    ///
    /// Por defecto `true` — el usuario espera ver los colores exactos
    /// del original, especialmente en fotografía.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,

    /// Presupuesto de memoria en MB para esta optimización. 0 = sin
    /// límite. Proviene de `AppSettings.max_memory_mb`.
    ///
    /// Los backends consultan este valor para decidir si usar presets
    /// agresivos (zopfli, AVIF speed=4) o conservadores. Por ejemplo,
    /// si el usuario pidió 512MB, el PNG backend baja del level 6
    /// (zopfli) al level 5 (libdeflater) porque zopfli en imágenes
    /// grandes puede consumir varios cientos de MB.
    #[serde(default)]
    pub memory_budget_mb: u64,
}

fn default_preserve_color_profile() -> bool {
    true
}
