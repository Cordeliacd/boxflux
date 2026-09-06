//! Perfiles de optimización, opciones de resize y tipos de resultado.
//!
//! Estos tipos cruzan la frontera Tauri ↔ React vía serde_json.

use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};

/// Perfil de optimización (built-in o definido por el usuario).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationProfile {
    pub name: String,
    pub kind: ProfileKind,

    /// Calidad JPEG 1-100.
    pub jpeg_quality: u8,
    pub jpeg_progressive: bool,

    /// WebP.
    pub webp_quality: u8,
    pub webp_lossless: bool,

    /// AVIF.
    pub avif_quality: u8,
    pub avif_alpha_quality: u8,

    /// Presupuesto de tiempo (ms) para el encode AVIF de este perfil.
    /// Lo fija el pipeline desde `Candidate::backend_time_budget_ms`;
    /// 0 = sin presupuesto (decisión interna por calidad, ver
    /// `AvifHandler::recommended_speed`).
    ///
    /// rav1e speed 4 es 16-21% más eficiente que speed 5/6 pero ~1.3×
    /// más lento: con presupuesto generoso usamos 4, con presupuesto
    /// corto aceleramos en lugar de agotar el tiempo del pipeline.
    #[serde(default)]
    pub avif_time_budget_ms: u64,

    /// Nivel de optimización PNG oxipng (0-6).
    pub png_optimization_level: u8,

    /// Manejo de metadata.
    pub metadata_mode: MetadataMode,

    /// Si es `true`, preserva el perfil de color ICC del original en la
    /// salida (Display P3, sRGB). Evita el oscurecimiento / desplazamiento
    /// de tonos negros y sombras que se produce al eliminar el perfil y
    /// asumir sRGB. Por defecto `true`.
    ///
    /// Para formatos que no soportan ICC nativo (WebP via `webp` crate,
    /// AVIF via `ravif`), el motor aplica chroma subsampling 4:4:4 y
    /// cuantización adaptativa para mitigar la pérdida de fidelidad
    /// cromática, pero no puede preservar el perfil ICC pixel-exact.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,

    /// Subsampling de croma para el re-encode JPEG.
    ///
    /// `true` = 4:4:4 (sin subsampling — mejor para gráficos, texto y
    /// screenshots con bordes saturados). `false` = 4:2:0 (descarta el
    /// 75% de la información de croma — 10-15% más pequeño, prácticamente
    /// imperceptible en fotografía).
    ///
    /// ICC y subsampling son decisiones independientes. El backend
    /// inteligente decide según la categoría de la imagen: fotografías
    /// → 4:2:0; screenshots/UI/ilustraciones → 4:4:4.
    #[serde(default = "default_jpeg_chroma_444")]
    pub jpeg_chroma_444: bool,

    /// Resize opcional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize: Option<ResizeOptions>,
}

fn default_preserve_color_profile() -> bool {
    true
}

fn default_jpeg_chroma_444() -> bool {
    true
}

