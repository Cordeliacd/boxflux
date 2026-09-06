//! Stats commands.

use tauri::State;

use crate::app_state::AppState;
use crate::statistics::LiveStats;

#[tauri::command]
pub fn get_live_stats(state: State<'_, AppState>) -> LiveStats {
    state.statistics.snapshot()
}

#[tauri::command]
pub fn get_batch_summary(state: State<'_, AppState>) -> String {
    state.statistics.format_batch_summary()
}
