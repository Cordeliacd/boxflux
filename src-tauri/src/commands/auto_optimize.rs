//! Auto-optimize commands. Ejecutan el pipeline de optimización inteligente
//! (intel/) en background, emitiendo events al frontend.
//!
//! Eventos emitidos:
//! - `boxflux://auto-optimize-started` { request_id }
//! - `boxflux://auto-optimize-finished` { request_id, decision }
//! - `boxflux://auto-optimize-failed` { request_id, error }
//! - `boxflux://auto-optimize-cancelled` { request_id, reason }
//! - `boxflux://auto-optimize-state-changed` { running }

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::engine::intel::{OptimizationDecision, OptimizationGoal, UserConstraints};

#[derive(Debug, Deserialize, Serialize)]
pub struct AutoOptimizeArgs {
    pub input_path: String,
    pub output_dir: String,
    pub goal: String,
    pub force_lossless: bool,
    pub strip_all_metadata: bool,
    /// Formato de salida forzado (modo Manual).
    ///
    /// `None` o string vacío → modo Auto: el motor evalúa todos los formatos
    /// disponibles y selecciona el de menor peso en bytes según el goal.
    ///
    /// `Some("PNG" | "JPEG" | "WebP" | "AVIF")` → modo Manual: el motor
    /// restringe los candidatos a ese formato y no realiza conversiones
    /// cruzadas. La extensión del archivo final coincidirá con el formato
    /// seleccionado.
    #[serde(default)]
    pub force_format: Option<String>,
    /// Si es `true`, preserva el perfil de color ICC del original en la
    /// salida (Display P3, sRGB). Evita el oscurecimiento de tonos negros
    /// y sombras que se produce al asumir sRGB. Por defecto `true`.
    #[serde(default = "default_preserve_color_profile")]
    pub preserve_color_profile: bool,
}

