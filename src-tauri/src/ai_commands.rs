//! The Tauri commands for AI recommendations (phase 22, PR B): the settings, the API key and a
//! connection check. The key goes in once through `ai_save_key` to the operating system's
//! credential store and is never read back; the webview only learns whether one is saved.
//! Only calls that reach the network check the license (D9), so saving or forgetting a key
//! and the settings work on Free.

use tauri::State;

use super::{i18n, lang, AppState};
use crate::ai::{self, AiCheck, AiError};
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
