//! Profile commands: solo lectura + selección del perfil por defecto.

use tauri::State;

use crate::app_state::AppState;
use crate::engine::optimization::OptimizationProfile;

#[tauri::command]
pub fn get_all_profiles(state: State<'_, AppState>) -> Vec<OptimizationProfile> {
    state.profiles.all()
}

#[tauri::command]
pub fn get_default_profile(state: State<'_, AppState>) -> String {
    state.profiles.default_name()
}

#[tauri::command]
pub fn set_default_profile(name: String, state: State<'_, AppState>) -> bool {
    state.profiles.set_default(&name)
}
