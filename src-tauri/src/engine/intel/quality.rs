//! QualityEvaluator — métricas objetivas de calidad de imagen: MSE,
//! PSNR, SSIM y butteraugli. butteraugli es la métrica perceptual: más
//! sensible que SSIM a los artifacts que el ojo ve (sombras, degradados,
//! bordes finos); se calcula como desempate cuando SSIM cae en zona gris
//! (0.85-0.95), o siempre en modo perceptual.
//!
//! Para lossless devuelve un marcador sin comparar píxeles. Para lossy,
//! ambas imágenes se decodifican y comparan a la misma resolución (el
//! output se resamplea si hubo resize).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::formats::FormatError;

/// Defense-in-depth: refuse to decode images whose pixel count would
/// risk OOM during quality evaluation. The analyzer enforces a similar
/// cap, but the quality evaluator runs on candidate outputs (which may
/// have been resized) and must not trust that cap was honored. A 50 MP
/// RGBA image is ~200 MB; allowing two such images side by side keeps
/// us well under typical desktop memory budgets.
const MAX_QUALITY_EVAL_PIXELS: u64 = 50_000_000;

/// Threshold de SSIM por debajo del cual consultamos butteraugli.
/// Si SSIM > 0.95, el candidato es obviamente bueno — butteraugli
/// no agregaría información. Si SSIM < 0.85, el candidato es
/// obviamente malo — butteraugli no lo rescataría.
///
/// `pub(crate)`: el scoring engine lo usa como piso de calidad para
/// candidatos iterativos.
pub(crate) const BUTTERAUGLI_SSIM_LOW: f64 = 0.85;
const BUTTERAUGLI_SSIM_HIGH: f64 = 0.95;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Mean squared error. 0 = identical.
    pub mse: f64,
    /// Peak signal-to-noise ratio in dB. Infinity = identical.
    pub psnr: f64,
    /// Structural similarity index. 1.0 = identical. 0 = unrelated.
    pub ssim: f64,
    /// True if the candidate is lossless by construction.
    pub is_lossless: bool,
    /// butteraugli score. None = no calculado (SSIM fuera de zona gris
    /// o candidato lossless). Menor = mejor. < 1.0 = visualmente
    /// idéntico. > 2.0 = visiblemente diferente.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub butteraugli: Option<f64>,
}

impl QualityMetrics {
    /// Returns a lossless marker (no comparison performed).
    pub fn lossless() -> Self {
        Self {
            mse: 0.0,
            psnr: f64::INFINITY,
            ssim: 1.0,
            is_lossless: true,
            butteraugli: None,
        }
    }

    /// Normalized quality score 0.0..=1.0 for use by the scoring engine.
    /// Blend de SSIM y PSNR: SSIM mide estructura y PSNR fidelidad de
    /// píxel — el blend conserva ambas señales sin anclar el score a 0
    /// por un PSNR bajo con SSIM decente. butteraugli actúa de desempate
    /// perceptual (>1.5 penaliza, <0.5 bonifica).
    pub fn normalized_score(&self) -> f64 {
        if self.is_lossless {
            return 1.0;
        }
        if self.psnr.is_nan() || self.ssim.is_nan() {
            return 0.0;
        }
        let apply_butteraugli = |mut score: f64| {
            if let Some(ba) = self.butteraugli {
                if ba > 1.5 {
                    score *= 0.9;
                } else if ba < 0.5 {
                    score *= 1.05; // bonus por visualmente idéntico
                }
            }
            score.min(1.0)
        };
        if self.psnr.is_infinite() && self.psnr.is_sign_positive() {
            let score = self.ssim.clamp(0.0, 1.0);
            return apply_butteraugli(score);
        }
        let psnr_score = ((self.psnr - 20.0) / 30.0).clamp(0.0, 1.0);
        let score = self.ssim.clamp(0.0, 1.0) * (0.85 + 0.15 * psnr_score);
        apply_butteraugli(score)
    }

    /// Decide si vale la pena calcular butteraugli para este candidato.
    /// Solo se calcula cuando SSIM está en zona gris (0.85-0.95) —
    /// fuera de esa zona, SSIM es suficiente.
    pub fn should_compute_butteraugli(&self) -> bool {
        !self.is_lossless
            && self.ssim >= BUTTERAUGLI_SSIM_LOW
            && self.ssim <= BUTTERAUGLI_SSIM_HIGH
            && self.butteraugli.is_none()
    }

