//! Scoring multi-factor: calidad, compresión (log), velocidad y
//! compatibilidad, normalizado a [0, 1.2]. Penaliza reducciones
//! triviales, salidas más grandes y calidad bajo umbral.

use super::candidate::CandidateResult;
use super::metrics::ResultMetrics;
use super::quality::BUTTERAUGLI_SSIM_LOW;
use super::strategy::OptimizationStrategy;

/// Guardarrail SSIM del gate perceptual (modo "Comprimir al Máximo").
/// El criterio de aceptación real es butteraugli medido ≤ target del
/// goal; este floor solo evita aprobar candidatos catastróficos cuyo
/// daño estructural (SSIM < 0.55 ≈ "otra imagen") ni el propio modo
/// toleraría. Los candidatos extremos legítimos miden SSIM 0.6-0.9.
const PERCEPTUAL_FLOOR_SSIM: f64 = 0.55;

pub struct CandidateScoringEngine;

impl CandidateScoringEngine {
    pub fn new() -> Self {
        Self
    }

    /// Score without access to the candidate (no quality estimation).
    pub fn score(
        &self,
        result: &ResultMetrics,
        strategy: &OptimizationStrategy,
    ) -> Result<f64, String> {
        self.score_with_candidate(result, None, strategy)
    }

    /// Score with explicit access to the `Candidate` (for quality estimation).
    ///
    /// This is the variant used by `rank()` which has access to the
    /// full `CandidateResult` (including the candidate's format + quality
    /// parameter). For formats whose quality cannot be measured (AVIF),
    /// we estimate from the candidate's `quality` field.
    pub fn score_with_candidate(
        &self,
        result: &ResultMetrics,
        candidate: Option<&super::candidate::Candidate>,
        strategy: &OptimizationStrategy,
    ) -> Result<f64, String> {
        let weights = &strategy.weights;
        if !weights.quality_weight.is_finite()
            || !weights.compression_weight.is_finite()
            || !weights.speed_weight.is_finite()
            || !weights.min_quality_threshold.is_finite()
            || weights.quality_weight < 0.0
            || weights.compression_weight < 0.0
            || weights.speed_weight < 0.0
            || !(0.0..=1.0).contains(&weights.min_quality_threshold)
        {
            return Err("invalid scoring weights or quality threshold".into());
        }

        // ---------- 1. Quality ----------
        let quality_score = self.estimate_quality_with_candidate(result, candidate, strategy);

        // Reject below-threshold lossy candidates. Lossless candidates
        // always pass (quality = 1.0).
        //
        // Los iterativos ya fueron validados por la métrica perceptual
        // (butteraugli contra un target adaptativo por imagen):
        // exigirles además el threshold de SSIM del goal es un doble
        // gate contradictorio — en una imagen ruidosa el target se
        // relaja (×1.3) y el óptimo puede quedar en SSIM ~0.87, bajo un
        // threshold 0.90, y todo el trabajo de la búsqueda se perdía.
        //
        // En modo perceptual la aceptación real la decide el daño
        // butteraugli MEDIDO (gate de abajo); el SSIM queda como
        // guardarrail bajo que rechaza basura absoluta sin matar a
        // los candidatos extremos legítimos que el modo existe para
        // producir.
        let iterative = candidate.map(|c| c.iterative).unwrap_or(false);
        let perceptual_gate = strategy.goal.uses_perceptual_gate();
        // En modo perceptual el guardarrail compara el SSIM medido, no
        // el quality_score (que incluye la penalty de ba>1.5 — eso es
        // información de ranking, no de aceptación: castigar dos veces
        // el mismo daño que el gate ya aprobó empujaba bajo el floor a
        // candidatos legítimos). Sin medición cae al score estimado.
        let structural_score = if perceptual_gate {
            result.quality.as_ref().map(|q| q.ssim).unwrap_or(quality_score)
        } else {
            quality_score
        };
        let threshold = if perceptual_gate {
            PERCEPTUAL_FLOOR_SSIM
        } else if iterative {
            BUTTERAUGLI_SSIM_LOW
        } else {
            weights.min_quality_threshold as f64
        };
        if !result.is_lossless() && structural_score < threshold - 1e-6 {
            return Err(format!(
                "quality {:.3} below threshold {:.3}",
                structural_score, threshold
            ));
        }

        // Gate perceptual del modo "Comprimir al Máximo": butteraugli
        // medido ≤ límite del modo (<1.0 idéntico, 1.0-2.0 apenas
        // perceptible, 2.0-2.5 visible pero tolerable). Para iterativos
        // el límite es el clamp máximo del target adaptativo (3.0): la
        // búsqueda ya validó contra el target del goal AJUSTADO por
        // perfil — re-aplicar aquí el target base invalidaría lo que
        // la búsqueda convergó de buena fe.
        if perceptual_gate && !result.is_lossless() {
            if let Some(ba) = result.quality.as_ref().and_then(|q| q.butteraugli) {
                let limit = if iterative {
                    // clamp máximo de `adaptive_butteraugli_target`
                    3.0
                } else {
                    strategy.goal.butteraugli_target()
                };
                if ba > limit + 1e-9 {
                    return Err(format!(
                        "perceptual gate: butteraugli {ba:.2} exceeds mode limit {limit:.2}"
                    ));
                }
            }
        }

        // ---------- 2. Compression efficiency (logarithmic) ----------
        // log10(1 + 9*ratio): rewards first 50% of savings more than 90%→95%.
        //   ratio=0.0 → 0.0, ratio=0.5 → 0.74, ratio=1.0 → 1.0
        // Output grew → heavy penalty (0.3×).
        let ratio = (result.percentage_saved / 100.0).clamp(-0.5, 1.0);
        let compression_score = if ratio <= 0.0 {
            (1.0 + ratio).max(0.0) * 0.3
        } else {
            (ratio * 9.0 + 1.0).log10()
        };

        // ---------- 3. Speed ----------
        // Speed score DETERMINISTA: se puntúa el COSTE ESTIMADO estático
        // del candidato, no el tiempo medido — el tiempo real varía con
        // la carga del sistema y hacía el ranking no reproducible (el
        // mismo candidato puntuaba distinto entre runs). El tiempo
        // medido sigue reportándose en
        // `CandidateResult.processing_time_ms` y alimenta la válvula
        // anti-runaway del pipeline; solo deja de decidir la selección.
        // estimated_cost_ms == 0 (candidatos a mano): coste despreciable.
        let speed_score = match candidate {
            Some(c) if c.estimated_cost_ms > 0 => {
                (1.0 - (c.estimated_cost_ms as f64 / 5000.0)).clamp(0.0, 1.0)
            }
            _ => 1.0,
        };

        // ---------- 4. Format compatibility (tie-breaker) ----------
        let compat_score = candidate
            .map(|c| format_preference_score(c.format, strategy))
            .unwrap_or(0.0);

        // ---------- Combine ----------
        let total_w =
            (weights.quality_weight + weights.compression_weight + weights.speed_weight) as f64;
        let total_w = if total_w > 0.0 { total_w } else { 1.0 };

        let mut score = (quality_score * weights.quality_weight as f64
            + compression_score * weights.compression_weight as f64
            + speed_score * weights.speed_weight as f64)
            / total_w;

        // Small compatibility bonus (capped at 0.05).
        score += compat_score * weights.compatibility_weight as f64 * 0.05;

        // El score de compresión log10(1+9r) ya es monótono y con
        // retornos decrecientes — la forma correcta de premiar el
        // ahorro. Un bonus de "sweet spot" por rango de ahorro rompería
        // la monotonía: menos ahorro no debe puntuar más.

        // Bonus por compresión excepcional (>80% de ahorro): ayuda al
        // MaximumCompression a elegir AVIF q40 (96% ahorrado) sobre
        // WebP q80 (85%) aunque este tenga mejor calidad. 0.10 a 80%,
        // hasta 0.20 a 95%+.
        // Reward candidates that achieve dramatic size reductions. This
        // helps MaximumCompression goal pick AVIF q40 (96% saved) over
        // WebP q80 (85% saved) even if the latter has better quality.
        // Bonus is 0.10 at 80% savings, scaling up to 0.20 at 95%+.
        if ratio >= 0.80 {
            let bonus = ((ratio - 0.80) / 0.20).clamp(0.0, 1.0) * 0.20;
            score += bonus;
        }

        score = score.clamp(0.0, 1.2);

        // ---------- Penalties ----------
        // Reducción marginal (<10%): un candidato que solo reduce 5% no
        // debe ganar contra uno que reduce 50% aunque la calidad sea
        // ligeramente mejor — el usuario quiere ver ahorro real.
        let reduction_ratio = result.percentage_saved / 100.0;
        if !result.is_lossless() && reduction_ratio < 0.10 {
            // Penalty lineal: 0% savings → 0.5×, 10% savings → 1.0×.
            let penalty = (0.5 + reduction_ratio * 5.0).clamp(0.5, 1.0);
            return Ok(score * penalty);
        }
        // Penalty intermedio para reducciones 10-20%: el usuario aprecia
        // algo de ahorro pero esperamos más.
        if !result.is_lossless() && reduction_ratio < 0.20 {
            let penalty = 0.7 + (reduction_ratio - 0.10) * 3.0; // 0.7 → 1.0
            return Ok(score * penalty.clamp(0.7, 1.0));
        }

        Ok(score)
    }

