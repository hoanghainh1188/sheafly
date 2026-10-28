//! The project compare and variance audit commands (sheafly-pro#4).
//!
//! Gated by `Feature::Diff`. Compares the active project against another `.sheafly` file
//! or baseline project.

use std::fs;
use std::path::Path;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::{await_pick, begin_dialog, engine_message, i18n, lang, lock, path_from_dialog, AppState};
use sheafly_engine::diff::ProjectDiffReport;
use sheafly_engine::mspdi::read_mspdi_with_report;
use sheafly_engine::store;
use sheafly_license::Feature;

async fn compare_file_internal(
    state: &AppState,
    path: &str,
    l: sheafly_engine::Lang,
) -> Result<ProjectDiffReport, String> {
    let file_path = Path::new(path);
    if !file_path.exists() {
        return Err(format!("File does not exist: {path}"));
    }

    // Load baseline Project from the target file
    let baseline_project = if path.ends_with(".xml") || path.ends_with(".mspdi") {
        let xml = fs::read_to_string(file_path).map_err(|e| format!("Could not read file: {e}"))?;
        let (project, _report) = read_mspdi_with_report(&xml).map_err(engine_message(state))?;
        project
    } else {
        let opened = store::open(path).map_err(engine_message(state))?;
        opened.document.project
    };

    let active_project = lock(state).project.clone();

    tauri::async_runtime::spawn_blocking(move || {
        sheafly_pro::project_diff(&baseline_project, &active_project, l)
    })
    .await
    .map_err(|e| e.to_string())?
}

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

    compare_file_internal(&state, &path, l).await
}

#[tauri::command]
pub(crate) async fn pick_and_compare_project(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<ProjectDiffReport>, String> {
    let l = lang(&state);
    if !sheafly_pro::available() {
        return Err(sheafly_pro::unavailable(l));
    }
    if !state.license.grants(Feature::Diff) {
        return Err(i18n::text(l).needs_pro_diff.into());
    }

    let Some(_permit) = begin_dialog(&state) else {
        return Ok(None);
    };

    let title = l.pick3(
        "Chọn file dự án để so sánh",
        "Choose project file to compare",
        "比較するプロジェクトファイルを選択",
    );
    let filter_name = l.pick3(
        "Tệp dự án (Sheafly, XML, MSPDI)",
        "Project files (Sheafly, XML, MSPDI)",
        "プロジェクトファイル (Sheafly, XML, MSPDI)",
    );

    let picked = await_pick(
        app.dialog()
            .file()
            .add_filter(filter_name, &["sheafly", "xml", "mspdi"])
            .set_title(title),
    )
    .await;

    let Some(path) = picked.and_then(path_from_dialog) else {
        return Ok(None);
    };

    compare_file_internal(&state, &path, l).await.map(Some)
}
