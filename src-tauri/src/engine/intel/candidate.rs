//! Candidate and CandidateGenerator.
//!
//! The generator walks the strategy's quality ranges and produces a
//! pruned list of [`Candidate`]s. It never brute-forces the full search
//! space — it consults [`OptimizationHeuristics`] to skip obviously
//! bad combinations.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::engine::formats::Format;
use crate::engine::optimization::{MetadataMode, ResizeOptions};

use super::heuristics::OptimizationHeuristics;
use super::profile::FileProfile;
use super::strategy::OptimizationStrategy;

/// Stable identifier for a candidate within a single optimization run.
pub type CandidateId = u32;

static NEXT_CANDIDATE_ID: AtomicU32 = AtomicU32::new(1);

fn next_candidate_id() -> CandidateId {
    // Candidate output names share the selected output directory, so ids
    // must remain process-wide unique to avoid collisions between concurrent
    // optimization runs. Skip zero when the counter eventually wraps.
    let mut current = NEXT_CANDIDATE_ID.load(Ordering::Relaxed);
    loop {
        let id = if current == 0 { 1 } else { current };
        let next = id.checked_add(1).unwrap_or(1);
        match NEXT_CANDIDATE_ID.compare_exchange_weak(
            current,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return id,
            Err(actual) => current = actual,
        }
    }
}

/// A single configuration the engine intends to test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: CandidateId,
    pub format: Format,
    /// None = lossless for this format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,
    pub lossless: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resize: Option<ResizeOptions>,
    pub metadata_mode: MetadataMode,
    /// Si es `true`, el backend debe preservar el perfil de color ICC
    /// del original en la salida. Threaded desde
    /// `OptimizationStrategy.preserve_color_profile`.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,
    /// Estimated effort 0.0..=1.0. Used by the scheduler to order work.
    pub estimated_effort: f32,

    /// Coste estático estimado (ms) en la máquina de referencia: función
    /// pura de (formato, calidad, lossless, píxeles) — no depende del
    /// reloj. El scorer lo usa como componente de velocidad para que el
    /// ranking sea reproducible; puntuar el tiempo medido haría la
    /// selección dependiente de la carga del sistema.
    ///
    /// 0 = no estimado (candidatos construidos a mano): el scorer cae al
    /// effort estático como aproximación.
    #[serde(default)]
    pub estimated_cost_ms: u64,
    /// Short human-readable label.
    pub label: String,
    /// Presupuesto de memoria en MB para este candidato. 0 = sin límite.
    /// Los backends lo consultan para elegir presets agresivos (zopfli,
    /// AVIF speed=4) o conservadores. Se propaga desde
    /// `AppSettings.max_memory_mb` vía `UserConstraints` y la strategy.
    #[serde(default)]
    pub memory_budget_mb: u64,

    /// `true` = ejecutar vía la búsqueda iterativa guiada por butteraugli
    /// en vez del encode directo del backend. Lo marca el generador para
    /// lossy JPEG/WebP/AVIF cuando el goal usa iterative search; el
    /// pipeline lo enruta a `iterative::iterative_quality_search`.
    #[serde(default)]
    pub iterative: bool,

    /// `true` si la imagen de origen es fotografía-like
    /// (`FileProfile::is_photo_like`). Lo fija el generador porque el
    /// scorer no tiene acceso al profile y lo necesita para elegir la
    /// curva de estimación de calidad AVIF (foto vs gráfico).
    #[serde(default)]
    pub photo_like: bool,

    /// Presupuesto de tiempo (ms) que este candidato puede consumir en su
    /// backend. Lo fija el pipeline como fracción del presupuesto del
    /// run: evita que un codec lento (zopfli, rav1e) acapare el tiempo
    /// compartido y mate de hambre al resto de candidatos.
    ///
    /// 0 = sin límite explícito (candidatos construidos fuera del
    /// pipeline).
    #[serde(default)]
    pub backend_time_budget_ms: u64,
}