    /// Estimate quality for candidates without measured metrics.
    ///
    /// Para AVIF sin métricas se estima desde el parámetro de calidad
    /// del candidato con curvas calibradas por tipo de contenido: fotos
    /// (SSIM con acantilado empinado — landscape q60 → 0.726 / q80 →
    /// 0.947) vs gráficos (SSIM 0.95+ estable en todo el rango q40-q80).
    /// Los valores son lower bounds conservadores por bucket de calidad:
    /// el error debe ir en dirección de rechazar, no de aprobar.
    /// q<40 es extrapolación conservadora por el mismo motivo.
    fn estimate_quality_with_candidate(
        &self,
        result: &ResultMetrics,
        candidate: Option<&super::candidate::Candidate>,
        strategy: &OptimizationStrategy,
    ) -> f64 {
        if let Some(q) = &result.quality {
            // Iterativos → score perceptual (sin el componente PSNR):
            // la búsqueda ya los validó con butteraugli y el PSNR
            // castigaría el denoising que la métrica perceptual aprueba.
            if let Some(cand) = candidate {
                if cand.iterative {
                    return q.iterative_normalized_score();
                }
            }
            return q.normalized_score();
        }
        if result.is_lossless() {
            return 1.0;
        }
        let Some(cand) = candidate else {
            return 0.85;
        };
        let q = cand.quality.unwrap_or(75);
        match cand.format {
            crate::engine::formats::Format::Avif => {
                // Dos curvas calibradas (foto vs gráfico). La decisión
                // la toma la SONDA de sensibilidad lossy del strategy
                // (SSIM del roundtrip WebP q75 del preview, medido por
                // el analyzer): < 0.955 → foto (acantilado), ≥ 0.955 →
                // gráfico (estable). Sin sonda → `Candidate::photo_like`
                // (clasificación por categoría).
                let photo_sensitive = match strategy.lossy_probe_ssim {
                    Some(probe) => probe < 0.955,
                    None => cand.photo_like,
                };
                if photo_sensitive {
                    match q {
                        90..=100 => 0.93,
                        80..=89 => 0.85,
                        70..=79 => 0.78,
                        60..=69 => 0.72,
                        50..=59 => 0.71,
                        40..=49 => 0.70,
                        30..=39 => 0.68,
                        _ => 0.62,
                    }
                } else {
                    match q {
                        90..=100 => 0.99,
                        80..=89 => 0.99,
                        70..=79 => 0.98,
                        60..=69 => 0.98,
                        50..=59 => 0.97,
                        40..=49 => 0.95,
                        30..=39 => 0.90,
                        _ => 0.75,
                    }
                }
            }
            crate::engine::formats::Format::Webp => {
                if q >= 90 {
                    0.95
                } else if q >= 80 {
                    0.92
                } else if q >= 70 {
                    0.88
                } else if q >= 60 {
                    0.84
                } else {
                    0.78
                }
            }
            crate::engine::formats::Format::Jpeg => {
                if q >= 95 {
                    0.96
                } else if q >= 85 {
                    0.92
                } else if q >= 75 {
                    0.88
                } else if q >= 65 {
                    0.84
                } else {
                    0.78
                }
            }
            _ => 0.85,
        }
    }

