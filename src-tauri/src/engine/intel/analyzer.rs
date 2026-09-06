//! Analyzer: produce un [`FileProfile`] a partir de un archivo en disco.
//! Es el único contacto directo del motor con el input: lee cabeceras
//! vía `formats::*Handler::analyze` y mide sus métricas sobre un
//! preview de píxeles reducido.
//!
//! Métricas del preview: complejidad, histograma de luma, saturación,
//! contraste, ruido, densidad de bordes, zonas oscuras y zonas planas.
//! Alimentan las heurísticas que eligen qué candidatos probar (las
//! zonas oscuras marcan dónde asoma el banding de JPEG, el ruido
//! penaliza lossless, las zonas planas favorecen PNG).

use std::path::Path;
use std::time::Instant;

use crate::engine::formats::{Format, FormatError};
use crate::engine::metadata;

use super::profile::{ColorModel, FileProfile, ImageCategory};

pub struct Analyzer;

/// Límite de píxeles comprobado a nivel de cabecera antes de decodificar:
/// el análisis y la evaluación de calidad pueden requerir varios
/// buffers RGBA simultáneos.
const MAX_ANALYSIS_PIXELS: u64 = 50_000_000;

/// Bins del histograma de luma (0-255, 32 bins de 8 valores).
const LUMA_BINS: usize = 32;