    /// Score de calidad para candidatos validados perceptualmente por el
    /// iterative search: como [`Self::normalized_score`] pero sin el
    /// componente PSNR. El PSNR castiga píxel-error que la métrica
    /// perceptual ya aprobó (p. ej. el denoising suave de un lossy q
    /// medio sobre una imagen ruidosa); mantenerlo re-introducía un
    /// doble gate contradictorio. Los ajustes de butteraugli se
    /// mantienen (información de ranking).
    pub fn iterative_normalized_score(&self) -> f64 {
        if self.is_lossless {
            return 1.0;
        }
        if self.ssim.is_nan() {
            return 0.0;
        }
        let mut score = self.ssim.clamp(0.0, 1.0);
        if let Some(ba) = self.butteraugli {
            if ba > 1.5 {
                score *= 0.9;
            } else if ba < 0.5 {
                score *= 1.05;
            }
        }
        score.min(1.0)
    }
}

pub struct QualityEvaluator;

impl QualityEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluate quality of `output` against `original`.
    ///
    /// If `candidate_is_lossless`, returns a lossless marker without
    /// decoding — the scorer will treat it as perfect quality.
    pub fn evaluate(
        &self,
        original: &Path,
        output: &Path,
        candidate_is_lossless: bool,
    ) -> Result<QualityMetrics, FormatError> {
        if candidate_is_lossless {
            return Ok(QualityMetrics::lossless());
        }

        let orig_img = self.decode(original)?;
        let out_img = self.decode(output)?;

        // Defense-in-depth: even if the analyzer allowed the input, refuse
        // to evaluate if either image is too large — we hold both in RAM
        // simultaneously during comparison.
        let orig_pixels = u64::from(orig_img.width()) * u64::from(orig_img.height());
        let out_pixels = u64::from(out_img.width()) * u64::from(out_img.height());
        if orig_pixels > MAX_QUALITY_EVAL_PIXELS || out_pixels > MAX_QUALITY_EVAL_PIXELS {
            return Err(FormatError::Corrupted(format!(
                "image too large for quality evaluation (orig={orig_pixels} px, out={out_pixels} px; max={MAX_QUALITY_EVAL_PIXELS})"
            )));
        }

        // If dimensions differ (resize was applied), resample output
        // to match original for fair comparison.
        let (ow, oh) = (orig_img.width(), orig_img.height());
        let out_resized = if out_img.width() != ow || out_img.height() != oh {
            out_img.resize(ow, oh, image::imageops::FilterType::Lanczos3)
        } else {
            out_img
        };

        let orig_rgba = orig_img.to_rgba8();
        let out_rgba = out_resized.to_rgba8();

        // premultiplicar alpha: la comparación cruda RGBA penaliza
        // diferencias de RGB en píxeles INVISIBLES (a=0), donde el
        // encoder puede usar cualquier valor. butteraugli NO recibe
        // premultiplicadas: compone sobre blanco internamente.
        let (orig_pm, out_pm) = (
            premultiply_rgba(&orig_rgba),
            premultiply_rgba(&out_rgba),
        );

        let mse = self.compute_mse(&orig_pm, &out_pm);
        let psnr = self.compute_psnr(mse);
        let ssim = self.compute_ssim(&orig_pm, &out_pm);

        // butteraugli advisory — solo se calcula en zona gris.
        let mut metrics = QualityMetrics {
            mse,
            psnr,
            ssim,
            is_lossless: mse == 0.0,
            butteraugli: None,
        };
        if metrics.should_compute_butteraugli() {
            if let Ok(ba) = self.compute_butteraugli(&orig_rgba, &out_rgba) {
                metrics.butteraugli = Some(ba);
            }
            // Si butteraugli falla (OOM, formato raro), simplemente lo
            // dejamos en None — el scorer cae al comportamiento previo.
        }

        Ok(metrics)
    }

    /// Compute butteraugli perceptual score between two RGBA images.
    ///
    /// Returns a score where:
    ///   - < 1.0 = visualmente idéntico
    ///   - 1.0..=2.0 = diferencia apenas perceptible
    ///   - > 2.0 = visiblemente diferente
    ///
    /// butteraugli requiere RGB8 (no RGBA). Si la imagen tiene alpha,
    /// la componemos sobre fondo blanco antes de pasarla.
    fn compute_butteraugli(
        &self,
        orig: &image::RgbaImage,
        dist: &image::RgbaImage,
    ) -> Result<f64, FormatError> {
        // butteraugli tiene un límite de tamaño interno ~10MP para
        // evitar OOM. Para imágenes grandes, reducimos a 1024px max.
        let (ow, oh) = (orig.width(), orig.height());
        let (nw, nh) = butteraugli_scale(ow, oh);
        let (orig_rgb, dist_rgb) = if nw != ow || nh != oh {
            let orig_small = image::imageops::resize(orig, nw, nh, image::imageops::FilterType::Lanczos3);
            let dist_small = image::imageops::resize(dist, nw, nh, image::imageops::FilterType::Lanczos3);
            (rgba_to_rgb(&orig_small), rgba_to_rgb(&dist_small))
        } else {
            (rgba_to_rgb(orig), rgba_to_rgb(dist))
        };
        butteraugli_score(&orig_rgb, &dist_rgb)
    }

    /// Evaluate quality of `output` against a pre-decoded original
    /// shared via the per-run [`OriginalImageCache`] — el original se
    /// decodifica UNA vez por run (lazy, con el primer candidato lossy)
    /// y el resto reutilizan el mismo `Arc<DynamicImage>`. El guard de
    /// píxeles se aplica dentro del cache.
    ///
    /// If `candidate_is_lossless`, returns a lossless marker without
    /// touching the cache.
    pub fn evaluate_with_cache(
        &self,
        cache: &super::original_cache::OriginalImageCache,
        output: &Path,
        candidate_is_lossless: bool,
    ) -> Result<QualityMetrics, FormatError> {
        self.evaluate_with_cache_opts(cache, output, candidate_is_lossless, false)
    }

    /// Evaluación PERCEPTUAL para el gate del modo "Comprimir al
    /// Máximo" ([`super::goal::OptimizationGoal::uses_perceptual_gate`]):
    /// idéntica a [`Self::evaluate_with_cache`] pero butteraugli se
    /// calcula SIEMPRE para lossy, no solo en la zona gris de SSIM. Los
    /// candidatos extremos miden SSIM 0.6-0.85 — fuera de la zona gris —
    /// y el scorer necesita el daño perceptual real para el gate.
    pub fn evaluate_with_cache_perceptual(
        &self,
        cache: &super::original_cache::OriginalImageCache,
        output: &Path,
        candidate_is_lossless: bool,
    ) -> Result<QualityMetrics, FormatError> {
        self.evaluate_with_cache_opts(cache, output, candidate_is_lossless, true)
    }

    fn evaluate_with_cache_opts(
        &self,
        cache: &super::original_cache::OriginalImageCache,
        output: &Path,
        candidate_is_lossless: bool,
        always_butteraugli: bool,
    ) -> Result<QualityMetrics, FormatError> {
        if candidate_is_lossless {
            return Ok(QualityMetrics::lossless());
        }

        // Reuse the cached original — no re-decode.
        let orig_img = cache.get_or_decode()?;
        let out_img = self.decode(output)?;

        // Defense-in-depth: even though the cache enforces the pixel
        // guard, the candidate output may be a different (larger)
        // size. Verify it too.
        let orig_pixels = u64::from(orig_img.width()) * u64::from(orig_img.height());
        let out_pixels = u64::from(out_img.width()) * u64::from(out_img.height());
        if out_pixels > MAX_QUALITY_EVAL_PIXELS {
            return Err(FormatError::Corrupted(format!(
                "candidate output too large for quality evaluation ({} px; max {})",
                out_pixels, MAX_QUALITY_EVAL_PIXELS
            )));
        }
        // The cache already verified orig_pixels; this is a sanity check.
        debug_assert!(orig_pixels <= MAX_QUALITY_EVAL_PIXELS);

        // If dimensions differ (resize was applied), resample output
        // to match original for fair comparison.
        let (ow, oh) = (orig_img.width(), orig_img.height());
        let out_resized = if out_img.width() != ow || out_img.height() != oh {
            out_img.resize(ow, oh, image::imageops::FilterType::Lanczos3)
        } else {
            out_img
        };

        let orig_rgba = orig_img.to_rgba8();
        let out_rgba = out_resized.to_rgba8();

        // premultiplicar alpha (misma razón que `evaluate`);
        // butteraugli recibe las crudas.
        let (orig_pm, out_pm) = (
            premultiply_rgba(&orig_rgba),
            premultiply_rgba(&out_rgba),
        );

        let mse = self.compute_mse(&orig_pm, &out_pm);
        let psnr = self.compute_psnr(mse);
        let ssim = self.compute_ssim(&orig_pm, &out_pm);

        // butteraugli: zona gris, o siempre en modo perceptual (el
        // scorer necesita el daño real de los candidatos de décadas
        // bajas).
        let mut metrics = QualityMetrics {
            mse,
            psnr,
            ssim,
            is_lossless: mse == 0.0,
            butteraugli: None,
        };
        if (always_butteraugli && !metrics.is_lossless) || metrics.should_compute_butteraugli() {
            if let Ok(ba) = self.compute_butteraugli(&orig_rgba, &out_rgba) {
                metrics.butteraugli = Some(ba);
            }
        }

        Ok(metrics)
    }

    fn decode(&self, path: &Path) -> Result<image::DynamicImage, FormatError> {
        // Detectar el formato por CONTENIDO (magic bytes), no por
        // extensión: los temporales del iterative search son `*.qNN.tmp`
        // y `image::open` rechazaría la extensión sin leer el archivo.
        // Con la feature `avif-native` el crate image también decodifica
        // AVIF vía dav1d (incluido el plano alpha auxiliar).
        let reader = image::ImageReader::open(path)?
            .with_guessed_format()?; // sniff magic bytes, ignora la extensión
        Ok(reader.decode()?)
    }

    fn compute_mse(&self, a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
        let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
        if w == 0 || h == 0 {
            return 0.0;
        }
        let mut sum: f64 = 0.0;
        let mut count: u64 = 0;
        for y in 0..h {
            for x in 0..w {
                let pa = a.get_pixel(x, y);
                let pb = b.get_pixel(x, y);
                for c in 0..4 {
                    let d = pa[c] as f64 - pb[c] as f64;
                    sum += d * d;
                    count += 1;
                }
            }
        }
        if count == 0 {
            0.0
        } else {
            sum / count as f64
        }
    }

    fn compute_psnr(&self, mse: f64) -> f64 {
        if mse == 0.0 {
            return f64::INFINITY;
        }
        let max_i = 255.0_f64;
        10.0 * (max_i * max_i / mse).log10()
    }

    /// Computes a global SSIM over the image using the **Wang et al.
    /// (2004) standard**: an 11×11 Gaussian-weighted sliding window
    /// (σ = 1.5) stepping pixel-by-pixel, computed on **luma** (Rec. 601).
    /// Las ventanas solapadas son más sensibles a artifacts locales que
    /// el 8×8 no solapado que usan algunos benchmarks. ~5× más caro que
    /// el 8×8; en 1920×1080 toma <100 ms — despreciable frente al encode.
    fn compute_ssim(&self, a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
        let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
        // Imagen más pequeña que la ventana 11×11: SSIM global (un
        // único bloque).
        if w < 11 || h < 11 {
            return self.simple_ssim(a, b);
        }

        // Pre-computar buffers de luma: el SSIM los recorre secuencial
        // (cache-friendly) en vez de O(w*h*window²) accesos con
        // `get_pixel`.
        let luma_a = Self::luma_buffer(a);
        let luma_b = Self::luma_buffer(b);

        // Kernel estándar del paper original, normalizado a suma 1.0.
        let kernel = Self::gaussian_kernel_2d(11, 1.5);

        // Constantes de estabilización (Wang et al. 2004).
        let l_max = 255.0_f64;
        let c1: f64 = (0.01 * l_max).powi(2);
        let c2: f64 = (0.03 * l_max).powi(2);

        let window = 11usize;
        let half = window / 2; // 5
        let stride = 1usize; // solapamiento: desplazar 1 píxel a la vez.

        let mut ssim_sum: f64 = 0.0;
        let mut window_count: u64 = 0;

        let mut y = half;
        while y + half < h as usize {
            let mut x = half;
            while x + half < w as usize {
                // Calcular media, varianza y covarianza ponderadas
                // por la Gaussiana 11×11 centrada en (x, y).
                let (ma, va) = Self::weighted_stats(&luma_a, w as usize, x, y, half, &kernel);
                let (mb, vb) = Self::weighted_stats(&luma_b, w as usize, x, y, half, &kernel);
                let cov =
                    Self::weighted_cov(&luma_a, &luma_b, w as usize, x, y, half, &kernel, ma, mb);

                let numerator = (2.0 * ma * mb + c1) * (2.0 * cov + c2);
                let denominator = (ma * ma + mb * mb + c1) * (va + vb + c2);
                if denominator > 0.0 {
                    ssim_sum += numerator / denominator;
                    window_count += 1;
                }
                x += stride;
            }
            y += stride;
        }

        if window_count == 0 {
            self.simple_ssim(a, b)
        } else {
            ssim_sum / window_count as f64
        }
    }

    /// Pre-computa un buffer 1D de valores de luma float (Rec. 601)
    /// para una imagen RGBA. Mucho más rápido que `get_pixel()`
    /// repetidamente durante el cómputo de SSIM.
    fn luma_buffer(img: &image::RgbaImage) -> Vec<f64> {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let p = img.get_pixel(x as u32, y as u32);
                // ITU-R BT.601 luma.
                let luma = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
                out.push(luma);
            }
        }
        out
    }

    /// Genera un kernel Gaussiano 2D de tamaño `size × size` con
    /// desviación `sigma`. Normalizado a suma 1.0.
    fn gaussian_kernel_2d(size: usize, sigma: f64) -> Vec<f64> {
        let half = (size / 2) as isize;
        let two_sigma_sq = 2.0 * sigma * sigma;
        let mut k = vec![0.0_f64; size * size];
        let mut sum = 0.0_f64;
        for dy in -half..=half {
            for dx in -half..=half {
                let r2 = (dx * dx + dy * dy) as f64;
                let w = (-r2 / two_sigma_sq).exp();
                let ix = (dx + half) as usize;
                let iy = (dy + half) as usize;
                k[iy * size + ix] = w;
                sum += w;
            }
        }
        // Normalizar a suma = 1.0.
        for v in k.iter_mut() {
            *v /= sum;
        }
        k
    }

    /// Estadísticas ponderadas (media, varianza) sobre una ventana
    /// centrada en (cx, cy) usando el kernel Gaussiano.
    fn weighted_stats(
        buf: &[f64],
        width: usize,
        cx: usize,
        cy: usize,
        half: usize,
        kernel: &[f64],
    ) -> (f64, f64) {
        let size = half * 2 + 1;
        let mut mean = 0.0_f64;
        // Primera pasada: media ponderada.
        for dy in 0..size {
            for dx in 0..size {
                let px = cx + dx - half;
                let py = cy + dy - half;
                let v = buf[py * width + px];
                let w = kernel[dy * size + dx];
                mean += v * w;
            }
        }
        // Segunda pasada: varianza ponderada.
        let mut variance = 0.0_f64;
        for dy in 0..size {
            for dx in 0..size {
                let px = cx + dx - half;
                let py = cy + dy - half;
                let v = buf[py * width + px];
                let w = kernel[dy * size + dx];
                let d = v - mean;
                variance += d * d * w;
            }
        }
        (mean, variance.max(0.0))
    }

    /// Covarianza ponderada entre dos buffers sobre la misma ventana.
    fn weighted_cov(
        a: &[f64],
        b: &[f64],
        width: usize,
        cx: usize,
        cy: usize,
        half: usize,
        kernel: &[f64],
        ma: f64,
        mb: f64,
    ) -> f64 {
        let size = half * 2 + 1;
        let mut cov = 0.0_f64;
        for dy in 0..size {
            for dx in 0..size {
                let px = cx + dx - half;
                let py = cy + dy - half;
                let va = a[py * width + px];
                let vb = b[py * width + px];
                let w = kernel[dy * size + dx];
                cov += (va - ma) * (vb - mb) * w;
            }
        }
        cov
    }

    fn window_stats(&self, img: &image::RgbaImage, x0: u32, y0: u32, window: usize) -> (f64, f64) {
        let mut sum: f64 = 0.0;
        let mut sum_sq: f64 = 0.0;
        let mut n: f64 = 0.0;
        for y in y0..(y0 + window as u32) {
            for x in x0..(x0 + window as u32) {
                let p = img.get_pixel(x, y);
                let luma = 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64;
                sum += luma;
                sum_sq += luma * luma;
                n += 1.0;
            }
        }
        let mean = sum / n;
        let variance = (sum_sq / n) - (mean * mean);
        (mean, variance.max(0.0))
    }

    fn window_cov(
        &self,
        a: &image::RgbaImage,
        b: &image::RgbaImage,
        x0: u32,
        y0: u32,
        window: usize,
        ma: f64,
        mb: f64,
    ) -> f64 {
        let mut sum: f64 = 0.0;
        let mut n: f64 = 0.0;
        for y in y0..(y0 + window as u32) {
            for x in x0..(x0 + window as u32) {
                let pa = a.get_pixel(x, y);
                let pb = b.get_pixel(x, y);
                let la = 0.299 * pa[0] as f64 + 0.587 * pa[1] as f64 + 0.114 * pa[2] as f64;
                let lb = 0.299 * pb[0] as f64 + 0.587 * pb[1] as f64 + 0.114 * pb[2] as f64;
                sum += (la - ma) * (lb - mb);
                n += 1.0;
            }
        }
        sum / n
    }

    fn simple_ssim(&self, a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
        let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
        if w == 0 || h == 0 {
            return 0.0;
        }
        let (ma, va) = self.window_stats(a, 0, 0, w as usize);
        let (mb, vb) = self.window_stats(b, 0, 0, w as usize);
        let cov = self.window_cov(a, b, 0, 0, w as usize, ma, mb);
        let c1: f64 = (0.01_f64 * 255.0_f64).powi(2);
        let c2: f64 = (0.03_f64 * 255.0_f64).powi(2);
        let numerator = (2.0 * ma * mb + c1) * (2.0 * cov + c2);
        let denominator = (ma * ma + mb * mb + c1) * (va + vb + c2);
        if denominator > 0.0 {
            numerator / denominator
        } else {
            0.0
        }
    }
}

