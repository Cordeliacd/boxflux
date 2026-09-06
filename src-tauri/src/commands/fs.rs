//! Filesystem commands.

use std::path::PathBuf;

use crate::fs::{self, ValidationResult};

#[tauri::command]
pub fn validate_input_file(path: String, max_size_bytes: u64) -> ValidationResult {
    fs::validate_input_file(&PathBuf::from(&path), max_size_bytes)
}

#[tauri::command]
pub fn validate_output_path(output: String, input: Option<String>) -> ValidationResult {
    let input = input.map(PathBuf::from);
    fs::validate_output_path(&PathBuf::from(&output), input.as_deref())
}

#[tauri::command]
pub fn discover_supported_files(dir: String, recursive: bool) -> Vec<String> {
    fs::discover_supported_files(&PathBuf::from(&dir), recursive)
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

#[tauri::command]
pub fn is_supported_image_file(path: String) -> bool {
    fs::is_supported_image_file(&PathBuf::from(&path))
}

#[tauri::command]
pub fn compute_output_path(
    input_path: String,
    mode: String,
    custom_folder: Option<String>,
    preserve_structure: bool,
    filename_template: String,
    target_extension: Option<String>,
) -> String {
    fs::compute_output_path(
        &PathBuf::from(&input_path),
        &mode,
        custom_folder.as_deref(),
        preserve_structure,
        &filename_template,
        target_extension.as_deref(),
    )
    .to_string_lossy()
    .to_string()
}
