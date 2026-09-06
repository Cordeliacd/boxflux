//! Orquestador del pipeline: análisis → estrategia → generación de
//! candidatos → procesado/medición/calidad → scoring → selección →
//! publicación. Único componente que toca tanto los internals del motor
//! (analyzer, strategy, generator, scorer, heuristics) como los backends
//! de códecs.

use std::path::Path;
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::engine::formats::Format;

use super::analyzer::Analyzer;
use super::backends::BackendRegistry;
use super::backend::BackendResult;
use super::cancel::CancellationToken;
use super::candidate::{Candidate, CandidateResult, RejectedCandidate};
use super::decision::OptimizationDecision;
use super::goal::{OptimizationGoal, UserConstraints};
use super::heuristics::OptimizationHeuristics;
use super::original_cache::OriginalImageCache;
use super::profile::FileProfile;
use super::quality::QualityEvaluator;
use super::strategy::{OptimizationStrategy, StrategyEngine};

/// Configuration for the pipeline.
#[derive(Clone)]
pub struct PipelineConfig {
    /// Backend registry con los codecs disponibles.
    pub backends: BackendRegistry,
    pub heuristics: OptimizationHeuristics,
    /// If true, evaluate quality (MSE/PSNR/SSIM) for lossy candidates.
    /// Defaults to true. Disable for very large batches to save time.
    pub evaluate_quality: bool,
    /// Maximum candidates to process in parallel.
    pub parallel_candidates: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            backends: BackendRegistry::with_real_codecs(),
            heuristics: OptimizationHeuristics::default(),
            evaluate_quality: true,
            parallel_candidates: 4,
        }
    }
}

/// The pipeline. Stateless after construction — `optimize` takes all
/// per-call inputs as arguments.
pub struct OptimizationPipeline {
    config: PipelineConfig,
    analyzer: Analyzer,
    strategy_engine: StrategyEngine,
    quality_evaluator: QualityEvaluator,
}

