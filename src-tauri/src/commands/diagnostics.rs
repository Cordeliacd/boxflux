//! Diagnostics commands.

use tauri::State;

use crate::app_state::AppState;
use crate::diagnostics::{LogEntry, LogLevel, LogSource};

#[tauri::command]
pub fn get_logs(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.diagnostics.all()
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) {
    state.diagnostics.clear();
}

#[tauri::command]
pub fn export_logs(state: State<'_, AppState>) -> String {
    state.diagnostics.export_as_text()
}

#[tauri::command]
pub fn copy_diagnostics(state: State<'_, AppState>) -> String {
    state.diagnostics.diagnostics_summary(&state.engine)
}

#[tauri::command]
pub fn log_debug(message: String, state: State<'_, AppState>) {
    state
        .diagnostics
        .log(LogLevel::Debug, LogSource::Frontend.as_str(), &message);
}

#[tauri::command]
pub fn log_info(message: String, state: State<'_, AppState>) {
    state
        .diagnostics
        .log(LogLevel::Info, LogSource::Frontend.as_str(), &message);
}

#[tauri::command]
pub fn log_warn(message: String, state: State<'_, AppState>) {
    state
        .diagnostics
        .log(LogLevel::Warning, LogSource::Frontend.as_str(), &message);
}

#[tauri::command]
pub fn log_error(message: String, state: State<'_, AppState>) {
    state
        .diagnostics
        .log(LogLevel::Error, LogSource::Frontend.as_str(), &message);
}
