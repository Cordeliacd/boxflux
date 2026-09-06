//! History commands.

use tauri::State;

use crate::app_state::AppState;
use crate::history::{Aggregate, HistoryEntry};

#[tauri::command]
pub fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state.history.all()
}

#[tauri::command]
pub fn remove_history_entry(id: u64, state: State<'_, AppState>) -> bool {
    state.history.remove(id)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) {
    state.history.clear();
}

#[tauri::command]
pub fn get_history_aggregate(state: State<'_, AppState>) -> Aggregate {
    state.history.aggregate()
}
