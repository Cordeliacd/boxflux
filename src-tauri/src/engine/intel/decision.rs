//! OptimizationDecision — the explainable outcome of an optimization run.
//!
//! Contains: the selected candidate, all measured candidates (with
//! scores), all rejected candidates (with reasons), and a
//! human-readable explanation generated from actual measurements.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::candidate::{CandidateResult, RejectedCandidate};
use super::profile::FileProfile;
use super::strategy::OptimizationStrategy;

/// The full explainable decision produced by the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationDecision {
    /// The input file that was optimized.
    pub input_path: PathBuf,
    /// The selected best candidate's result. None if all candidates failed.
    pub selected: Option<CandidateResult>,
    /// All candidates that were tested (including the selected one).
    pub all_results: Vec<CandidateResult>,
    /// Candidates that were rejected before testing (by heuristics) or
    /// after testing (by the scorer).
    pub rejected: Vec<RejectedCandidate>,
    /// The strategy that produced this decision.
    pub strategy_summary: String,
    /// The file profile that was analyzed.
    pub profile_summary: String,
    /// Human-readable explanation of why this candidate was selected.
    pub explanation: String,
    /// Original file size, for reference.
    pub original_size: u64,
    /// Final output size (selected candidate's size), or original if none.
    pub final_size: u64,
    /// True if no candidate improved on the original (kept input as-is).
    pub kept_original: bool,
    /// `true` si el pipeline NO pudo procesar el archivo (error de
    /// análisis, directorio de salida inaccesible, etc.).
    ///
    /// Distinto de `kept_original`: "kept original" es una DECISIÓN
    /// válida (ningún candidato superó al original — el archivo ya
    /// estaba optimizado); `failed` es un ERROR que la cola debe
    /// reportar como job `Failed`, no como `Completed`.
    #[serde(default)]
    pub failed: bool,
}

impl OptimizationDecision {
    /// Build a decision from the pipeline's components.
    pub fn build(
        input_path: PathBuf,
        original_size: u64,
        profile: &FileProfile,
        strategy: &OptimizationStrategy,
        mut results: Vec<CandidateResult>,
        rejected: Vec<RejectedCandidate>,
    ) -> Self {
        // Score all results. Collect into owned data to release the
        // borrow on `results` before we mutate it below.
        let scorer = super::scoring::CandidateScoringEngine::new();
        let (ranked, rejected_by_score) = scorer.rank(&results, original_size, strategy);

        // Materialize ranked into owned (CandidateId, score) pairs so
        // we no longer hold a borrow on `results`.
        let ranked_owned: Vec<(u32, f64)> =
            ranked.iter().map(|(r, s)| (r.candidate.id, *s)).collect();

        // The original is the baseline: an output that is not smaller must
        // never suppress a lower-ranked candidate that does save space.
        //
        // Lossless preference: among the candidates that beat the original,
        // if the smallest lossless result is no larger than the smallest
        // lossy result, prefer the lossless one — even if the lossy
        // candidate scored higher (typically on speed). Rationale: a
        // lossless output that is the same size or smaller is strictly
        // better than a lossy output of the same size, because the
        // lossless output preserves bit-exact pixels and there is no
        // future re-encode penalty.
        let heuristics = super::heuristics::OptimizationHeuristics::default();
        let default_choice = ranked
            .iter()
            .find(|(result, _)| result.output_size < original_size)
            .copied();
        let selected_ranked = Self::apply_lossless_preference(&ranked, original_size, &heuristics);
        let kept_original = selected_ranked.is_none();
        let heuristic_fired = match (selected_ranked, default_choice) {
            (Some((sel, _)), Some((def, _))) => sel.candidate.id != def.candidate.id,
            _ => false,
        };

        let (selected, explanation) = if kept_original {
            (
                None,
                format!(
                    "No candidate improved on the original {} bytes. Keeping the input unchanged.",
                    original_size
                ),
            )
        } else if let Some((result, score)) = selected_ranked {
            let selected = (*result).clone();
            let pref_note = if heuristic_fired {
                " Lossless preferred over a higher-scoring lossy candidate (lossless-if-smaller heuristic)."
            } else {
                ""
            };
            // Only report alternatives that also beat the original size,
            // excluding the SELECTED candidate (not just "skip first").
            //
            // Excluir por id y no con `skip(1)`: cuando la heurística
            // lossless-if-smaller sobreescribe la selección por score, el
            // elegido puede estar en cualquier posición del ranking —
            // `skip(1)` dejaría fuera al primero (que puede no ser el
            // elegido) e incluso listar al propio elegido como runner-up.
            let selected_id = selected.candidate.id;
            let runners_up: Vec<(String, f64)> = ranked
                .iter()
                .filter(|(candidate, _)| {
                    candidate.output_size < original_size && candidate.candidate.id != selected_id
                })
                .map(|(r, s)| (r.candidate.label.clone(), *s))
                .collect();
            let mut why = Self::explain_selection_from_owned(
                &selected,
                score,
                &runners_up,
                strategy,
                original_size,
            );
            why.push_str(pref_note);
            (Some(selected), why)
        } else {
            let why = if results.iter().all(|r| !r.success) {
                format!(
                    "All {} candidates failed. Keeping the input unchanged.",
                    results.len()
                )
            } else {
                "All candidates were rejected by the scorer. Keeping the input unchanged.".into()
            };
            (None, why)
        };

        // Mark rejected-by-score with their reasons.
        let mut all_rejected = rejected;
        for (r, reason) in rejected_by_score.into_iter() {
            all_rejected.push(RejectedCandidate {
                candidate: r.candidate.clone(),
                reason,
            });
        }

        // Annotate scores on results (ranked borrow is now released).
        for r in &mut results {
            if let Some((_, score)) = ranked_owned.iter().find(|(id, _)| *id == r.candidate.id) {
                r.score = Some(*score);
            }
        }

        let final_size = selected
            .as_ref()
            .map(|r| r.output_size)
            .unwrap_or(original_size);

        Self {
            input_path,
            selected,
            all_results: results,
            rejected: all_rejected,
            strategy_summary: strategy.rationale.clone(),
            profile_summary: profile.summary(),
            explanation,
            original_size,
            final_size,
            kept_original,
            // `build` solo se llama con resultados procesados: si llegamos
            // aquí el archivo se pudo analizar y el pipeline corrió. Un
            // "kept original" aquí es decisión válida, no error.
            failed: false,
        }
    }

