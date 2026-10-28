//! The project compare and variance audit commands (sheafly-pro#4).
//!
//! Gated by `Feature::Diff`. Compares the active project against another `.sheafly` file
//! or baseline project.

use std::fs;
use std::path::Path;
use tauri::State;

use super::{engine_message, i18n, lang, lock, AppState};
use sheafly_engine::diff::ProjectDiffReport;
use sheafly_engine::mspdi::read_mspdi_with_report;
use sheafly_engine::store;
use sheafly_license::Feature;

#[tauri::command]
pub(crate) async fn compare_project_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<ProjectDiffReport, String> {
    let l = lang(&state);
    if !sheafly_pro::available() {
        return Err(sheafly_pro::unavailable(l));
    }
    if !state.license.grants(Feature::Diff) {
        return Err(i18n::text(l).needs_pro_diff.into());
    }

    let file_path = Path::new(&path);
    if !file_path.exists() {
        return Err(format!("File does not exist: {path}"));
    }

    // Load Document or Project from the target file
    let baseline_project = if path.ends_with(".xml") || path.ends_with(".mspdi") {
        let xml = fs::read_to_string(file_path).map_err(|e| format!("Could not read file: {e}"))?;
        let (project, _report) = read_mspdi_with_report(&xml).map_err(engine_message(&state))?;
        project
    } else {
        let opened = store::open(&path).map_err(engine_message(&state))?;
        opened.document.project
    };

    let active_project = lock(&state).project.clone();

    tauri::async_runtime::spawn_blocking(move || {
        sheafly_pro::project_diff(&baseline_project, &active_project, l)
    })
    .await
    .map_err(|e| e.to_string())?
}