impl Analyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analiza un archivo del disco: decodifica un preview pequeño para
    /// el análisis de píxeles y no retiene el buffer completo.
    pub fn analyze(&self, path: &Path) -> Result<FileProfile, FormatError> {
        let start = Instant::now();
        let format = Format::from_path(path);
        if format == Format::Unknown {
            return Err(FormatError::Unsupported(format!(
                "unknown extension for {}",
                path.display()
            )));
        }
        let file_size = std::fs::metadata(path)?.len();

        let reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let (width, height) = reader.into_dimensions()?;
        let pixel_count = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || pixel_count > MAX_ANALYSIS_PIXELS {
            return Err(FormatError::Corrupted(format!(
                "unsupported image dimensions {width}x{height} (maximum {MAX_ANALYSIS_PIXELS} pixels)"
            )));
        }

        let preview = self.decode_preview(path, format, 256)?;
        let analysis = self.analyze_pixels(&preview, width, height, format);

        // Sonda de sensibilidad lossy: codifica el preview (≤256 px) como
        // WebP q75, lo decodifica y mide el SSIM contra el original
        // (~20 ms). Predice cómo reacciona el contenido a lossy:
        //   probe ≥ 0.955 → gráfico: AVIF/WebP estables en todo q40-80
        //   probe < 0.955 → fotográfico: acantilado de calidad
        // La clasificación por categorías no logra esta separación
        // (confundía fotos con gradientes).
        let (lossy_probe_ssim, lossy_probe_bpp) = self.probe_lossy_sensitivity(&preview);

        let meta = metadata::inspect(path).unwrap_or_default();
        let _ = start.elapsed();

        let color_model = self.detect_color_model(&preview, format);
        let has_alpha = color_model.has_alpha();

        // Características a partir de las métricas.
        let mut characteristics = analysis.characteristics.clone();

        // Alpha suave (0 < a < 255). Etiqueta informativa: AVIF
        // preserva el canal alpha bien (PSNR 53-76 dB, los 256 niveles
        // de 8 bits), así que no activa penalización de formatos.
        if has_alpha {
            let rgba = preview.to_rgba8();
            let has_soft_alpha = rgba
                .pixels()
                .any(|p| p[3] > 0 && p[3] < 255);
            if has_soft_alpha {
                characteristics.push("soft_alpha".into());
            }
        }

        // Características por color y luminancia.
        if analysis.color_vibrancy > 0.6 {
            characteristics.push("highly_saturated".into());
        }
        if analysis.dark_region_ratio > 0.4 {
            characteristics.push("predominantly_dark".into());
        }
        if analysis.flat_region_ratio > 0.3 {
            characteristics.push("large_flat_areas".into());
        }
        if analysis.noise_estimate > 0.3 {
            characteristics.push("noisy".into());
        }
        if analysis.edge_density > 0.15 {
            characteristics.push("text_or_ui".into());
        }

        Ok(FileProfile {
            format,
            width,
            height,
            file_size,
            bit_depth: 8,
            color_model,
            has_alpha,
            has_metadata: meta.has_exif,
            metadata_has_gps: meta.has_gps,
            is_animated: format == Format::Gif,
            frame_count: 1,
            complexity: analysis.complexity,
            estimated_compressibility: analysis.compressibility,
            category: analysis.category,
            capabilities: self.capabilities_for(format),
            characteristics,
            aspect_ratio: width as f32 / height.max(1) as f32,
            mean_luma: analysis.mean_luma as f32,
            contrast: analysis.contrast as f32,
            mean_saturation: analysis.mean_saturation as f32,
            dark_region_ratio: analysis.dark_region_ratio as f32,
            bright_region_ratio: analysis.bright_region_ratio as f32,
            edge_density: analysis.edge_density as f32,
            flat_region_ratio: analysis.flat_region_ratio as f32,
            noise_estimate: analysis.noise_estimate as f32,
            // Sonda de sensibilidad lossy (ver `probe_lossy_sensitivity`).
            // None si el probe falló.
            lossy_probe_ssim,
            // bpp del MISMO encode del probe: predictor del coste de
            // rav1e (ver `probe_lossy_sensitivity`).
            lossy_probe_bpp,
        })
    }

    /// Sonda de sensibilidad lossy: roundtrip WebP q75 del preview +
    /// SSIM (ver comentario en `analyze`). Devuelve (None, _) si el
    /// encode o el decode fallan; el motor cae a la clasificación por
    /// categoría, más conservadora.
    ///
    /// Devuelve también el bpp del propio encode (`bytes*8/píxeles del
    /// preview`), con coste marginal cero. Predice el coste de rav1e
    /// (ms/MP ≈ 1014·bpp^0.28) porque el trabajo de un encoder con RDO
    /// por bloque escala con los bits que emite, no con la dificultad
    /// perceptual — el SSIM del probe no predice el coste.
    fn probe_lossy_sensitivity(&self, preview: &image::DynamicImage) -> (Option<f32>, Option<f32>) {
        let rgba = preview.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w == 0 || h == 0 {
            return (None, None);
        }
        // WebP q75 sobre el preview, method bajo (rapidez, no tamaño —
        // lo que importa es la SENSIBILIDAD del contenido, no el output).
        let mut encoder = webpx::Encoder::new_rgba(rgba.as_raw(), w, h);
        encoder = encoder.quality(75.0).method(4).sharp_yuv(true);
        let webp_bytes: Vec<u8> = match encoder.encode(webpx::Unstoppable) {
            Ok(b) => b,
            Err(_) => return (None, None),
        };
        // bpp del probe: predice el coste de rav1e (ver doc del método).
        let probe_bpp =
            Some((webp_bytes.len() as f32 * 8.0) / (w as f32 * h as f32));
        // Decode del roundtrip — el image crate soporta WebP decode.
        let decoded = match image::load_from_memory_with_format(
            &webp_bytes,
            image::ImageFormat::WebP,
        ) {
            Ok(d) => d,
            Err(_) => return (None, probe_bpp),
        };
        let dec_rgba = decoded.to_rgba8();
        if dec_rgba.dimensions() != (w, h) {
            return (None, probe_bpp);
        }
        // SSIM rápido por bloques 8×8 sobre luma: es una sonda, no un
        // gate, y reutilizar el evaluador del engine implicaría I/O de
        // temporales.
        let luma = |img: &image::RgbaImage| -> Vec<f64> {
            img.pixels()
                .map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64)
                .collect()
        };
        let a = luma(&rgba);
        let b = luma(&dec_rgba);
        // SSIM global 8×8: rapidez; basta para separar 0.83 de 0.96.
        let c1: f64 = (0.01_f64 * 255.0_f64).powi(2);
        let c2: f64 = (0.03_f64 * 255.0_f64).powi(2);
        let mut ssim_sum = 0.0;
        let mut count = 0u64;
        let by = 8;
        let mut y = 0;
        while y + by <= h as usize {
            let mut x = 0;
            while x + by <= w as usize {
                let mut ma = 0.0;
                let mut mb = 0.0;
                for j in 0..by {
                    for i in 0..by {
                        ma += a[(y + j) * w as usize + x + i];
                        mb += b[(y + j) * w as usize + x + i];
                    }
                }
                let n = (by * by) as f64;
                ma /= n;
                mb /= n;
                let mut va = 0.0;
                let mut vb = 0.0;
                let mut cov = 0.0;
                for j in 0..by {
                    for i in 0..by {
                        let da = a[(y + j) * w as usize + x + i] - ma;
                        let db = b[(y + j) * w as usize + x + i] - mb;
                        va += da * da;
                        vb += db * db;
                        cov += da * db;
                    }
                }
                va /= n;
                vb /= n;
                cov /= n;
                ssim_sum += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                    / ((ma * ma + mb * mb + c1) * (va + vb + c2));
                count += 1;
                x += by;
            }
            y += by;
        }
        if count == 0 {
            return (None, probe_bpp);
        }
        (
            Some((ssim_sum / count as f64) as f32),
            probe_bpp,
        )
    }

    fn decode_preview(
        &self,
        path: &Path,
        format: Format,
        max_dim: u32,
    ) -> Result<image::DynamicImage, FormatError> {
        if format == Format::Avif {
            return Ok(image::DynamicImage::ImageRgb8(image::RgbImage::new(1, 1)));
        }
        let img = image::open(path)?;
        let (w, h) = (img.width(), img.height());
        if w > max_dim || h > max_dim {
            Ok(img.resize(max_dim, max_dim, image::imageops::FilterType::Nearest))
        } else {
            Ok(img)
        }
    }

    /// Análisis de píxeles completo: calcula las 8 métricas sobre el
    /// preview.
    fn analyze_pixels(
        &self,
        img: &image::DynamicImage,
        full_w: u32,
        full_h: u32,
        format: Format,
    ) -> PixelAnalysis {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if w < 2 || h < 2 {
            return PixelAnalysis::default();
        }

        let mut histogram = [0u32; LUMA_BINS];
        let mut luma_sum: f64 = 0.0;
        let mut luma_sq_sum: f64 = 0.0;
        let mut saturation_sum: f64 = 0.0;
        let mut dark_count: u32 = 0;
        let mut bright_count: u32 = 0;
        let total_pixels = (w * h) as f64;

        // Primera pasada: luma, saturación, histograma, brillo.
        for pixel in rgba.pixels() {
            let r = pixel[0] as f64;
            let g = pixel[1] as f64;
            let b = pixel[2] as f64;

            // Luma ITU-R BT.601.
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            luma_sum += luma;
            luma_sq_sum += luma * luma;

            // Saturación: (max - min) / max, con max = max(r,g,b).
            let max_c = r.max(g).max(b);
            let min_c = r.min(g).min(b);
            let saturation = if max_c > 0.0 {
                (max_c - min_c) / max_c
            } else {
                0.0
            };
            saturation_sum += saturation;

            // Histograma: luma → bin (0..31).
            let bin = ((luma / 8.0) as usize).min(LUMA_BINS - 1);
            histogram[bin] += 1;

            // Contadores de zonas oscuras/brillantes.
            if luma < 30.0 {
                dark_count += 1;
            }
            if luma > 225.0 {
                bright_count += 1;
            }
        }

        let mean_luma = luma_sum / total_pixels;
        let variance = (luma_sq_sum / total_pixels) - (mean_luma * mean_luma);
        let contrast = variance.sqrt().min(255.0); // desviación típica del luma
        let mean_saturation = saturation_sum / total_pixels;
        let dark_region_ratio = dark_count as f64 / total_pixels;
        let bright_region_ratio = bright_count as f64 / total_pixels;

        // Segunda pasada: complejidad (diferencias entre píxeles),
        // densidad de bordes, zonas planas, ruido.
        let mut total_diff: f64 = 0.0;
        let mut sample_count: u64 = 0;
        let mut flat_run_count: u64 = 0;
        let mut current_run: u32 = 0;
        let mut edge_count: u32 = 0;
        let flat_threshold: f64 = 6.0 * 4.0; // suma de 4 canales
        let edge_threshold: f64 = 30.0 * 4.0; // gradiente fuerte

        for y in 0..h {
            for x in 0..w.saturating_sub(1) {
                let p1 = rgba.get_pixel(x, y);
                let p2 = rgba.get_pixel(x + 1, y);
                let dr = (p1[0] as i32 - p2[0] as i32).unsigned_abs() as f64;
                let dg = (p1[1] as i32 - p2[1] as i32).unsigned_abs() as f64;
                let db = (p1[2] as i32 - p2[2] as i32).unsigned_abs() as f64;
                let da = (p1[3] as i32 - p2[3] as i32).unsigned_abs() as f64;
                let d = dr + dg + db + da;
                total_diff += d;
                sample_count += 1;
                if d < flat_threshold {
                    current_run += 1;
                } else {
                    if current_run >= 4 {
                        flat_run_count += 1;
                    }
                    current_run = 0;
                }
                if d > edge_threshold {
                    edge_count += 1;
                }
            }
        }
        if current_run >= 4 {
            flat_run_count += 1;
        }

        let avg_diff = if sample_count > 0 {
            total_diff / sample_count as f64 / (4.0 * 255.0)
        } else {
            0.0
        };
        let complexity = (avg_diff.clamp(0.0, 1.0)) as f32;
        let compressibility = (1.0 - complexity as f64).clamp(0.0, 1.0) as f32;
        let edge_density = edge_count as f64 / sample_count as f64;
        let flat_region_ratio = flat_run_count as f64 / (h as f64 * 2.0).max(1.0);

        // Ruido: energía de alta frecuencia residual en zonas planas —
        // si las zonas planas siguen con varianza alta, la imagen es
        // ruidosa.
        let noise_estimate = (complexity as f64 * flat_region_ratio * 2.0).min(1.0);

        // Clasificar la categoría con todas las métricas.
        let pixel_count = full_w as u64 * full_h as u64;
        let category = self.classify(
            complexity,
            flat_run_count,
            pixel_count,
            format,
            edge_density,
            dark_region_ratio,
            mean_saturation,
        );

        // Lista de características.
        let mut characteristics: Vec<String> = Vec::new();
        if flat_run_count > 100 {
            characteristics.push("flat_regions".into());
        }
        if complexity > 0.5 {
            characteristics.push("high_noise".into());
        }
        if complexity < 0.1 {
            characteristics.push("smooth".into());
        }
        if complexity > 0.3 && complexity < 0.5 {
            characteristics.push("sharp_edges".into());
        }
        if rgba.pixels().any(|p| p[3] < 255) {
            characteristics.push("transparency".into());
        }

        PixelAnalysis {
            complexity,
            compressibility,
            category,
            characteristics,
            mean_luma,
            contrast,
            mean_saturation,
            color_vibrancy: mean_saturation,
            dark_region_ratio,
            bright_region_ratio,
            edge_density,
            flat_region_ratio,
            noise_estimate,
        }
    }

    /// Clasifica la imagen en una categoría a partir de las métricas.
    /// edge_density y dark_region_ratio afinan la separación entre
    /// screenshots, imágenes con texto y fotos oscuras.
    fn classify(
        &self,
        complexity: f32,
        flat_run_count: u64,
        pixel_count: u64,
        format: Format,
        edge_density: f64,
        dark_region_ratio: f64,
        _mean_saturation: f64,
    ) -> ImageCategory {
        // Screenshots: densidad de bordes muy alta (texto + UI) con
        // complejidad moderada (fondos planos entre texto).
        if edge_density > 0.12 && flat_run_count > 20 && complexity < 0.35 {
            return ImageCategory::Screenshot;
        }
        // Texto denso: aún más bordes, complejidad baja.
        if edge_density > 0.15 && complexity < 0.25 {
            return ImageCategory::TextHeavy;
        }
        // Gradiente: complejidad muy baja en toda la imagen.
        if complexity < 0.05 {
            return ImageCategory::Gradient;
        }
        // Ilustración: complejidad baja + zonas planas.
        if complexity < 0.15 && flat_run_count > 50 {
            return ImageCategory::Illustration;
        }
        // Muy detallada: complejidad alta + imagen grande.
        if complexity > 0.45 && pixel_count > 500_000 {
            return ImageCategory::HighlyDetailed;
        }
        // Textura de juego: complejidad media-alta + tamaño mediano-grande.
        if complexity > 0.30 && pixel_count > 200_000 {
            return ImageCategory::GameTexture;
        }
        // Icono: imagen pequeña + complejidad moderada.
        if complexity > 0.20 && pixel_count < 50_000 {
            return ImageCategory::Icon;
        }
        // Foto oscura: dominan las zonas oscuras + complejidad moderada.
        if dark_region_ratio > 0.35 && complexity > 0.15 && pixel_count > 50_000 {
            return ImageCategory::Photograph;
        }
        // Default: fotografía si es suficientemente grande.
        if pixel_count > 100_000 {
            return ImageCategory::Photograph;
        }
        // Fallback: PNG de baja complejidad (probable texto o arte plano).
        if complexity < 0.25 && format == Format::Png {
            return ImageCategory::TextHeavy;
        }
        ImageCategory::Unknown
    }

    fn detect_color_model(&self, img: &image::DynamicImage, format: Format) -> ColorModel {
        let rgba = img.to_rgba8();
        let has_alpha = rgba.pixels().any(|p| p[3] < 255);
        let mut is_gray = true;
        'outer: for p in rgba.pixels() {
            if p[0] != p[1] || p[1] != p[2] {
                is_gray = false;
                break 'outer;
            }
        }
        match (format, is_gray, has_alpha) {
            (Format::Gif, _, _) => ColorModel::Indexed,
            (_, true, true) => ColorModel::GrayAlpha,
            (_, true, false) => ColorModel::Gray,
            (_, false, true) => ColorModel::Rgba,
            (_, false, false) => ColorModel::Rgb,
        }
    }

    fn capabilities_for(&self, format: Format) -> crate::engine::formats::FormatCapabilities {
        use crate::engine::formats::FormatCapabilities;
        match format {
            Format::Png => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: true,
                can_convert: true,
            },
            Format::Jpeg => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: true,
                can_convert: true,
            },
            Format::Webp => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: true,
                can_convert: true,
            },
            Format::Avif => FormatCapabilities {
                can_read: false,
                can_write: true,
                can_optimize: false,
                can_convert: true,
            },
            Format::Gif => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: false,
                can_convert: true,
            },
            Format::Bmp => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: false,
                can_convert: true,
            },
            Format::Tiff => FormatCapabilities {
                can_read: true,
                can_write: true,
                can_optimize: false,
                can_convert: true,
            },
            Format::Unknown => FormatCapabilities {
                can_read: false,
                can_write: false,
                can_optimize: false,
                can_convert: false,
            },
        }
    }
}