impl Default for QualityEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Premultiplica el alpha de una imagen RGBA: `rgb' = rgb × (a/255)`.
/// Un píxel con alpha=0 produce (0,0,0,0) independientemente de su RGB
/// original — es invisible y no debe penalizar las métricas. Los codecs
/// lossy (WebP/AVIF) rellenan el RGB de las zonas transparentes con
/// valores arbitrarios: comparar RGBA crudo penaliza PSNR/SSIM en logos
/// con transparencia aunque visualmente sean idénticos. Para imágenes
/// opacas (alpha=255 en todo) la operación es la identidad.
fn premultiply_rgba(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    // Fast path: imagen opaca → clonar sin tocar (la mayoría de fotos).
    if img.pixels().all(|p| p[3] == 255) {
        return img.clone();
    }
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y);
            let a = p[3] as f64 / 255.0;
            let px = image::Rgba([
                (p[0] as f64 * a).round() as u8,
                (p[1] as f64 * a).round() as u8,
                (p[2] as f64 * a).round() as u8,
                p[3],
            ]);
            out.put_pixel(x, y, px);
        }
    }
    out
}

/// Convierte RGBA8 a RGB8 componiendo sobre fondo blanco — butteraugli
/// requiere RGB8. Es el comportamiento estándar de la mayoría de viewers
/// y evita artifacts visibles en la comparación.
fn rgba_to_rgb(rgba: &image::RgbaImage) -> image::RgbImage {
    let (w, h) = rgba.dimensions();
    let mut out = image::RgbImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = rgba.get_pixel(x, y);
            let a = p[3] as f64 / 255.0;
            let r = (p[0] as f64 * a + 255.0 * (1.0 - a)).round() as u8;
            let g = (p[1] as f64 * a + 255.0 * (1.0 - a)).round() as u8;
            let b = (p[2] as f64 * a + 255.0 * (1.0 - a)).round() as u8;
            out.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    out
}