fn default_preserve_color_profile() -> bool {
    true
}

/// Contexto de contenido para el coste estático: features deterministas
/// del `FileProfile` calculadas en ANALYSIS. El coste sigue siendo una
/// función pura — más información de entrada, nada de reloj.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentCostContext {
    /// bpp del probe WebP q75 del preview (`lossy_probe_bpp`).
    /// None = probe no disponible (fallback conservador).
    pub lossy_probe_bpp: Option<f32>,
    /// La imagen tiene canal alpha (encode_rgba: 2 planos).
    pub has_alpha: bool,
}

impl ContentCostContext {
    /// Contexto neutral para tests / candidatos construidos a mano:
    /// sin sonda, sin alpha → el modelo cae al fallback.
    pub fn opaque() -> Self {
        Self {
            lossy_probe_bpp: None,
            has_alpha: false,
        }
    }
}

impl From<&FileProfile> for ContentCostContext {
    fn from(p: &FileProfile) -> Self {
        Self {
            lossy_probe_bpp: p.lossy_probe_bpp,
            has_alpha: p.has_alpha,
        }
    }
}

/// The result of running a candidate through a backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateResult {
    pub candidate: Candidate,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_path: Option<std::path::PathBuf>,
    pub output_size: u64,
    pub processing_time_ms: u64,
    pub backend_name: String,
    /// Score assigned by the scoring engine. None = not yet scored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Quality metrics (None for lossless or if backend couldn't measure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<super::quality::QualityMetrics>,
}

/// A candidate the engine decided not to test, or tested and rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedCandidate {
    pub candidate: Candidate,
    pub reason: String,
}

/// Generates candidates from a strategy + profile. Pure function of
/// (strategy, profile, heuristics) — no I/O.
#[derive(Debug, Default)]
pub struct CandidateGenerator {
    heuristics: OptimizationHeuristics,
}

