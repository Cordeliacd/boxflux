//! `OptimizationStrategy`: el plan que emite el `StrategyEngine` a
//! partir de un `FileProfile`, un `OptimizationGoal` y
//! `UserConstraints`. Acota la generación de candidatos y guía el
//! scoring.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::engine::formats::Format;
use crate::engine::optimization::{MetadataMode, ResizeOptions};

use super::goal::{GoalWeights, OptimizationGoal, UserConstraints};
use super::profile::FileProfile;

/// Rango de calidad a probar para un formato. Inclusivo en ambos extremos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityRange {
    pub low: u8,
    pub high: u8,
    /// Paso entre valores de calidad probados: low=70, high=90,
    /// step=10 prueba {70, 80, 90}.
    pub step: u8,
}

impl QualityRange {
    pub fn iter_values(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut v = self.low;
        while v <= self.high {
            out.push(v);
            if v == self.high {
                break;
            }
            let next = v.saturating_add(self.step);
            if next <= v {
                break;
            } // step era 0 o hubo overflow
            v = next;
        }
        // Asegurar que high quede incluido exactamente una vez.
        if out.last().copied() != Some(self.high) {
            out.push(self.high);
        }
        out
    }
}

/// El plan: formatos a probar, rangos de calidad por formato, modo de
/// metadata, resize y presupuesto de tiempo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationStrategy {
    pub goal: OptimizationGoal,
    pub weights: GoalWeights,

    /// Formatos a probar como candidatos, en orden de preferencia.
    pub candidate_formats: Vec<Format>,

    /// Rango de calidad por formato; los ausentes usan el default del
    /// formato.
    pub quality_ranges: BTreeMap<u8, QualityRange>, // clave: Format as u8

    /// Resize opcional aplicado a todos los candidatos. None = sin resize.
    pub resize: Option<ResizeOptions>,

    pub metadata_mode: MetadataMode,

    /// Si es `true`, los backends deben preservar el perfil de color ICC
    /// del original en la salida. Threaded desde `UserConstraints`.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,

    /// Si se deben probar candidatos lossless.
    pub include_lossless: bool,

    /// Si se pueden probar candidatos lossy.
    pub include_lossy: bool,

    /// Presupuesto de tiempo por archivo, en ms (límite duro).
    pub max_processing_time_ms: u64,

    /// Máximo de candidatos que puede emitir el generador; lo fija el goal.
    pub max_candidates: usize,

    /// Justificación en texto libre, mostrada en la UI.
    pub rationale: String,

    /// Presupuesto de memoria en MB para todos los candidatos de esta
    /// estrategia. 0 = sin límite. Los backends lo consultan para
    /// decidir si usar presets agresivos (zopfli, AVIF speed=4) o
    /// conservadores.
    ///
    /// Proviene de `AppSettings.max_memory_mb` vía `UserConstraints`.
    #[serde(default)]
    pub memory_budget_mb: u64,

    /// Sonda de sensibilidad lossy: SSIM del roundtrip WebP q75 del
    /// preview, medido por el analyzer. Decide la escalera AVIF y la
    /// curva de estimación de calidad del scorer: ≥ 0.955 → contenido
    /// gráfico (AVIF estable), < 0.955 → fotográfico (acantilado de
    /// calidad). None → el probe falló: se usa `FileProfile::is_photo_like()`.
    #[serde(default)]
    pub lossy_probe_ssim: Option<f32>,
}

fn default_preserve_color_profile() -> bool {
    true
}

impl OptimizationStrategy {
    pub fn quality_range_for(&self, format: Format) -> Option<QualityRange> {
        self.quality_ranges.get(&(format as u8)).copied()
    }

    pub fn set_quality_range(&mut self, format: Format, range: QualityRange) {
        self.quality_ranges.insert(format as u8, range);
    }
}

/// Motor de estrategia: función pura de (profile, goal, constraints).
#[derive(Debug, Default)]
pub struct StrategyEngine;

impl StrategyEngine {
    pub fn new() -> Self {
        Self
    }

    /// ¿Contenido sensible a lossy? Prefiere la sonda medida (roundtrip
    /// WebP q75 del preview) y cae a la clasificación por categoría si
    /// no hay. La sonda manda: las categorías confundían fotos con
    /// gradientes.
    fn photo_sensitive(profile: &FileProfile) -> bool {
        match profile.lossy_probe_ssim {
            Some(probe) => probe < 0.955,
            None => profile.is_photo_like(),
        }
    }