/// Resolución objetivo de butteraugli: cap a 1024 px en el lado mayor
/// (manteniendo aspect ratio). Extraída como función para que el
/// iterative search prepare la referencia del original con la MISMA
/// regla de escala y no pague el Lanczos3 del original en cada rung.
fn butteraugli_scale(w: u32, h: u32) -> (u32, u32) {
    const MAX_DIM: u32 = 1024;
    if w > MAX_DIM || h > MAX_DIM {
        let ratio = (MAX_DIM as f32) / (w.max(h) as f32).max(1.0);
        let nw = (((w as f32) * ratio).round() as u32).max(1);
        let nh = (((h as f32) * ratio).round() as u32).max(1);
        (nw, nh)
    } else {
        (w, h)
    }
}

/// Núcleo de butteraugli sobre dos imágenes RGB8 ya preparadas (misma
/// resolución); la lógica de escala queda en el caller.
fn butteraugli_score(orig: &image::RgbImage, dist: &image::RgbImage) -> Result<f64, FormatError> {
    // butteraugli API: Img<RGB8> slices.
    use butteraugli::{butteraugli, ButteraugliParams, Img, RGB8};
    // El método `.as_ref()` de Img viene del trait `ImgExt` — sin este
    // import no compila.
    use imgref::ImgExt;
    let (w, h) = orig.dimensions();
    let orig_flat: Vec<RGB8> = orig.pixels().map(|p| RGB8::new(p[0], p[1], p[2])).collect();
    let dist_flat: Vec<RGB8> = dist.pixels().map(|p| RGB8::new(p[0], p[1], p[2])).collect();
    let orig_img_ref = Img::new(&orig_flat[..], w as usize, h as usize);
    let dist_img_ref = Img::new(&dist_flat[..], w as usize, h as usize);
    let result = butteraugli(
        orig_img_ref.as_ref(),
        dist_img_ref.as_ref(),
        &ButteraugliParams::default(),
    )
    .map_err(|e| FormatError::EncoderFailure(format!("butteraugli: {e}")))?;
    Ok(result.score)
}

