//! Engine commands: info del engine y lista de formatos.
//!
//! Solo lectura; toda operación sobre archivos pasa por la cola
//! (queue / auto_optimize), no por commands directos.

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;

#[derive(Debug, Serialize)]
pub struct EngineInfo {
    pub engine_version: String,
    pub intel_engine_version: String,
    pub max_workers: u32,
}

#[tauri::command]
pub fn get_engine_info(state: State<'_, AppState>) -> EngineInfo {
    EngineInfo {
        engine_version: crate::engine::ENGINE_VERSION.to_string(),
        intel_engine_version: crate::engine::INTEL_ENGINE_VERSION.to_string(),
        max_workers: state.engine.max_workers() as u32,
    }
}

#[derive(Debug, Serialize)]
pub struct FormatCapabilityDto {
    pub format: String,
    pub can_read: bool,
    pub can_write: bool,
    pub can_optimize: bool,
    pub can_convert: bool,
}

#[tauri::command]
pub fn get_format_list(state: State<'_, AppState>) -> Vec<FormatCapabilityDto> {
    state
        .engine
        .list_formats()
        .into_iter()
        .map(|(f, c)| FormatCapabilityDto {
            format: f.as_str().to_string(),
            can_read: c.can_read,
            can_write: c.can_write,
            can_optimize: c.can_optimize,
            can_convert: c.can_convert,
        })
        .collect()
}