impl Default for OptimizationProfile {
    fn default() -> Self {
        Self::preset_web()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ProfileKind {
    Web,
    Lossless,
    Custom,
}

impl ProfileKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfileKind::Web => "Web",
            ProfileKind::Lossless => "Lossless",
            ProfileKind::Custom => "Custom",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "Web" | "web" => ProfileKind::Web,
            "Lossless" | "lossless" => ProfileKind::Lossless,
            "Custom" | "custom" => ProfileKind::Custom,
            _ => ProfileKind::Custom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum MetadataMode {
    Keep,
    RemoveSafe,
    RemoveAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ResizeMode {
    /// Preserva aspect ratio; escala sin exceder el target.
    Fit,
    /// Preserva aspect ratio; crop/fill al target exacto.
    Fill,
    /// Ignora aspect ratio; estira al target.
    Stretch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ResizeOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    #[serde(default = "default_resize_mode")]
    pub mode: ResizeMode,
}

fn default_resize_mode() -> ResizeMode {
    ResizeMode::Fit
}

impl Default for ResizeOptions {
    fn default() -> Self {
        Self {
            target_width: None,
            target_height: None,
            percentage: None,
            max_width: None,
            max_height: None,
            mode: ResizeMode::Fit,
        }
    }
}

/// Resultado de una operación de optimización.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub original_size: u64,
    pub output_size: u64,
    pub bytes_saved: u64,
    pub percentage_saved: f64,
    pub processing_time_ms: u64,
    pub format: crate::engine::formats::Format,
    pub success: bool,
    #[serde(default)]
    pub error: String,
}

impl OptimizationProfile {
    pub fn preset_web() -> Self {
        Self {
            name: "Web".into(),
            kind: ProfileKind::Web,
            // JPEG quality 85 (vs 82 antes): el usuario reportó "negros
            // más profundos" en fotos optimizadas. La combinación de
            // quality=82 + chroma subsampling 4:2:0 era demasiado agresiva
            // para fotografía. Subimos a 85 + 4:4:4 (sin subsampling) para
            // preservar mejor las sombras.
            jpeg_quality: 85,
            jpeg_progressive: true,
            // WebP quality 82 (vs 80): mismo motivo que JPEG.
            webp_quality: 82,
            webp_lossless: false,
            avif_quality: 60,
            avif_alpha_quality: 80,
            avif_time_budget_ms: 0,
            // PNG level 4 (vs 3 antes): libdeflater-12 + full filter eval.
            // Es el nivel máximo razonable sin activar zopfli (que tarda
            // 10-100× más). Para la mayoría de PNGs web (screenshots,
            // logos) esto da 20-35% de reducción sobre el original.
            png_optimization_level: 4,
            metadata_mode: MetadataMode::RemoveSafe,
            // Preserva perfiles ICC por defecto — los usuarios de web
            // quieren colores consistentes entre dispositivos.
            preserve_color_profile: true,
            jpeg_chroma_444: true, // Web: gráficos y fotos mixtas — 4:4:4 por defecto,
            resize: None,
        }
    }

    pub fn preset_lossless() -> Self {
        Self {
            name: "Lossless".into(),
            kind: ProfileKind::Lossless,
            jpeg_quality: 100,
            jpeg_progressive: false,
            webp_quality: 100,
            webp_lossless: true,
            avif_quality: 100,
            avif_alpha_quality: 100,
            avif_time_budget_ms: 0,
            // PNG level 5 (vs 4 antes): añade filtros Up, MinSum y
            // evaluación completa. Sigue usando libdeflater (no zopfli)
            // para mantener el modo "lossless" razonablemente rápido.
            png_optimization_level: 5,
            metadata_mode: MetadataMode::Keep,
            preserve_color_profile: true,
            jpeg_chroma_444: true, // Lossless: sin degradación de croma,
            resize: None,
        }
    }

    pub fn preset_custom() -> Self {
        Self {
            name: "Custom".into(),
            kind: ProfileKind::Custom,
            ..Self::preset_web()
        }
    }

    /// Lista de presets built-in. Útil para inicializar el ProfileManager.
    ///
    /// Sin presets Quality/MaximumCompression: la optimización pasa
    /// por el pipeline con GOALS (`OptimizationGoal::Quality` /
    /// `OptimizationGoal::MaximumCompression`).
    pub fn all_presets() -> Vec<Self> {
        vec![Self::preset_web(), Self::preset_lossless(), Self::preset_custom()]
    }
}

/// Calcula las dimensiones target dadas las opciones de resize.
/// Devuelve `None` si no se requiere resize.
pub fn compute_target_size(
    current_width: u32,
    current_height: u32,
    opts: &ResizeOptions,
) -> Option<(u32, u32)> {
    let mut target_w = opts.target_width;
    let mut target_h = opts.target_height;

    if let Some(p) = opts.percentage {
        let p = p.max(0.0);
        let w = ((current_width as f32) * p / 100.0).round() as u32;
        let h = ((current_height as f32) * p / 100.0).round() as u32;
        target_w = Some(w.max(1));
        target_h = Some(h.max(1));
    }

    if let Some(mw) = opts.max_width {
        target_w = Some(target_w.map(|w| w.min(mw)).unwrap_or(mw));
    }
    if let Some(mh) = opts.max_height {
        target_h = Some(target_h.map(|h| h.min(mh)).unwrap_or(mh));
    }

    let (tw, th) = match (target_w, target_h) {
        (None, None) => return None,
        (Some(w), None) => {
            let h = (current_height as u64 * w as u64 / current_width.max(1) as u64) as u32;
            (w, h.max(1))
        }
        (None, Some(h)) => {
            let w = (current_width as u64 * h as u64 / current_height.max(1) as u64) as u32;
            (w.max(1), h)
        }
        (Some(w), Some(h)) => match opts.mode {
            ResizeMode::Stretch => (w, h),
            ResizeMode::Fit => {
                let ar_in = current_width as f64 / current_height.max(1) as f64;
                let ar_out = w as f64 / h as f64;
                if ar_in > ar_out {
                    let new_h = (w as f64 / ar_in).round() as u32;
                    (w, new_h.max(1))
                } else {
                    let new_w = (h as f64 * ar_in).round() as u32;
                    (new_w.max(1), h)
                }
            }
            ResizeMode::Fill => (w, h),
        },
    };

    Some((tw, th))
}

/// Aplica un resize a una DynamicImage usando el filtro configurado.
pub fn apply_resize(
    img: DynamicImage,
    opts: Option<&ResizeOptions>,
) -> Result<DynamicImage, crate::engine::formats::FormatError> {
    let Some(opts) = opts else {
        return Ok(img);
    };
    let (w, h) = img.dimensions();
    let Some((tw, th)) = compute_target_size(w, h, opts) else {
        return Ok(img);
    };
    use image::imageops::FilterType;
    let filter = match opts.mode {
        ResizeMode::Stretch => FilterType::Nearest, // distorsión explícita
        _ => FilterType::Lanczos3,
    };
    let mut out = img.resize_exact(tw, th, filter);
    if opts.mode == ResizeMode::Fill {
        let (cw, ch) = out.dimensions();
        let x = cw.saturating_sub(tw) / 2;
        let y = ch.saturating_sub(th) / 2;
        out = out.crop_imm(x, y, tw.min(cw), th.min(ch));
    }
    Ok(out)
}

/// Resize de un RgbImage (usado por el handler JPEG antes de encode).
pub fn apply_resize_rgb(
    rgb: &image::RgbImage,
    opts: &ResizeOptions,
) -> Result<image::RgbImage, crate::engine::formats::FormatError> {
    let img = DynamicImage::ImageRgb8(rgb.clone());
    let resized = apply_resize(img, Some(opts))?;
    Ok(resized.to_rgb8())
}