impl QualityEvaluator {
    /// Referencia butteraugli del original, preparada UNA vez por
    /// búsqueda iterativa: decodifica vía cache (lazy, compartido con
    /// el resto del pipeline) y escala con la MISMA regla de
    /// `compute_butteraugli` (cap 1024px, Lanczos3). Los rungs la
    /// reutilizan: el resize del original se paga una sola vez.
    pub fn prepare_butteraugli_reference(
        &self,
        cache: &super::original_cache::OriginalImageCache,
    ) -> Result<image::RgbaImage, FormatError> {
        let orig_img = cache.get_or_decode()?;
        let orig_rgba = orig_img.to_rgba8();
        let (ow, oh) = (orig_rgba.width(), orig_rgba.height());
        let (nw, nh) = butteraugli_scale(ow, oh);
        if nw != ow || nh != oh {
            Ok(image::imageops::resize(
                &orig_rgba,
                nw,
                nh,
                image::imageops::FilterType::Lanczos3,
            ))
        } else {
            Ok(orig_rgba)
        }
    }

    /// butteraugli REAL de un output contra una referencia ya preparada
    /// (ver [`Self::prepare_butteraugli_reference`]). Cada rung del
    /// iterative search necesita una sola decisión binaria
    /// (`ba < target`): medir butteraugli directamente es más barato que
    /// el flujo completo de métricas, y no impone el suelo implícito de
    /// SSIM 0.85 que bloquearía explorar calidades bajas.
    pub fn butteraugli_against(
        &self,
        reference: &image::RgbaImage,
        output: &Path,
    ) -> Result<f64, FormatError> {
        let out_img = self.decode(output)?;
        let out_pixels = u64::from(out_img.width()) * u64::from(out_img.height());
        if out_pixels > MAX_QUALITY_EVAL_PIXELS {
            return Err(FormatError::Corrupted(format!(
                "candidate output too large for butteraugli evaluation ({} px; max {})",
                out_pixels, MAX_QUALITY_EVAL_PIXELS
            )));
        }
        let (rw, rh) = (reference.width(), reference.height());
        // Misma semántica que compute_butteraugli: ambas imágenes
        // terminan en la MISMA resolución objetivo.
        let out_resized = if out_img.width() != rw || out_img.height() != rh {
            out_img.resize(rw, rh, image::imageops::FilterType::Lanczos3)
        } else {
            out_img
        };
        let out_rgba = out_resized.to_rgba8();
        self.compute_butteraugli(reference, &out_rgba)
    }
}