    /// Apply the lossless-if-smaller heuristic to the ranked list.
    ///
    /// Returns the candidate to select, or None if no candidate beat the
    /// original. The default choice is the highest-scoring candidate that
    /// beats the original size. If a lossless candidate exists that is no
    /// larger than the smallest lossy candidate that also beats the
    /// original, the lossless candidate is preferred — even if a lossy
    /// candidate scored higher overall.
    fn apply_lossless_preference<'a>(
        ranked: &'a [(&'a CandidateResult, f64)],
        original_size: u64,
        heuristics: &super::heuristics::OptimizationHeuristics,
    ) -> Option<(&'a CandidateResult, f64)> {
        // Default: highest-scoring candidate that beats the original.
        let default = ranked
            .iter()
            .copied()
            .find(|(result, _)| result.output_size < original_size);

        // Find the smallest lossy candidate that beats the original.
        let smallest_lossy = ranked
            .iter()
            .copied()
            .filter(|(r, _)| !r.candidate.lossless && r.output_size < original_size)
            .min_by_key(|(r, _)| r.output_size);

        // Find the smallest lossless candidate that beats the original.
        let smallest_lossless = ranked
            .iter()
            .copied()
            .filter(|(r, _)| r.candidate.lossless && r.output_size < original_size)
            .min_by_key(|(r, _)| r.output_size);

        // Apply the heuristic only when both a lossy and a lossless
        // candidate beat the original. If only lossless candidates beat
        // the original, the default selection (highest-scoring lossless)
        // is already correct. If only lossy candidates beat the original,
        // there is no lossless alternative to prefer.
        let (Some(lossy), Some(lossless)) = (smallest_lossy, smallest_lossless) else {
            return default;
        };

        if heuristics.prefer_lossless_if_smaller(lossless.0.output_size, lossy.0.output_size) {
            // Return the lossless candidate with its own score. The
            // explanation layer annotates this as a heuristic preference.
            Some(lossless)
        } else {
            default
        }
    }

    fn explain_selection_from_owned(
        selected: &CandidateResult,
        score: f64,
        runners_up: &[(String, f64)],
        strategy: &OptimizationStrategy,
        original_size: u64,
    ) -> String {
        let cand = &selected.candidate;
        let bytes_saved = original_size.saturating_sub(selected.output_size);
        let pct = if original_size > 0 {
            100.0 * bytes_saved as f64 / original_size as f64
        } else {
            0.0
        };

        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "{} was selected because it produced the highest score ({:.4}) under the {} goal.",
            cand.label,
            score,
            strategy.goal.name()
        ));
        parts.push(format!(
            "Output: {} bytes (saved {} bytes, {:.1}% reduction).",
            selected.output_size, bytes_saved, pct
        ));
        if let Some(q) = &selected.quality {
            if q.is_lossless {
                parts.push("Quality: lossless (identical to source).".into());
            } else {
                parts.push(format!(
                    "Quality: PSNR={:.2}dB, SSIM={:.4}.",
                    q.psnr, q.ssim
                ));
            }
        }
        parts.push(format!("Backend: {}.", selected.backend_name));
        if !runners_up.is_empty() {
            parts.push(format!(
                "Runner-up: {} (score {:.4}).",
                runners_up[0].0, runners_up[0].1
            ));
        }
        parts.join(" ")
    }

    /// Compact one-line summary for the UI.
    pub fn one_line_summary(&self) -> String {
        match &self.selected {
            Some(r) => format!(
                "→ {} ({} → {} bytes, {:.1}% saved)",
                r.candidate.label,
                self.original_size,
                r.output_size,
                if self.original_size > 0 {
                    100.0 * (self.original_size - r.output_size) as f64 / self.original_size as f64
                } else {
                    0.0
                }
            ),
            None => format!("→ kept original ({} bytes)", self.original_size),
        }
    }
}