impl OptimizationPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            config,
            analyzer: Analyzer::new(),
            strategy_engine: StrategyEngine::new(),
            quality_evaluator: QualityEvaluator::new(),
        }
    }

    /// Build a pipeline with the default real-codec configuration.
    pub fn with_real_codecs() -> Self {
        Self::new(PipelineConfig::default())
    }

    /// Lista los backends cargados en el pipeline.
    /// Útil para mostrar al usuario qué codecs están activos.
    pub fn list_backends(&self) -> Vec<(crate::engine::formats::Format, String)> {
        self.config.backends.list()
    }

    /// Run the full pipeline on a single file.
    ///
    /// `output_dir` is where candidate outputs are written; the selected
    /// candidate ends up at `output_dir/selected.<ext>`. Uses un token no
    /// cancelable — para cancelación cooperativa usar
    /// [`OptimizationPipeline::optimize_with_cancel`].
    pub fn optimize(
        &self,
        input: &Path,
        output_dir: &Path,
        goal: OptimizationGoal,
        constraints: &UserConstraints,
    ) -> OptimizationDecision {
        let token = CancellationToken::new();
        self.optimize_with_cancel(input, output_dir, goal, constraints, &token)
    }

    /// Run the full pipeline with cooperative cancellation.
    ///
    /// El token se consulta en las fronteras entre candidatos (un load
    /// atómico barato), nunca dentro de un codec. Tras una cancelación:
    /// el codec en vuelo termina pero su resultado se descarta, los
    /// candidatos pendientes no arrancan y la decisión es "kept
    /// original".
    pub fn optimize_with_cancel(
        &self,
        input: &Path,
        output_dir: &Path,
        goal: OptimizationGoal,
        constraints: &UserConstraints,
        token: &CancellationToken,
    ) -> OptimizationDecision {
        let profile = match self.analyzer.analyze(input) {
            Ok(p) => p,
            Err(e) => {
                let reason = format!("analysis failed: {e}");
                return self.fail_decision(input, profile_file_size(input), &goal, &reason);
            }
        };

        if token.is_cancelled() {
            return self.kept_original_decision(
                input,
                &profile,
                &goal,
                "cancellation requested before strategy planning",
            );
        }

        if profile.is_trivially_small() {
            return self.kept_original_decision(
                input,
                &profile,
                &goal,
                "file is trivially small; skipping optimization",
            );
        }

        let strategy = self
            .strategy_engine
            .plan(&profile, goal.clone(), constraints);

        let generator =
            super::candidate::CandidateGenerator::with_heuristics(self.config.heuristics.clone());
        let (candidates, rejected_by_heuristics) = generator.generate(&profile, &strategy);

        if candidates.is_empty() {
            // El rationale puede traer la razón de bloqueo de un formato
            // forzado embebida tras "ADVERTENCIA: " — extraerla para que
            // el usuario vea por qué no se pudo usar el formato elegido.
            let blocked_hint = strategy
                .rationale
                .split("ADVERTENCIA: ")
                .nth(1)
                .map(|s| s.to_string());
            let msg = if let Some(reason) = blocked_hint {
                format!(
                    "no se generaron candidatos para el formato solicitado. {}. \
                     (heurísticas rechazaron {} candidatos adicionales)",
                    reason,
                    rejected_by_heuristics.len()
                )
            } else {
                format!(
                    "no candidates generated; {} rejected by heuristics",
                    rejected_by_heuristics.len()
                )
            };
            return self.kept_original_decision(input, &profile, &goal, &msg);
        }

        if let Err(e) = std::fs::create_dir_all(output_dir) {
            return self.fail_decision(
                input,
                profile.file_size,
                &goal,
                &format!("cannot create candidate output directory: {e}"),
            );
        }

        // Cache por-run del original: decodifica lazy con el primer
        // candidato lossy y comparte el Arc con el resto.
        let original_cache = OriginalImageCache::new(input.to_path_buf(), profile.format);

        // Cache de deduplicación de encodes AVIF del run. Solo
        // MaximumCompression: es el único goal con search AVIF
        // iterativo, y por tanto el único donde anclas y rungs del
        // search codifican lo mismo dos veces.
        let dedup_cache = if strategy.goal
            == crate::engine::intel::goal::OptimizationGoal::MaximumCompression
        {
            super::encode_cache::EncodeDedupCache::new(output_dir)
        } else {
            super::encode_cache::EncodeDedupCache::disabled()
        };

        let original_size = profile.file_size;
        let results = self.process_candidates(
            &candidates,
            input,
            output_dir,
            &profile,
            &strategy,
            constraints,
            token,
            &original_cache,
            &dedup_cache,
        );

        let was_cancelled = token.is_cancelled();

        let mut decision = OptimizationDecision::build(
            input.to_path_buf(),
            original_size,
            &profile,
            &strategy,
            results,
            rejected_by_heuristics,
        );

        // Nunca publicar un resultado cancelado: aunque el scorer lo
        // hubiera rankeado primero, se descarta aquí.
        if was_cancelled {
            if let Some(selected) = decision.selected.take() {
                decision.rejected.push(RejectedCandidate {
                    candidate: selected.candidate,
                    reason: "cancellation requested; selected candidate discarded".into(),
                });
            }
            decision.final_size = decision.original_size;
            decision.kept_original = true;
            decision.explanation = format!(
                "Kept original because cancellation was requested during processing. {}",
                decision.explanation
            );
            // Sweep all candidate outputs — including the would-be-selected.
            cleanup_candidate_outputs(&decision.all_results);
            return decision;
        }

        // Publicación: copiar el output elegido a una ruta estable.
        // El usuario pudo cancelar entre selección y publicación.
        if token.is_cancelled() {
            if decision.selected.is_some() {
                if let Some(selected) = decision.selected.take() {
                    decision.rejected.push(RejectedCandidate {
                        candidate: selected.candidate,
                        reason: "cancellation requested before publication".into(),
                    });
                }
            }
            decision.final_size = decision.original_size;
            decision.kept_original = true;
            decision.explanation = format!(
                "Kept original because cancellation was requested before publication. {}",
                decision.explanation
            );
            cleanup_candidate_outputs(&decision.all_results);
            return decision;
        }

        // Los archivos intermedios de candidatos nunca deben sobrevivir
        // al run: filtran bytes lossy que el usuario no eligió y crecen
        // sin límite entre runs en el mismo output_dir.
        if let Some(sel) = decision.selected.as_ref() {
            let ext = extension_for(sel.candidate.format);
            let final_path = output_dir.join(format!("selected.{ext}"));
            // Segunda verificación inmediatamente antes de la copia:
            // cierra la ventana entre el check de arriba y el publish.
            // No se interrumpe una copia en vuelo — la defensa es no
            // empezarla.
            if token.is_cancelled() {
                if let Some(selected) = decision.selected.take() {
                    decision.rejected.push(RejectedCandidate {
                        candidate: selected.candidate,
                        reason: "cancellation requested immediately before publication".into(),
                    });
                }
                decision.final_size = decision.original_size;
                decision.kept_original = true;
                decision.explanation = format!(
                    "Kept original because cancellation was requested immediately before publication. {}",
                    decision.explanation
                );
                cleanup_candidate_outputs(&decision.all_results);
                super::output_safety::sweep_stale_temporaries(output_dir);
                return decision;
            }
            let publication = sel
                .output_path
                .as_deref()
                .ok_or_else(|| "selected candidate has no output path".to_string())
                .and_then(|src| publish_selected(src, &final_path, sel.candidate.id));
            if let Err(reason) = publication {
                if let Some(selected) = decision.selected.take() {
                    decision.rejected.push(RejectedCandidate {
                        candidate: selected.candidate,
                        reason: format!("selected output was not published: {reason}"),
                    });
                }
                decision.final_size = decision.original_size;
                decision.kept_original = true;
                decision.explanation = format!(
                    "Kept original because the selected candidate could not be published safely: {reason}"
                );
            }
        }

        // Limpieza best-effort: un fallo del OS al borrar no invalida
        // la decisión. Se barren también los temporales .boxflux-* que
        // hayan dejado runs anteriores que crashearon en el mismo
        // output_dir.
        cleanup_candidate_outputs(&decision.all_results);
        super::output_safety::sweep_stale_temporaries(output_dir);

        decision
    }

    /// Process all candidates in parallel, returning their results.
    /// Failures are recorded per-candidate; the pipeline continues.
    ///
    /// Cancellation: the token is checked before each candidate. If
    /// cancelled, the remaining candidates are recorded as
    /// cancelled-failures (not started). In-flight candidates are
    /// allowed to finish.
    fn process_candidates(
        &self,
        candidates: &[Candidate],
        input: &Path,
        output_dir: &Path,
        profile: &FileProfile,
        strategy: &OptimizationStrategy,
        constraints: &UserConstraints,
        token: &CancellationToken,
        original_cache: &OriginalImageCache,
        dedup_cache: &super::encode_cache::EncodeDedupCache,
    ) -> Vec<CandidateResult> {
        let time_budget = Duration::from_millis(strategy.max_processing_time_ms);
        // Presupuesto hard anti-runaway: el presupuesto del goal es guía
        // de planeación determinista (misma imagen + mismo goal → mismos
        // parámetros de códec → mismos bytes); el reloj de pared solo se
        // consulta como válvula de emergencia a 2× para cortar códecs
        // desbocados (zopfli/rav1e colgados) o carga extrema. Así el run
        // es reproducible bajo carga normal y degrada honestamente (con
        // razón explícita) bajo carga extrema.
        let hard_budget = time_budget
            .checked_mul(2)
            .unwrap_or(Duration::from_secs(600));
        let pipeline_start = Instant::now();
        let available = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1);
        let max_parallel = self.config.parallel_candidates.max(1).min(available);

        // Presupuesto por candidato: fracción FIJA del presupuesto del
        // goal (función pura de goal + imagen → mismo budget para el
        // mismo candidato siempre). Floor de 2 s para goals rápidos, cap
        // de 30 s para no dejar al resto de candidatos sin válvula de
        // emergencia.
        let backend_budget_ms = (strategy.max_processing_time_ms / 4)
            .clamp(2_000, 30_000);

        // Waves de exactamente `max_parallel` candidatos, cada una un
        // `par_iter` puro: sin locks bloqueantes dentro del par_iter,
        // porque los códecs (oxipng) usan Rayon internamente y un thread
        // bloqueado en un condvar no puede robar su trabajo — todos
        // acaban colgados. Entre waves se revisa cancelación/presupuesto.
        //
        // Planificación LPT (mayor effort estimado primero: AVIF,
        // zopfli, iterative): un candidato lento solo retrasa su wave,
        // nunca la cola entera. El orden de resultados se restaura al
        // original para determinismo en scoring/selección.
        let mut order: Vec<usize> = (0..candidates.len()).collect();
        // Desempate total determinista: mismo effort → menor índice.
        order.sort_by(|&a, &b| {
            candidates[b]
                .estimated_effort
                .partial_cmp(&candidates[a].estimated_effort)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });

        let mut results_by_index: Vec<Option<CandidateResult>> = vec![None; candidates.len()];

        for wave in order.chunks(max_parallel) {
            // Solo la válvula hard corta aquí; el presupuesto del goal
            // no — es guía de planeación, no límite de ejecución.
            let skip_reason = if token.is_cancelled() {
                Some("cancelled by caller before processing")
            } else if pipeline_start.elapsed() >= hard_budget {
                Some("hard emergency budget exceeded before processing (runaway codec or system overload)")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                for &idx in wave {
                    results_by_index[idx] = Some(candidate_failure(
                        &candidates[idx],
                        reason,
                        "none",
                        0,
                    ));
                }
                continue;
            }

            let wave_results: Vec<(usize, CandidateResult)> = wave
                .par_iter()
                .map(|&idx| {
                    let cand = &candidates[idx];
                    if token.is_cancelled() {
                        return (
                            idx,
                            candidate_failure(
                                cand,
                                "cancelled by caller before processing",
                                "none",
                                0,
                            ),
                        );
                    }
                    if pipeline_start.elapsed() >= hard_budget {
                        return (
                            idx,
                            candidate_failure(cand, "hard emergency budget exceeded before processing", "none", 0),
                        );
                    }
                    // Presupuesto de backend fijo por candidato (ver
                    // arriba): zopfli lo usa como timeout y rav1e para
                    // elegir el speed.
                    let cand_with_budget = Candidate {
                        backend_time_budget_ms: backend_budget_ms,
                        ..cand.clone()
                    };
                    let result = self.process_single(
                        &cand_with_budget,
                        input,
                        output_dir,
                        profile,
                        strategy,
                        constraints,
                        pipeline_start,
                        hard_budget,
                        token,
                        original_cache,
                        dedup_cache,
                    );
                    (idx, result)
                })
                .collect();

            for (idx, result) in wave_results {
                results_by_index[idx] = Some(result);
            }
        }

        results_by_index
            .into_iter()
            .zip(candidates.iter())
            .map(|(slot, cand)| slot.unwrap_or_else(|| {
                // Inalcanzable en la práctica: toda wave asigna resultado.
                candidate_failure(cand, "internal: candidate not scheduled", "none", 0)
            }))
            .collect()
    }

    fn process_single(
        &self,
        cand: &Candidate,
        input: &Path,
        output_dir: &Path,
        profile: &FileProfile,
        strategy: &OptimizationStrategy,
        constraints: &UserConstraints,
        pipeline_start: Instant,
        hard_budget: Duration,
        token: &CancellationToken,
        original_cache: &OriginalImageCache,
        dedup_cache: &super::encode_cache::EncodeDedupCache,
    ) -> CandidateResult {
        let ext = match cand.format {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp => "webp",
            Format::Avif => "avif",
            Format::Gif => "gif",
            Format::Bmp => "bmp",
            Format::Tiff => "tif",
            Format::Unknown => "bin",
        };
        let output_path = output_dir.join(format!(
            "cand_{}_{}.{}",
            cand.id,
            cand.format.as_str().to_lowercase(),
            ext
        ));

        // Defense in depth: si el output del candidato colisiona con el
        // input (nombre exótico o symlink), el codec sobrescribiría la
        // fuente a mitad de run. Check léxico barato primero, canónico
        // solo si ambos paths existen.
        if output_path == input {
            return candidate_failure(
                cand,
                "candidate output path collides with the input file",
                "none",
                0,
            );
        }
        if let (Ok(in_abs), Ok(out_abs)) = (
            std::fs::canonicalize(input),
            std::fs::canonicalize(&output_path),
        ) {
            if in_abs == out_abs {
                return candidate_failure(
                    cand,
                    "candidate output path resolves to the input file (symlink?)",
                    "none",
                    0,
                );
            }
        }

        // Candidato iterativo: buscar la calidad óptima guiada por
        // butteraugli en lugar del encode directo del backend.
        //
        // Si la búsqueda falla (error del codec, imagen no
        // decodificable), caer al path normal con el quality alto del
        // rango del strategy: el generador no emite escalera para
        // formatos iterativos, así que sin fallback el formato se
        // quedaría sin ningún candidato lossy.
        let iterative_fallback: Option<Candidate> = if cand.iterative {
            let start = Instant::now();
            match super::iterative::iterative_quality_search(
                input,
                &output_path,
                cand,
                profile,
                &strategy.goal,
                original_cache,
                dedup_cache,
                token,
            ) {
                Ok(result) => {
                    return result.into_candidate_result(cand);
                }
                Err(e) => {
                    // Cancelación: falla rápido — el resultado se
                    // descartaría en la selección de todas formas.
                    if token.is_cancelled() {
                        return candidate_failure(
                            cand,
                            &format!("iterative search cancelled: {e}"),
                            "IterativeSearch",
                            start.elapsed().as_millis() as u64,
                        );
                    }
                    let fallback_q = strategy
                        .quality_range_for(cand.format)
                        .map(|r| r.high)
                        .unwrap_or(82);
                    Some(Candidate {
                        quality: Some(fallback_q),
                        iterative: false,
                        label: format!(
                            "{} q{} (iterative fallback)",
                            cand.format.as_str(),
                            fallback_q
                        ),
                        ..cand.clone()
                    })
                }
            }
        } else {
            None
        };
        // A partir de aquí, el candidato efectivo (fallback u original).
        // Mismo id y formato → el output_path de arriba sigue válido.
        let cand: &Candidate = iterative_fallback.as_ref().unwrap_or(cand);

        let backend = match self.config.backends.get(cand.format) {
            Some(b) => b,
            None => {
                return candidate_failure(
                    cand,
                    &format!("no backend for format {}", cand.format.as_str()),
                    "none",
                    0,
                );
            }
        };

        if !backend.can_handle(profile, cand) {
            return candidate_failure(
                cand,
                "backend cannot safely handle candidate",
                backend.name(),
                0,
            );
        }

        let start = Instant::now();

        // Dedupe AVIF: un candidato lossy no iterativo (ancla o fallback
        // del iterative) codifica con el mismo perfil que los rungs del
        // search. Si este run ya produjo exactamente esos bytes, copiar
        // equivale a codificar (encoder determinista) sin pagar el
        // encode. Validación, calidad, scoring y selección corren igual
        // sobre el archivo copiado.
        let avif_dedup_key: Option<String> =
            if cand.format == Format::Avif && !cand.lossless {
                let p = super::backends::avif::avif_candidate_profile(cand);
                Some(super::encode_cache::avif_encode_key(&p))
            } else {
                None
            };

        enum DedupOutcome {
            Hit(u64),
            Produce,
        }
        let dedup_outcome = match avif_dedup_key.as_deref() {
            Some(key) => match dedup_cache.lookup(key, token) {
                super::encode_cache::DedupLookup::Hit(cached, size) => {
                    match std::fs::copy(&cached, &output_path) {
                        Ok(_) => {
                            // Honestidad: reportar el tamaño REAL del
                            // archivo copiado, no el prometido.
                            let real = std::fs::metadata(&output_path)
                                .map(|m| m.len())
                                .unwrap_or(size);
                            DedupOutcome::Hit(real)
                        }
                        // Copia fallida (disco/permisos): codificar
                        // como si no hubiera cache — el dedupe nunca
                        // convierte un hit en un fallo.
                        Err(_) => DedupOutcome::Produce,
                    }
                }
                super::encode_cache::DedupLookup::Cancelled => {
                    return candidate_failure(
                        cand,
                        "cancelled while waiting for a duplicate AVIF encode",
                        "none",
                        start.elapsed().as_millis() as u64,
                    );
                }
                super::encode_cache::DedupLookup::Produce => DedupOutcome::Produce,
            },
            None => DedupOutcome::Produce,
        };

        let backend_result = match dedup_outcome {
            DedupOutcome::Hit(size) => Ok(BackendResult {
                output_path: output_path.clone(),
                output_size: size,
                processing_time_ms: start.elapsed().as_millis() as u64,
                lossless: false, // idéntico al AvifBackend::process real
            }),
            DedupOutcome::Produce => {
                let result = backend.process(input, &output_path, cand, profile);
                match (&result, avif_dedup_key.as_deref()) {
                    // Publicar el encode propio para consumidores
                    // posteriores (waves siguientes del mismo run).
                    (Ok(_), Some(key)) => {
                        if std::fs::metadata(&output_path).is_ok() {
                            dedup_cache.store(key, &output_path);
                        }
                    }
                    // Fallo del codec: liberar la clave in-flight para
                    // que el próximo consumidor la codifique él mismo.
                    (Err(_), Some(key)) => dedup_cache.abandon(key),
                    _ => {}
                }
                result
            }
        };
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Tras el codec, antes de validar/medir: si ya hay cancelación,
        // el resultado se descartará en la selección — no gastar más
        // tiempo. El archivo lo barre `cleanup_candidate_outputs`.
        if token.is_cancelled() {
            // Best-effort: clean up the codec's output immediately so it
            // doesn't linger. The sweep at the end of the run will catch
            // it if this fails.
            let _ = std::fs::remove_file(&output_path);
            return candidate_failure(
                cand,
                "cancelled by caller during processing",
                backend.name(),
                elapsed_ms,
            );
        }

        match backend_result {
            Ok(br) => {
                // Descartar un candidato que terminó BIEN solo ante la
                // válvula hard (códec desbocado / carga extrema): cortar
                // trabajo terminado por el reloj haría la selección
                // dependiente de la carga del sistema.
                if pipeline_start.elapsed() > hard_budget {
                    let _ = std::fs::remove_file(&output_path);
                    return candidate_failure(
                        cand,
                        "hard emergency budget exceeded while processing (output discarded)",
                        backend.name(),
                        elapsed_ms,
                    );
                }
                if let Some(max_output_size) = constraints.max_output_size {
                    if br.output_size > max_output_size {
                        return candidate_failure(
                            cand,
                            &format!(
                                "output size {} exceeds configured limit {}",
                                br.output_size, max_output_size
                            ),
                            backend.name(),
                            elapsed_ms,
                        );
                    }
                }
                let measured_size = match std::fs::metadata(&br.output_path) {
                    Ok(metadata) if metadata.is_file() => metadata.len(),
                    Ok(_) => {
                        return candidate_failure(
                            cand,
                            "backend output is not a regular file",
                            backend.name(),
                            elapsed_ms,
                        )
                    }
                    Err(e) => {
                        return candidate_failure(
                            cand,
                            &format!("backend output is unavailable: {e}"),
                            backend.name(),
                            elapsed_ms,
                        )
                    }
                };
                if measured_size != br.output_size {
                    return candidate_failure(
                        cand,
                        "backend reported an output size that does not match the written file",
                        backend.name(),
                        elapsed_ms,
                    );
                }
                // Evaluación de calidad solo para lossy y solo si está
                // habilitada; AVIF se mide como cualquier formato (dav1d)
                // y la curva estimada del scorer queda como fallback sin
                // métricas. En modo perceptual se mide butteraugli
                // siempre: los candidatos extremos caen fuera de la zona
                // gris de SSIM.
                let perceptual = strategy.goal.uses_perceptual_gate();
                let quality = if self.config.evaluate_quality && !cand.lossless {
                    let evaluation = if perceptual {
                        self.quality_evaluator.evaluate_with_cache_perceptual(
                            original_cache,
                            &br.output_path,
                            false,
                        )
                    } else {
                        self.quality_evaluator.evaluate_with_cache(
                            original_cache,
                            &br.output_path,
                            false,
                        )
                    };
                    match evaluation {
                        Ok(q) => Some(q),
                        Err(e) => {
                            return candidate_failure(
                                cand,
                                &format!("quality evaluation failed: {e}"),
                                backend.name(),
                                elapsed_ms,
                            );
                        }
                    }
                } else if cand.lossless {
                    Some(super::quality::QualityMetrics::lossless())
                } else {
                    // Quality eval disabled: no metrics. The scorer
                    // handles the missing-quality case by ranking on
                    // compression + speed only.
                    None
                };

                CandidateResult {
                    candidate: cand.clone(),
                    success: true,
                    error: None,
                    output_path: Some(br.output_path),
                    output_size: br.output_size,
                    processing_time_ms: elapsed_ms,
                    backend_name: backend.name().to_string(),
                    score: None, // set by the decision builder
                    quality,
                }
            }
            Err(e) => {
                // Fallo de backend: registrar y seguir con el resto —
                // el pipeline no crashea. Puede haber quedado un output
                // parcial (disk-full, staging de resize): borrarlo aquí
                // porque el failure result no lleva output_path y el
                // sweep final no lo alcanzaría.
                let _ = std::fs::remove_file(&output_path);
                CandidateResult {
                    candidate: cand.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    output_path: None,
                    output_size: 0,
                    processing_time_ms: elapsed_ms,
                    backend_name: backend.name().to_string(),
                    score: None,
                    quality: None,
                }
            }
        }
    }

    fn kept_original_decision(
        &self,
        input: &Path,
        profile: &FileProfile,
        goal: &OptimizationGoal,
        reason: &str,
    ) -> OptimizationDecision {
        OptimizationDecision {
            input_path: input.to_path_buf(),
            selected: None,
            all_results: vec![],
            rejected: vec![],
            strategy_summary: format!("Goal: {} — kept original", goal.name()),
            profile_summary: profile.summary(),
            explanation: format!("Kept original. {}", reason),
            original_size: profile.file_size,
            final_size: profile.file_size,
            kept_original: true,
            failed: false,
        }
    }

    fn fail_decision(
        &self,
        input: &Path,
        original_size: u64,
        goal: &OptimizationGoal,
        reason: &str,
    ) -> OptimizationDecision {
        OptimizationDecision {
            input_path: input.to_path_buf(),
            selected: None,
            all_results: vec![],
            rejected: vec![],
            strategy_summary: format!("Goal: {} — failed", goal.name()),
            profile_summary: format!("(analysis failed: {})", reason),
            explanation: format!("Optimization failed: {}", reason),
            original_size,
            final_size: original_size,
            kept_original: true,
            // La cola marca el job como Failed (no Completed) según
            // este campo.
            failed: true,
        }
    }
}