    pub fn plan(
        &self,
        profile: &FileProfile,
        goal: OptimizationGoal,
        constraints: &UserConstraints,
    ) -> OptimizationStrategy {
        let weights = goal.weights();

        // Formatos a probar.
        let mut candidate_formats: Vec<Format> = Vec::new();

        // Siempre incluir el formato fuente (recompresión lossless).
        if profile.format != Format::Unknown {
            candidate_formats.push(profile.format);
        }

        // Formatos preferidos por el goal, si la fuente puede convertir.
        for f in &weights.preferred_formats {
            if !candidate_formats.contains(f) && self.can_convert(profile, *f) {
                candidate_formats.push(*f);
            }
        }

        // Formato forzado: estrechar a solo ese. La lista puede quedar
        // vacía (p.ej. PNG con alpha forzado a JPEG) y el motor responde
        // "kept original" con explicación en vez de perder la
        // transparencia silenciosamente.
        let mut force_format_blocked_reason: Option<String> = None;
        if let Some(forced) = constraints.force_format {
            candidate_formats.retain(|f| *f == forced);
            if forced != Format::Unknown
                && self.can_convert(profile, forced)
                && !candidate_formats.contains(&forced)
            {
                candidate_formats.push(forced);
            }
            // Si la lista quedó vacía y la fuente es compatible con el
            // formato forzado a nivel de codec, pero `can_convert` lo
            // bloqueó (p.ej. alpha→JPEG), producimos una razón legible
            // para que el pipeline la incluya en su explicación.
            if candidate_formats.is_empty() && forced != Format::Unknown {
                force_format_blocked_reason =
                    Some(self.explain_force_format_block(profile, forced));
            }
        }

        // Animado: restringir a formatos que preservan la animación (GIF).
        if profile.is_animated {
            candidate_formats.retain(|f| *f == Format::Gif);
        }

        // Tope de formatos: reusar `weights.max_candidates` (tope de
        // candidatos TOTALES que aplica el generador) también aquí es
        // intencional — cada formato aporta al menos un candidato, así
        // que acota el trabajo del generador. Con los goals actuales no
        // recorta nunca; defiende contra goals custom muy restrictivos.
        candidate_formats.truncate(weights.max_candidates.max(1));

        // Lossless vs lossy.
        let include_lossless = true;
        let include_lossy = if constraints.force_lossless {
            false
        } else {
            weights.allow_lossy
        };

        // Rangos de calidad por formato.
        let mut quality_ranges: BTreeMap<u8, QualityRange> = BTreeMap::new();
        if include_lossy {
            for f in &candidate_formats {
                let range = self.quality_range_for_format(*f, &goal, profile);
                if let Some(r) = range {
                    quality_ranges.insert(*f as u8, r);
                }
            }
        }

        // Resize.
        //
        // Contradice una petición lossless: prevalece la garantía
        // lossless antes que aplicar una transformación geométrica con
        // pérdida.
        let resize = self.compute_resize(
            profile,
            constraints,
            constraints.force_lossless || matches!(&goal, OptimizationGoal::Lossless),
        );

        // Metadata.
        let metadata_mode = if constraints.strip_all_metadata {
            MetadataMode::RemoveAll
        } else {
            match &goal {
                OptimizationGoal::Lossless | OptimizationGoal::Quality => MetadataMode::Keep,
                OptimizationGoal::Web
                | OptimizationGoal::Balanced
                | OptimizationGoal::MaximumCompression
                | OptimizationGoal::ExtremeLightweight => MetadataMode::RemoveSafe,
                OptimizationGoal::Custom(_) => MetadataMode::RemoveSafe,
            }
        };

        // Perfil de color.
        //
        // Los backends leen el flag vía `Candidate.preserve_color_profile`
        // para embeber el ICC del original (mozjpeg `set_icc_profile`,
        // oxipng `StripChunks::Safe`). `strip_all_metadata` gana: el
        // usuario pidió explícitamente limpiar todo.
        let preserve_color_profile =
            constraints.preserve_color_profile && !constraints.strip_all_metadata;

        // Presupuesto de tiempo.
        let max_processing_time_ms = constraints
            .max_processing_time_ms
            .unwrap_or(weights.max_processing_time_ms);

        // Rationale para el panel de explicabilidad de la UI.
        let mut rationale = self.build_rationale(profile, &goal, &candidate_formats, include_lossy);
        if let Some(reason) = &force_format_blocked_reason {
            rationale = format!("{rationale} ADVERTENCIA: {reason}");
        }
        let max_candidates = weights.max_candidates;

        OptimizationStrategy {
            goal,
            weights,
            candidate_formats,
            quality_ranges,
            resize,
            metadata_mode,
            preserve_color_profile,
            include_lossless,
            include_lossy,
            max_processing_time_ms,
            max_candidates,
            rationale,
            // Presupuesto de memoria threaded desde los UserConstraints
            // (que vienen de AppSettings.max_memory_mb).
            memory_budget_mb: constraints.memory_budget_mb,
            // Sonda de sensibilidad lossy del analyzer.
            lossy_probe_ssim: profile.lossy_probe_ssim,
        }
    }

