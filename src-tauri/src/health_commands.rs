//! The schedule health check (phase 23, sheafly-pro#7): DCMA's 14 points, scored by the
//! closed-source Pro crate (`sheafly_pro::schedule_health`). A read, never logged. Builds from
//! the public source say they have no Pro modules before asking for a license.

use tauri::State;

use super::{i18n, lang, lock, AppState};
use sheafly_engine::health::HealthReport;
use sheafly_license::Feature;

#[tauri::command]
pub(crate) async fn schedule_health(state: State<'_, AppState>) -> Result<HealthReport, String> {
    let l = lang(&state);
    if !sheafly_pro::available() {
        return Err(sheafly_pro::unavailable(l));
    }
    if !state.license.grants(Feature::Health) {
        return Err(i18n::text(l).needs_pro_health.into());
    }
    // Criterion 12 schedules a copy of the project, so the work runs off the main thread.
    let project = lock(&state).project.clone();
    tauri::async_runtime::spawn_blocking(move || sheafly_pro::schedule_health(&project, l))
        .await
        .map_err(|error| error.to_string())?
}