fn candidate_failure(
    candidate: &Candidate,
    error: &str,
    backend_name: &str,
    processing_time_ms: u64,
) -> CandidateResult {
    CandidateResult {
        candidate: candidate.clone(),
        success: false,
        error: Some(error.to_string()),
        output_path: None,
        output_size: 0,
        processing_time_ms,
        backend_name: backend_name.to_string(),
        score: None,
        quality: None,
    }
}

fn extension_for(format: Format) -> &'static str {
    match format {
        Format::Png => "png",
        Format::Jpeg => "jpg",
        Format::Webp => "webp",
        Format::Avif => "avif",
        Format::Gif => "gif",
        Format::Bmp => "bmp",
        Format::Tiff => "tif",
        Format::Unknown => "bin",
    }
}

/// Publish a completed candidate without exposing a partial `selected.*`
/// file. The destination is deliberately not overwritten: a stale result is
/// safer to report than silently replacing a file outside this run.
fn publish_selected(src: &Path, destination: &Path, candidate_id: u32) -> Result<(), String> {
    if src == destination {
        return Ok(());
    }
    if destination.exists() {
        return Err(format!(
            "destination already exists: {}",
            destination.display()
        ));
    }
    let temporary = destination.with_file_name(format!(".boxflux-selected-{candidate_id}.tmp"));
    if temporary.exists() {
        // A stale temp from a previous crashed run. Best-effort remove;
        // if it fails (permissions, race), refuse so we don't overwrite
        // an unrelated file's contents via the copy below.
        if let Err(e) = std::fs::remove_file(&temporary) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!(
                    "temporary publication path already exists and could not be removed: {e}"
                ));
            }
        }
    }
    // Copy the candidate output into the temp path. If the copy fails
    // (disk full, permission, etc.), we must remove the partial temp so
    // it doesn't linger — `sweep_stale_temporaries` would eventually
    // clean it, but removing it now keeps the destination directory
    // clean for the next run.
    std::fs::copy(src, &temporary).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        format!("copy to temporary output failed: {e}")
    })?;
    std::fs::rename(&temporary, destination).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        format!("atomic publication failed: {e}")
    })?;
    Ok(())
}

fn profile_file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Remove the intermediate candidate output files left in `output_dir` by
/// `process_single`. The selected candidate has already been copied to
/// `selected.<ext>` via `publish_selected`, so the original candidate
/// files are pure leftover.
///
/// Best-effort: a removal failure is logged to stderr and ignored. The
/// pipeline's job is to make an explainable decision, not to be a
/// filesystem janitor — but we must not leak candidate bytes for the
/// candidates the user did not select.
fn cleanup_candidate_outputs(results: &[CandidateResult]) {
    for r in results {
        if let Some(path) = r.output_path.as_ref() {
            // Only remove files we wrote ourselves. Defensive: skip paths
            // that do not look like candidate outputs (cand_<id>_<fmt>.<ext>).
            let looks_like_candidate = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with("cand_"))
                .unwrap_or(false);
            if !looks_like_candidate {
                continue;
            }
            if let Err(e) = std::fs::remove_file(path) {
                // NotFound is fine — the file may have been cleaned already.
                if e.kind() != std::io::ErrorKind::NotFound {
                    eprintln!(
                        "boxflux: failed to remove intermediate candidate output {}: {e}",
                        path.display()
                    );
                }
            }
        }
    }
}