    fn can_convert(&self, profile: &FileProfile, target: Format) -> bool {
        // De AVIF solo salimos como AVIF: la conversión a otros formatos
        // requeriría decodificar a píxeles y no hay ruta de re-encode.
        if profile.format == Format::Avif && target != Format::Avif {
            return false;
        }
        // Fuente animada → solo GIF: el resto perdería la animación.
        if profile.is_animated && target != Format::Gif {
            return false;
        }
        // JPEG/BMP no soportan alpha y no hay opción de aplanado en las
        // constraints públicas: convertir descartaría la transparencia
        // silenciosamente.
        if profile.has_alpha && matches!(target, Format::Jpeg | Format::Bmp) {
            return false;
        }
        true
    }

    /// Genera una explicación legible cuando `force_format` produce una
    /// lista de candidatos vacía. El pipeline la incluye en su
    /// `decision.explanation` para que el usuario entienda por qué se
    /// mantuvo el original.
    fn explain_force_format_block(&self, profile: &FileProfile, forced: Format) -> String {
        let forced_name = forced.as_str();
        if profile.format == Format::Avif && forced != Format::Avif {
            return format!(
                "el formato solicitado ({}) no se puede aplicar a un archivo AVIF de entrada — \
                 BoxFlux no decodifica AVIF en esta versión. \
                 Prueba con WebP o convierte el AVIF a PNG antes con una herramienta externa.",
                forced_name
            );
        }
        if profile.is_animated && forced != Format::Gif {
            return format!(
                "el formato solicitado ({}) no preserva la animación del GIF original — \
                 usa GIF para mantener la animación, o convierte a un formato estático aceptando \
                 que se pierde el movimiento.",
                forced_name
            );
        }
        if profile.has_alpha && matches!(forced, Format::Jpeg | Format::Bmp) {
            return format!(
                "el formato solicitado ({}) no soporta transparencia y la imagen original \
                 tiene canal alfa. BoxFlux no flatten-a automáticamente (no decide por ti qué \
                 color de fondo usar). Prueba con PNG, WebP o AVIF que sí soportan alpha.",
                forced_name
            );
        }
        format!(
            "el formato solicitado ({}) no es compatible con esta imagen por una razón \
             no cubierta por los casos conocidos. Revisa el formato de entrada y vuelve a \
             intentarlo.",
            forced_name
        )
    }