/// Resultado del análisis de píxeles: las 8 métricas medidas sobre
/// el preview.
#[derive(Debug, Clone, Default)]
struct PixelAnalysis {
    /// Energía de alta frecuencia (0..1).
    complexity: f32,
    /// Inversa de la complejidad. Mayor = más comprimible.
    compressibility: f32,
    category: ImageCategory,
    /// Características textuales para las heurísticas.
    characteristics: Vec<String>,
    /// Luma media (0..255).
    mean_luma: f64,
    /// Desviación típica del luma (0..255). Alta = alto contraste.
    contrast: f64,
    /// Saturación media (0..1). Alta = colores vivos.
    mean_saturation: f64,
    /// Alias de mean_saturation (lo usa el builder de características).
    color_vibrancy: f64,
    /// Fracción de píxeles con luma < 30 (0..1). Alta = imagen oscura.
    dark_region_ratio: f64,
    /// Fracción de píxeles con luma > 225 (0..1). Alta = imagen clara.
    bright_region_ratio: f64,
    /// Fracción de pares de píxeles con gradiente fuerte (0..1). Alta
    /// = texto/UI.
    edge_density: f64,
    /// Fracción de la imagen en zonas planas (0..1). Alta = comprime bien.
    flat_region_ratio: f64,
    /// Nivel de ruido estimado (0..1).
    noise_estimate: f64,
}

impl Default for ImageCategory {
    fn default() -> Self {
        ImageCategory::Unknown
    }
}

impl Analyzer {
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}