impl CandidateGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_heuristics(heuristics: OptimizationHeuristics) -> Self {
        Self { heuristics }
    }

    /// Produce the list of candidates to test, after heuristic pruning.
    pub fn generate(
        &self,
        profile: &FileProfile,
        strategy: &OptimizationStrategy,
    ) -> (Vec<Candidate>, Vec<RejectedCandidate>) {
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut rejected: Vec<RejectedCandidate> = Vec::new();
        let mut seen: HashSet<(Format, Option<u8>, bool)> = HashSet::new();

        for &format in &strategy.candidate_formats {
            // Candidato ITERATIVO para lossy JPEG/WebP (y AVIF bajo
            // MaximumCompression, que sí admite alpha). Con iterative no
            // se emite la escalera de calidades de estos formatos (el
            // search la cubre y con métrica perceptual — tener ambos es
            // redundante y caro), pero el LOSSLESS del formato SÍ se
            // genera: WebP lossless suele ser el mejor candidato para
            // logos/screenshots bajo cualquier goal.
            let format_uses_iterative = strategy.goal.uses_iterative_search()
                && strategy.include_lossy
                && ((matches!(format, Format::Jpeg | Format::Webp)
                    && !profile.has_alpha) // lossy JPEG no soporta alpha
                    || (format == Format::Avif
                        && matches!(
                            strategy.goal,
                            crate::engine::intel::goal::OptimizationGoal::MaximumCompression
                        )));

            if format_uses_iterative {
                let mut cand =
                    self.build_candidate(
                        next_candidate_id(),
                        format,
                        None,
                        false,
                        strategy,
                        profile,
                    );
                cand.iterative = true;
                cand.label = format!("{} iterative", format.as_str());
                // El search hace 4-10 codificaciones completas +
                // evaluación butteraugli por escalón: coste real ~6× el
                // de una codificación.
                cand.estimated_cost_ms = cand.estimated_cost_ms.saturating_mul(6);
                if seen.insert((cand.format, cand.quality, cand.lossless)) {
                    match self.heuristics.should_test(profile, &cand, strategy) {
                        Ok(()) => candidates.push(cand),
                        Err(reason) => rejected.push(RejectedCandidate {
                            candidate: cand,
                            reason,
                        }),
                    }
                }
            }

            // Candidato "source-quality" para fuente JPEG: re-encode con
            // las quant tables del ORIGINAL (estilo jpegtran) — el mejor
            // candidato calidad/tiempo para JPEGs, sin elegir quality.
            // El pipeline mide su SSIM real (~0.999) porque el roundtrip
            // no es bit-exacto. Con goal iterative es redundante (el
            // search ya explora q cercanas al original).
            if !strategy.goal.uses_iterative_search()
                && strategy.include_lossy
                && format == Format::Jpeg
                && profile.format == Format::Jpeg
            {
                let cand = self.build_candidate(
                        next_candidate_id(),
                        format,
                        None,
                        false,
                        strategy,
                        profile,
                    );
                let cand = Candidate {
                    label: format!("{} source-quality", format.as_str()),
                    ..cand
                };
                if seen.insert((cand.format, cand.quality, cand.lossless)) {
                    match self.heuristics.should_test(profile, &cand, strategy) {
                        Ok(()) => candidates.push(cand),
                        Err(reason) => rejected.push(RejectedCandidate {
                            candidate: cand,
                            reason,
                        }),
                    }
                }
            }

            // Lossless candidate for this format (always included if
            // strategy.include_lossless and the format supports it).
            if strategy.include_lossless && supports_lossless_pixels(format, profile.format) {
                let cand = self.build_candidate(
                        next_candidate_id(),
                        format,
                        None,
                        true,
                        strategy,
                        profile,
                    );
                if seen.insert((cand.format, cand.quality, cand.lossless)) {
                    match self.heuristics.should_test(profile, &cand, strategy) {
                        Ok(()) => candidates.push(cand),
                        Err(reason) => rejected.push(RejectedCandidate {
                            candidate: cand,
                            reason,
                        }),
                    }
                }
            }

            // Candidatos lossy del rango de calidades. Para formatos
            // iterativos se emiten solo los 3 niveles MÁS BARATOS como
            // anclas: el search optimiza butteraugli (no el score del
            // goal), y si converge alto o no alcanza el target, el pool
            // necesita candidatos baratos del formato. Los demás goals
            // toman la escalera completa.
            if strategy.include_lossy {
                if let Some(range) = strategy.quality_range_for(format) {
                    let anchor_cap = if format_uses_iterative { 3 } else { usize::MAX };
                    // Poda ex-ante AVIF: la calidad estimada es PLANA por
                    // década (q80 y q85 estiman lo mismo) → dos calidades
                    // de la misma década empatan en quality_score pero la
                    // mayor tiene siempre más tamaño y coste: dominada.
                    // Solo AVIF — WebP/JPEG miden SSIM real y dos
                    // calidades de la misma década difieren.
                    let mut seen_avif_decades: HashSet<u8> = HashSet::new();
                    // `iter_values()` es ascendente: `.take(anchor_cap)`
                    // se queda con los niveles MÁS BARATOS cuando el
                    // formato es iterativo (anclas).
                    for q in range.iter_values().into_iter().take(anchor_cap) {
                        if format == Format::Avif
                            && !seen_avif_decades.insert(q / 10)
                        {
                            let dominated = self.build_candidate(
                                next_candidate_id(),
                                format,
                                Some(q),
                                false,
                                strategy,
                                profile,
                            );
                            rejected.push(RejectedCandidate {
                                candidate: dominated,
                                reason: format!(
                                    "AVIF q{q}: same estimated-quality decade as a lower q \
                                     (curve is flat per decade) — dominated ex-ante, pruned"
                                ),
                            });
                            continue;
                        }
                        let cand = self.build_candidate(
                            next_candidate_id(),
                            format,
                            Some(q),
                            false,
                            strategy,
                            profile,
                        );
                        if seen.insert((cand.format, cand.quality, cand.lossless)) {
                            match self.heuristics.should_test(profile, &cand, strategy) {
                                Ok(()) => candidates.push(cand),
                                Err(reason) => rejected.push(RejectedCandidate {
                                    candidate: cand,
                                    reason,
                                }),
                            }
                        }
                    }
                }
            }
        }

        // Honor max_candidates. Keep highest-priority (first) ones.
        if candidates.len() > strategy.max_candidates {
            let dropped = candidates.split_off(strategy.max_candidates);
            for c in dropped {
                rejected.push(RejectedCandidate {
                    candidate: c,
                    reason: "Exceeded max_candidates; dropped by priority cap".into(),
                });
            }
        }

        (candidates, rejected)
    }

    fn build_candidate(
        &self,
        id: CandidateId,
        format: Format,
        quality: Option<u8>,
        lossless: bool,
        strategy: &OptimizationStrategy,
        profile: &FileProfile,
    ) -> Candidate {
        let pixels = u64::from(profile.width) * u64::from(profile.height);
        let photo_like = profile.is_photo_like();
        let label = match (format, quality, lossless) {
            (f, None, true) => format!("{} lossless", f.as_str()),
            (f, Some(q), false) => format!("{} q{}", f.as_str(), q),
            (f, _, _) => format!("{}", f.as_str()),
        };
        let effort = self.estimate_effort(format, quality, lossless, strategy);
        let estimated_cost_ms = static_cost_ms(
            format,
            quality,
            lossless,
            pixels,
            effort,
            ContentCostContext::from(profile),
        );
        Candidate {
            id,
            format,
            quality,
            lossless,
            resize: strategy.resize,
            metadata_mode: strategy.metadata_mode,
            preserve_color_profile: strategy.preserve_color_profile,
            estimated_effort: effort,
            estimated_cost_ms,
            label,
            memory_budget_mb: strategy.memory_budget_mb,
            // El generador lo sobrescribe a true para lossy JPEG/WebP/AVIF
            // cuando el goal usa iterative search.
            iterative: false,
            // Del FileProfile — decide la curva de estimación AVIF
            // (foto vs gráfico) en el scorer.
            photo_like,
            // 0 = sin límite; el pipeline lo fija al procesar.
            backend_time_budget_ms: 0,
        }
    }

    fn estimate_effort(
        &self,
        format: Format,
        quality: Option<u8>,
        lossless: bool,
        strategy: &OptimizationStrategy,
    ) -> f32 {
        // AVIF is the most expensive, JPEG the cheapest.
        let mut base: f32 = match format {
            Format::Avif => 0.9,
            Format::Webp => 0.6,
            Format::Png if lossless => 0.5, // oxipng can be slow at high presets
            Format::Png => 0.3,
            Format::Jpeg => 0.2,
            Format::Gif | Format::Bmp | Format::Tiff => 0.3,
            Format::Unknown => 0.5,
        };
        // MaximumCompression sube el effort de los candidatos lossless
        // de PNG a 0.85 para que el backend alcance el level 6 de oxipng
        // (zopfli).
        if lossless
            && format == Format::Png
            && matches!(
                strategy.goal,
                crate::engine::intel::goal::OptimizationGoal::MaximumCompression
            )
        {
            base = base.max(0.85_f32);
        }
        let q_factor = quality.map(|q| 1.0 - (q as f32 / 200.0)).unwrap_or(0.5);
        (base + q_factor * 0.2).clamp(0.0, 1.0)
    }
}