    /// Score and rank all candidate results. Returns the sorted list
    /// (highest score first) plus a list of rejected candidates with
    /// reasons. The original input "do nothing" option is included as
    /// a baseline with score 0.
    pub fn rank<'a>(
        &self,
        results: &'a [CandidateResult],
        original_size: u64,
        strategy: &OptimizationStrategy,
    ) -> (
        Vec<(&'a CandidateResult, f64)>,
        Vec<(&'a CandidateResult, String)>,
    ) {
        let mut ranked: Vec<(&'a CandidateResult, f64)> = Vec::new();
        let mut rejected: Vec<(&'a CandidateResult, String)> = Vec::new();

        // First pass: compute scores and collect rejections.
        let scored: Vec<(usize, Result<f64, String>)> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                if !r.success {
                    return (
                        i,
                        Err(format!(
                            "backend error: {}",
                            r.error.clone().unwrap_or_default()
                        )),
                    );
                }
                let metrics = ResultMetrics::from_sizes(
                    original_size,
                    r.output_size,
                    r.processing_time_ms,
                    r.quality.clone(),
                );
                // Use score_with_candidate so AVIF gets estimated quality.
                let score = self.score_with_candidate(&metrics, Some(&r.candidate), strategy);
                (i, score)
            })
            .collect();

        for (i, res) in scored {
            match res {
                Ok(s) => ranked.push((&results[i], s)),
                Err(reason) => rejected.push((&results[i], reason)),
            }
        }
        // Orden total determinista: score desc; empate exacto → menor
        // tamaño; si persiste → menor id. El ranking es independiente
        // del orden de entrada y reproducible entre runs.
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.output_size.cmp(&b.0.output_size))
                .then_with(|| a.0.candidate.id.cmp(&b.0.candidate.id))
        });

        // Filtro de dominancia Pareto: elimina del ranking todo
        // candidato estrictamente dominado por otro (size ≥ y
        // quality ≤ dominador, con al menos una desigualdad estricta)
        // — seleccionar un dominado es pérdida pura en ambos ejes.
        // La calidad se compara vía normalized_score medido/estimado.
        // Coste O(n²) con n ≤ ~16: despreciable frente a los códecs.
        // No prefiere lossless per se — solo elimina perdedores
        // objetivos (la preferencia explícita vive en
        // `decision::apply_lossless_preference`).
        if ranked.len() > 1 {
            let qualities: Vec<f64> = ranked
                .iter()
                .map(|(r, _)| {
                    if let Some(q) = &r.quality {
                        q.normalized_score()
                    } else if r.candidate.lossless {
                        1.0
                    } else {
                        // AVIF sin métrica: usar la estimación calibrada
                        // del scorer para que la comparación sea justa.
                        let metrics = ResultMetrics::from_sizes(
                            original_size,
                            r.output_size,
                            r.processing_time_ms,
                            None,
                        );
                        self.estimate_quality_with_candidate(&metrics, Some(&r.candidate), strategy)
                    }
                })
                .collect();

            let dominated: Vec<bool> = (0..ranked.len())
                .map(|i| {
                    (0..ranked.len()).any(|j| {
                        if i == j {
                            return false;
                        }
                        let (size_i, size_j) = (ranked[i].0.output_size, ranked[j].0.output_size);
                        let (q_i, q_j) = (qualities[i], qualities[j]);
                        // j domina a i: no peor en ningún eje, mejor en
                        // al menos uno. Estricto: tamaños/qualities
                        // idénticos NO cuentan como dominancia (evita
                        // eliminar candidatos empatados).
                        (size_j < size_i && q_j >= q_i - f64::EPSILON)
                            || (size_j <= size_i && q_j > q_i + f64::EPSILON)
                    })
                })
                .collect();

            let mut dominated_results: Vec<(&CandidateResult, String)> = Vec::new();
            let mut survivors: Vec<(&CandidateResult, f64)> = Vec::new();
            for ((result, score), is_dominated) in ranked.into_iter().zip(dominated) {
                if is_dominated {
                    dominated_results.push((
                        result,
                        "dominated by a smaller-or-equal candidate with equal-or-better quality"
                            .to_string(),
                    ));
                } else {
                    survivors.push((result, score));
                }
            }
            rejected.extend(dominated_results);
            ranked = survivors;
        }

        (ranked, rejected)
    }
}

fn format_preference_score(
    format: crate::engine::formats::Format,
    strategy: &OptimizationStrategy,
) -> f64 {
    let preferred = &strategy.weights.preferred_formats;
    match preferred.iter().position(|candidate| *candidate == format) {
        Some(position) if !preferred.is_empty() => {
            (preferred.len() - position) as f64 / preferred.len() as f64
        }
        _ => 0.0,
    }
}

impl Default for CandidateScoringEngine {
    fn default() -> Self {
        Self::new()
    }
}
