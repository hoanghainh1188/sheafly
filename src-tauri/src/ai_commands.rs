//! The Tauri commands for AI recommendations (phase 22, PR B and C): the settings, the API
//! key, a connection check, and explaining the project's risks. The key goes in once through `ai_save_key` to the operating system's
//! credential store and is never read back; the webview only learns whether one is saved.
//! Only calls that reach the network check the license (D9), so saving or forgetting a key
//! and the settings work on Free.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use sheafly_engine::advice::RiskCard;
use sheafly_engine::summary::{project_summary, SummaryOptions};
use tauri::State;

use super::{i18n, lang, lock, AppState};
use crate::ai::{self, AiCheck, AiError, RunError};
use crate::settings::{self, AiSettings};
use sheafly_license::Feature;

fn say(state: &AppState) -> impl Fn(AiError) -> String + '_ {
    move |error| error.message(lang(state)).to_string()
}

#[tauri::command]
pub(crate) fn ai_settings(state: State<'_, AppState>) -> AiSettings {
    state.ai.lock().unwrap_or_else(|err| err.into_inner()).clone()
}

/// Replace the AI settings: refused while the model ID is not one, or AI would be on without
/// its data notice accepted. `settings.json` is written first and the settings in use change
/// only once it is, so a failed write never leaves AI on behind the dialog's back (and the
/// two never disagree after a restart). Without a config folder they live in memory only.
#[tauri::command]
pub(crate) fn ai_set_settings(state: State<'_, AppState>, settings: AiSettings) -> Result<AiSettings, String> {
    let chosen = AiSettings { model: settings.model.trim().to_string(), ..settings };
    ai::check_settings(&chosen).map_err(say(&state))?;
    let _guard = state.settings_lock.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(path) = &state.settings {
        settings::write_ai(path, &chosen).map_err(|err| i18n::settings_not_saved(lang(&state), &err.to_string()))?;
    }
    *state.ai.lock().unwrap_or_else(|err| err.into_inner()) = chosen.clone();
    Ok(chosen)
}

// The credential store can be slow or ask the user, so these run off the main thread.

#[tauri::command(async)]
pub(crate) fn ai_save_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    ai::save_key(&key).map_err(say(&state))
}

/// Whether a key is saved (the key itself is never sent to the webview).
#[tauri::command(async)]
pub(crate) fn ai_has_key() -> bool {
    matches!(ai::load_key(), Ok(Some(_)))
}

#[tauri::command(async)]
pub(crate) fn ai_forget_key(state: State<'_, AppState>) -> Result<(), String> {
    ai::forget_key().map_err(say(&state))
}

/// Check the saved key and the chosen model with one small request.
#[tauri::command(async)]
pub(crate) fn ai_test(state: State<'_, AppState>) -> Result<AiCheck, String> {
    let settings = state.ai.lock().unwrap_or_else(|err| err.into_inner()).clone();
    let api = ai::connect(&settings, state.license.grants(Feature::Ai), ai::load_key).map_err(say(&state))?;
    ai::check(&api, &settings.model).map_err(say(&state))
}

/// What explaining the risks found: the cards, the tokens they took, and whether the summary
/// sent was cut to its size limit (D7).
#[derive(Debug, Serialize)]
pub(crate) struct RiskReport {
    cards: Vec<RiskCard>,
    input_tokens: u64,
    output_tokens: u64,
    requests: u32,
    truncated: bool,
    omitted_tasks: usize,
    omitted_resources: usize,
}

/// Explain the risks of the project as it is now (kind 1, read only). Needs the Pro modules,
/// AI on with its notice accepted, the `ai` license and a key; nothing is read or sent
/// otherwise. A newer call or `ai_cancel` stops this one between two requests, and it then
/// answers `None`. Edits made meanwhile do not stop it: the cards describe the project as
/// it was asked about.
#[tauri::command(async)]
pub(crate) fn ai_risks(state: State<'_, AppState>) -> Result<Option<RiskReport>, String> {
    let l = lang(&state);
    if !sheafly_pro::available() {
        return Err(sheafly_pro::unavailable(l));
    }
    let settings = state.ai.lock().unwrap_or_else(|err| err.into_inner()).clone();
    let api = ai::connect(&settings, state.license.grants(Feature::Ai), ai::load_key).map_err(say(&state))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.ai_cancel.lock().unwrap_or_else(|err| err.into_inner());
        running.store(true, Ordering::Relaxed);
        *running = cancel.clone();
    }
    let project = lock(&state).project.clone();
    let summary = project_summary(&project, &SummaryOptions { mask: settings.mask, ..SummaryOptions::default() });
    let first = sheafly_pro::risks_request(&summary, &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| {
        sheafly_pro::risks_next(body, reply, &project, settings.mask, l)
    };
    match ai::run_advice(&api, first, next, &cancel) {
        Ok(run) => Ok(Some(RiskReport {
            cards: run.cards,
            input_tokens: run.input_tokens,
            output_tokens: run.output_tokens,
            requests: run.requests,
            truncated: summary.truncated,
            omitted_tasks: summary.omitted_tasks,
            omitted_resources: summary.omitted_resources,
        })),
        Err(RunError::Cancelled) => Ok(None),
        Err(RunError::Ai(error)) => Err(error.message(l).to_string()),
        Err(RunError::Reply(message)) => Err(message),
    }
}

/// Stop the request `ai_risks` is running (the Stop button, or the panel closing).
#[tauri::command]
pub(crate) fn ai_cancel(state: State<'_, AppState>) {
    state.ai_cancel.lock().unwrap_or_else(|err| err.into_inner()).store(true, Ordering::Relaxed);
}