/// Coste estático (ms) de un candidato en la máquina de referencia (2
/// cores, rav1e 2 threads). Función PURA de (formato, calidad, lossless,
/// píxeles, contenido): misma entrada → mismo valor, siempre. El scorer
/// la usa como componente de velocidad para que el ranking sea
/// reproducible; el tiempo MEDIDO se reporta en
/// `CandidateResult.processing_time_ms` pero no decide la selección.
///
/// Constantes por MP: JPEG (mozjpeg, progressive) ≈ 150, WebP lossy
/// (m6 + sharp_yuv) ≈ 550, WebP lossless ≈ 900, PNG oxipng level 4-5
/// ≈ 350, PNG zopfli 15 iter ≈ 4000. Los candidatos ITERATIVOS
/// multiplican por ~6 (4-10 codificaciones + medición por escalón).
///
/// Modelo AVIF: el predictor es el bpp del probe WebP q75 del analyzer
/// (coste marginal cero — ya se hace ese encode). La escala es plana en
/// MP; la calidad importa (rav1e hace más RDO con más bits); el alpha
/// codifica 2 planos (×2.5) y la contención del pipeline (2 cores
/// compartidos) ×1.8. Sin sonda: 2000 ms/MP conservador.
fn static_cost_ms(
    format: Format,
    quality: Option<u8>,
    lossless: bool,
    pixels: u64,
    effort: f32,
    ctx: ContentCostContext,
) -> u64 {
    let mps = (pixels as f64 / 1_000_000.0).max(0.01);
    let per_mp: f64 = match format {
        Format::Avif => {
            // Base q50 por contenido (en solitario, speed 4).
            let base_q50 = match ctx.lossy_probe_bpp {
                Some(bpp) => {
                    let bpp = (bpp as f64).clamp(0.01, 8.0);
                    let mut b = 1014.0 * bpp.powf(0.28);
                    if ctx.has_alpha {
                        b *= 2.5; // encode_rgba: plano alpha completo
                    }
                    // Contención percibida bajo pipeline (2 cores
                    // compartidos con otros candidatos).
                    b * 1.8
                }
                None => 2_000.0, // sin sonda: p75 de las mediciones × contención
            };
            // Multiplicador de calidad: rav1e hace más RDO con más
            // bits. q50 → 1.0, q60 → 1.73, q80 → 4.49, cap q87+ → 8.
            let q = quality.unwrap_or(75) as f64;
            let q_mult = ((q + 50.0) / 100.0).powf(5.8).clamp(1.0, 8.0);
            (base_q50 * q_mult).clamp(400.0, 15_000.0)
        }
        Format::Webp if lossless => 900.0,
        Format::Webp => 550.0,
        Format::Png if lossless && effort >= 0.8 => 4_000.0, // zopfli (level 6)
        Format::Png if lossless => 350.0,
        Format::Png => 350.0,
        Format::Jpeg => 150.0,
        _ => 500.0,
    };
    // La calidad afecta poco al coste salvo en WebP (m6 recorre más
    // filtros con más detalle) — factor suave 0.9..1.1.
    let q_factor = if format == Format::Avif {
        1.0 // AVIF ya modela la calidad en `q_mult`
    } else {
        quality.map(|q| 0.9 + (100 - q) as f64 / 500.0).unwrap_or(1.0)
    };
    let mut cost = mps * per_mp * q_factor;
    // Iterativos: la búsqueda hace 4-10 codificaciones + evaluación
    // por escalón; el caller lo compensa multiplicando externamente
    // (los candidatos se marcan iterative después de construirse).
    cost = cost.max(1.0);
    cost.min(u32::MAX as f64) as u64
}

/// Formats for which the configured backend can preserve decoded pixels.
/// Other format adapters use a re-encode and must never be advertised as
/// lossless merely because no quality value was supplied.
///
/// JPEG no está en la lista: el roundtrip IDCT→FDCT de mozjpeg-rs no es
/// bit-exacto — el re-encode a calidad de origen se ofrece como
/// candidato lossy medido, no como lossless etiquetado.
fn supports_lossless_pixels(format: Format, source_format: Format) -> bool {
    let _ = source_format;
    matches!(format, Format::Png | Format::Webp)
}