    fn quality_range_for_format(
        &self,
        format: Format,
        goal: &OptimizationGoal,
        profile: &FileProfile,
    ) -> Option<QualityRange> {
        if !goal.weights().allow_lossy {
            return None;
        }
        // Steps finos: 4-5 niveles por formato. Cuesta ~2× el tiempo de
        // encode pero sube mucho la probabilidad de dar con el óptimo,
        // sobre todo en WebP, donde la curva tamaño/calidad tiene un
        // codo entre 75 y 85.
        match format {
            Format::Jpeg => match goal {
                OptimizationGoal::MaximumCompression => Some(QualityRange {
                    low: 60,
                    high: 82,
                    step: 7,
                }),
                OptimizationGoal::Web => Some(QualityRange {
                    low: 72,
                    high: 88,
                    step: 8,
                }),
                OptimizationGoal::Balanced => Some(QualityRange {
                    low: 78,
                    high: 92,
                    step: 7,
                }),
                OptimizationGoal::Quality => Some(QualityRange {
                    low: 88,
                    high: 96,
                    step: 4,
                }),
                // ExtremeLightweight: JPEG lo cubre el iterative search;
                // este rango es el quality de fallback del pipeline si la
                // búsqueda iterativa falla (ver `pipeline::process_single`).
                OptimizationGoal::ExtremeLightweight => Some(QualityRange {
                    low: 65,
                    high: 90,
                    step: 5,
                }),
                _ => None,
            },
            Format::Webp => match goal {
                OptimizationGoal::MaximumCompression => Some(QualityRange {
                    low: 55,
                    high: 80,
                    step: 8,
                }),
                OptimizationGoal::Web => Some(QualityRange {
                    low: 72,
                    high: 88,
                    step: 8,
                }),
                OptimizationGoal::Balanced => Some(QualityRange {
                    low: 78,
                    high: 92,
                    step: 7,
                }),
                OptimizationGoal::Quality => Some(QualityRange {
                    low: 88,
                    high: 96,
                    step: 4,
                }),
                // Fallback si la búsqueda iterativa de WebP falla
                // (ver `pipeline::process_single`).
                OptimizationGoal::ExtremeLightweight => Some(QualityRange {
                    low: 60,
                    high: 90,
                    step: 5,
                }),
                _ => None,
            },
            Format::Avif => match goal {
                // Escalera AVIF consciente de contenido: las fotos tienen
                // un acantilado de calidad (q60 puede caer a SSIM ~0.73;
                // q80 ya ~0.95) y los gráficos son estables en todo el
                // rango. Los candidatos AVIF se miden con métricas reales,
                // así que el rango bajo se puede explorar y dejar que la
                // medición decida.
                OptimizationGoal::MaximumCompression => {
                    if Self::photo_sensitive(profile) {
                        // El modo más compresivo explora todo el rango
                        // bajo con medición real.
                        Some(QualityRange {
                            low: 40,
                            high: 80,
                            step: 10,
                        })
                    } else {
                        // Gráficos: en modo extremo el rango bajo es
                        // territorio legítimo (comprime brutalmente
                        // sin romperse).
                        Some(QualityRange {
                            low: 20,
                            high: 60,
                            step: 10,
                        })
                    }
                }
                OptimizationGoal::Web => return None, // AVIF no es prioritario para Web
                OptimizationGoal::Balanced => {
                    if Self::photo_sensitive(profile) {
                        // Threshold 0.80: la estimación foto pasa a partir
                        // de q80 (0.85). q75 (est 0.78) sería rechazado.
                        Some(QualityRange {
                            low: 80,
                            high: 90,
                            step: 5,
                        })
                    } else {
                        Some(QualityRange {
                            low: 50,
                            high: 70,
                            step: 10,
                        })
                    }
                }
                OptimizationGoal::Quality => {
                    if Self::photo_sensitive(profile) {
                        // Threshold 0.92: solo q90+ (est 0.93) pasa.
                        Some(QualityRange {
                            low: 90,
                            high: 95,
                            step: 5,
                        })
                    } else {
                        Some(QualityRange {
                            low: 65,
                            high: 82,
                            step: 9,
                        })
                    }
                }
                // La escalera se mide con métricas reales y se amplía al
                // rango medio-bajo: decide el gate de calidad del scorer,
                // no una estimación pesimista.
                OptimizationGoal::ExtremeLightweight => {
                    if Self::photo_sensitive(profile) {
                        Some(QualityRange {
                            low: 70,
                            high: 95,
                            step: 8,
                        })
                    } else {
                        Some(QualityRange {
                            low: 30,
                            high: 65,
                            step: 8,
                        })
                    }
                }
                _ => None,
            },
            // PNG es lossless: sin rango de calidad.
            Format::Png => None,
            // GIF/BMP/TIFF: sin parámetro de calidad lossy.
            Format::Gif | Format::Bmp | Format::Tiff => None,
            Format::Unknown => None,
        }
        .map(|r| {
            // Imágenes muy grandes: estrechar el rango para ahorrar tiempo.
            if profile.is_very_large() {
                QualityRange {
                    low: r.high, // solo el extremo alto
                    high: r.high,
                    step: 1,
                }
            } else {
                r
            }
        })
    }

    fn compute_resize(
        &self,
        profile: &FileProfile,
        constraints: &UserConstraints,
        lossless_required: bool,
    ) -> Option<ResizeOptions> {
        if lossless_required {
            return None;
        }
        let max_w = constraints.max_width;
        let max_h = constraints.max_height;
        let needs_resize = match (max_w, max_h) {
            (Some(mw), Some(mh)) => profile.width > mw || profile.height > mh,
            (Some(mw), None) => profile.width > mw,
            (None, Some(mh)) => profile.height > mh,
            (None, None) => false,
        };
        if needs_resize {
            Some(ResizeOptions {
                target_width: None,
                target_height: None,
                percentage: None,
                max_width: max_w,
                max_height: max_h,
                mode: crate::engine::optimization::ResizeMode::Fit,
            })
        } else {
            None
        }
    }

    fn build_rationale(
        &self,
        profile: &FileProfile,
        goal: &OptimizationGoal,
        candidates: &[Format],
        include_lossy: bool,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "Goal: {} for a {}x{} {} {} image ({:.1} KiB)",
            goal.name(),
            profile.width,
            profile.height,
            profile.format,
            profile.color_model.as_str(),
            profile.file_size as f32 / 1024.0,
        ));
        parts.push(format!(
            "Category: {} (complexity {:.2}, compressibility {:.2})",
            profile.category.as_str(),
            profile.complexity,
            profile.estimated_compressibility,
        ));
        if profile.is_animated {
            parts.push("Animated: candidates restricted to GIF.".into());
        }
        if profile.has_alpha {
            parts.push("Has alpha: lossy candidates that drop alpha will be penalized.".into());
        }
        parts.push(format!(
            "Will test {} candidate format(s): {}",
            candidates.len(),
            candidates
                .iter()
                .map(|f| f.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
        if include_lossy {
            parts.push("Lossy candidates permitted within quality threshold.".into());
        } else {
            parts.push("Lossy candidates disabled; only lossless recompression.".into());
        }
        parts.join(" ")
    }
}
