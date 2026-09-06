//! Licensing commands.

use tauri::State;

use crate::app_state::AppState;
use crate::licensing::{LicenseStatus, LicenseTier, ProductEntitlement};

#[tauri::command]
pub fn get_license_tier(state: State<'_, AppState>) -> String {
    state.licensing.tier().as_str().to_string()
}

#[tauri::command]
pub fn get_license_status(state: State<'_, AppState>) -> String {
    state.licensing.status().as_str().to_string()
}

#[tauri::command]
pub fn get_license_entitlement(state: State<'_, AppState>) -> ProductEntitlement {
    state.licensing.entitlement()
}

#[tauri::command]
pub fn activate_license(
    key: String,
    state: State<'_, AppState>,
) -> Result<ProductEntitlement, String> {
    state.licensing.activate_with_key(&key)
}

#[tauri::command]
pub fn deactivate_license(state: State<'_, AppState>) {
    state.licensing.deactivate();
}

#[tauri::command]
pub fn is_entitled_to(feature: String, state: State<'_, AppState>) -> bool {
    state.licensing.is_entitled_to(&feature)
}

#[tauri::command]
pub fn list_license_tiers() -> Vec<String> {
    vec![
        LicenseTier::Free.as_str().to_string(),
        LicenseTier::Standard.as_str().to_string(),
        LicenseTier::Pro.as_str().to_string(),
        LicenseTier::Team.as_str().to_string(),
    ]
}

#[tauri::command]
pub fn list_license_statuses() -> Vec<String> {
    vec![
        LicenseStatus::Unknown.as_str().to_string(),
        LicenseStatus::Valid.as_str().to_string(),
        LicenseStatus::Expired.as_str().to_string(),
        LicenseStatus::Revoked.as_str().to_string(),
        LicenseStatus::Invalid.as_str().to_string(),
    ]
}