fn default_preserve_color_profile() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOptimizeStartedPayload {
    pub request_id: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOptimizeFinishedPayload {
    pub request_id: u64,
    pub decision: OptimizationDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOptimizeFailedPayload {
    pub request_id: u64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOptimizeCancelledPayload {
    pub request_id: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoOptimizeStateChangedPayload {
    pub running: bool,
}

fn parse_goal(s: &str) -> OptimizationGoal {
    // El goal puede ser un identificador canonico
    // ("ExtremeLightweight", "Lossless", ...) o un prompt en lenguaje
    // natural; si no matchea identificador, va al goal_parser.
    match s {
        "Lossless" => OptimizationGoal::Lossless,
        "Quality" => OptimizationGoal::Quality,
        "Balanced" => OptimizationGoal::Balanced,
        "Compress" | "MaximumCompression" => OptimizationGoal::MaximumCompression,
        "Web" => OptimizationGoal::Web,
        "ExtremeLightweight" => OptimizationGoal::ExtremeLightweight,
        _ => {
            // No es identificador → prompt en lenguaje natural
            // (el parser reconoce espanol e ingles).
            crate::engine::intel::parse_goal_prompt(s)
        }
    }
}

/// Parsea el string `force_format` recibido del frontend a un `Format`.
///
/// Acepta los strings canonizados por `Format::as_str()` (p.ej. "PNG",
/// "JPEG", "WebP", "AVIF") o extensiones sin punto ("png", "jpg", "webp",
/// "avif"). Devuelve `None` si el string está vacío o no se reconoce.
fn parse_force_format(s: &Option<String>) -> Option<crate::engine::formats::Format> {
    let s = s.as_deref()?.trim();
    if s.is_empty() {
        return None;
    }
    // Quita un punto inicial si existe (".png" → "png").
    let s = s.trim_start_matches('.');
    // Intenta primero como nombre canonizado (case-sensitive en Format::as_str,
    // pero el FromStr de Format es case-insensitive).
    s.parse::<crate::engine::formats::Format>().ok()
}

fn build_constraints(args: &AutoOptimizeArgs, memory_budget_mb: u64) -> UserConstraints {
    UserConstraints {
        force_lossless: args.force_lossless,
        strip_all_metadata: args.strip_all_metadata,
        force_format: parse_force_format(&args.force_format),
        preserve_color_profile: args.preserve_color_profile,
        memory_budget_mb,
        ..Default::default()
    }
}

#[tauri::command]
pub fn start_auto_optimize(
    app_handle: AppHandle,
    args: AutoOptimizeArgs,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    // Check y set bajo un único write-lock: con locks separados, dos
    // invocaciones concurrentes pasaban ambas el check y se
    // sobrescribían el token, dejando la primera incancelable.
    let cancel_token = crate::engine::intel::CancellationToken::new();
    {
        let mut cur = state.auto_optimize_cancel.write();
        if let Some(token) = cur.as_ref() {
            if !token.is_cancelled() {
                return Err("ya hay una optimización en curso".into());
            }
        }
        *cur = Some(cancel_token.clone());
    }
    let request_id = state.next_request_id();
    let _ = app_handle.emit(
        "boxflux://auto-optimize-state-changed",
        AutoOptimizeStateChangedPayload { running: true },
    );
    let _ = app_handle.emit(
        "boxflux://auto-optimize-started",
        AutoOptimizeStartedPayload { request_id },
    );

    let pipeline = Arc::clone(&state.pipeline);
    let history = Arc::clone(&state.history);
    // El token se limpia al terminar: si no, la segunda optimización
    // de la sesión fallaría para siempre con "ya hay una optimización
    // en curso".
    let app_state: tauri::State<'_, AppState> = app_handle.state();
    let state_arc = app_state.inner();
    let auto_optimize_cancel = state_arc.auto_optimize_cancel.clone();
    let app_handle_clone = app_handle.clone();
    let input_path = PathBuf::from(&args.input_path);
    let output_dir = PathBuf::from(&args.output_dir);
    let goal = parse_goal(&args.goal);
    // Pasar el memory_budget del queue (configurado en Ajustes).
    let constraints = build_constraints(&args, state.queue.memory_budget_mb());

    let cancel_token_for_check = cancel_token.clone();
    let run_started = std::time::Instant::now();
    // `goal` se mueve al closure de spawn_blocking; clonamos para el
    // registro de historial posterior.
    let goal_for_history = goal.clone();
    tauri::async_runtime::spawn(async move {
        let app_handle_for_result = app_handle_clone.clone();
        let history_for_result = Arc::clone(&history);
        let result = tauri::async_runtime::spawn_blocking(move || {
            pipeline.optimize_with_cancel(
                &input_path,
                &output_dir,
                goal,
                &constraints,
                &cancel_token,
            )
        })
        .await;
        match result {
            Ok(decision) => {
                // Registrar la ejecución en el historial.
                let kept = decision.kept_original;
                let entry = crate::history::HistoryEntry {
                    id: 0,                    // auto-asignado por append()
                    timestamp: String::new(), // auto-asignado por append()
                    files_processed: if kept { 0 } else { 1 },
                    files_failed: if kept { 1 } else { 0 },
                    profile_name: format!("auto:{}", goal_for_history.name()),
                    original_size: decision.original_size,
                    output_size: decision.final_size,
                    duration_ms: run_started.elapsed().as_millis() as u64,
                };
                let _ = history_for_result.append(entry);
                if cancel_token_for_check.is_cancelled() {
                    let _ = app_handle_for_result.emit(
                        "boxflux://auto-optimize-cancelled",
                        AutoOptimizeCancelledPayload {
                            request_id,
                            reason: "cancelado por el usuario".into(),
                        },
                    );
                } else {
                    let _ = app_handle_for_result.emit(
                        "boxflux://auto-optimize-finished",
                        AutoOptimizeFinishedPayload {
                            request_id,
                            decision,
                        },
                    );
                }
            }
            Err(e) => {
                let _ = app_handle_for_result.emit(
                    "boxflux://auto-optimize-failed",
                    AutoOptimizeFailedPayload {
                        request_id,
                        error: format!("panic del worker: {e}"),
                    },
                );
            }
        }
        // Solo limpiamos si el token registrado sigue siendo el
        // NUESTRO: si fue cancelado y una nueva optimización ya lo
        // reemplazó, su token debe permanecer registrado.
        {
            let mut cur = auto_optimize_cancel.write();
            let is_ours = cur
                .as_ref()
                .map(|t| t.ptr_eq(&cancel_token_for_check))
                .unwrap_or(false);
            if is_ours {
                *cur = None;
            }
        }
        let _ = app_handle_for_result.emit(
            "boxflux://auto-optimize-state-changed",
            AutoOptimizeStateChangedPayload { running: false },
        );
    });

    Ok(request_id)
}

#[tauri::command]
pub fn cancel_auto_optimize(state: State<'_, AppState>) -> Result<(), String> {
    let cur = state.auto_optimize_cancel.read();
    if let Some(token) = cur.as_ref() {
        token.cancel();
        Ok(())
    } else {
        Err("no hay optimización en curso".into())
    }
}

#[tauri::command]
pub fn is_auto_optimize_running(state: State<'_, AppState>) -> bool {
    let cur = state.auto_optimize_cancel.read();
    cur.as_ref().map(|t| !t.is_cancelled()).unwrap_or(false)
}

#[tauri::command]
pub fn list_intel_backends(state: State<'_, AppState>) -> Vec<(String, String)> {
    // Usamos el BackendRegistry a través del pipeline config.
    state
        .pipeline
        .list_backends()
        .into_iter()
        .map(|(f, name)| (f.as_str().to_string(), name))
        .collect()
}

// Use std::sync::Arc for pipeline clone in start_auto_optimize.
use std::sync::Arc;
