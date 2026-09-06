//! Settings commands.

use tauri::State;

use crate::app_state::AppState;
use crate::settings::{AppSettings, OutputMode, Theme};

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.settings.get()
}

/// Actualiza los settings Y aplica los cambios en runtime al JobQueue.
///
/// Antes este comando solo guardaba los settings a disco — los cambios
/// a `worker_count` y `cpu_limit_percent` no tomaban efecto hasta
/// reiniciar la app. Ahora se aplican inmediatamente:
///
///   - `worker_count` → `JobQueue::set_max_workers()`
///   - `cpu_limit_percent` → `JobQueue::set_cpu_limit_percent()`
///
/// Así los sliders de la página de Ajustes funcionan de verdad: el
/// usuario ve el efecto al instante (workers activos suben/bajan
/// según el valor).
#[tauri::command]
pub fn update_settings(settings: AppSettings, state: State<'_, AppState>) -> bool {
    state.settings.replace(settings.clone());
    // Aplicar cambios de performance al JobQueue en runtime.
    state.queue.set_max_workers(settings.worker_count);
    state
        .queue
        .set_cpu_limit_percent(settings.cpu_limit_percent);
    state.queue.set_memory_budget_mb(settings.max_memory_mb);
    tracing::info!(
        "settings aplicados: workers={}, cpu_limit={}%, mem_hint={}MB",
        settings.worker_count,
        settings.cpu_limit_percent,
        settings.max_memory_mb
    );
    true
}

#[tauri::command]
pub fn list_themes() -> Vec<String> {
    vec![
        Theme::Light.as_str().to_string(),
        Theme::Dark.as_str().to_string(),
        Theme::System.as_str().to_string(),
    ]
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }
}

impl OutputMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputMode::SameFolder => "SameFolder",
            OutputMode::OutputSubfolder => "OutputSubfolder",
            OutputMode::CustomFolder => "CustomFolder",
        }
    }
}
