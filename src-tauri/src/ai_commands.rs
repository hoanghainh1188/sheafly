//! The Tauri commands for AI recommendations (phase 22, PR B to E): the settings, the API
//! key, a connection check, explaining the project's risks, proposals to fix it and checking
//! its estimates (tried by the engine and applied only by the user), a work breakdown from a
//! description (imported only by the user) and a status report. The key goes in once through `ai_save_key` to the operating system's
//! credential store and is never read back; the webview only learns whether one is saved.
//! Only calls that reach the network check the license (D9), so saving or forgetting a key
//! and the settings work on Free.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;
use sheafly_engine::advice::{Proposal, RiskCard, StatusReport};
use sheafly_engine::forecast::{forecast, Forecast};
use sheafly_engine::import::ImportTask;
use sheafly_engine::EngineError;
use sheafly_engine::command::Command;
use sheafly_engine::proposal::{trial_proposal, ProposalTrial};
use sheafly_engine::summary::{project_summary, SummaryOptions};
use tauri::State;

use super::{engine_message, ensure_licensed, ensure_no_dialog, i18n, lang, lock, mark_edited, AppState};
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

/// What every recommendation request needs, checked in this order: the Pro modules, then AI
/// on with its notice, the license and a key (`ai::connect`); then it stops the request
/// running and takes the project and its summary as they are now.
struct Start {
    settings: AiSettings,
    api: ai::HttpAiApi,
    cancel: Arc<AtomicBool>,
    project: sheafly_engine::Project,
    summary: sheafly_engine::summary::ProjectSummary,
}

fn start(state: &AppState) -> Result<Start, String> {
    if !sheafly_pro::available() {
        return Err(sheafly_pro::unavailable(lang(state)));
    }
    let settings = state.ai.lock().unwrap_or_else(|err| err.into_inner()).clone();
    let api = ai::connect(&settings, state.license.grants(Feature::Ai), ai::load_key).map_err(say(state))?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut running = state.ai_cancel.lock().unwrap_or_else(|err| err.into_inner());
        running.store(true, Ordering::Relaxed);
        *running = cancel.clone();
    }
    let project = lock(state).project.clone();
    let summary = project_summary(&project, &SummaryOptions { mask: settings.mask, ..SummaryOptions::default() });
    Ok(Start { settings, api, cancel, project, summary })
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
    let Start { settings, api, cancel, project, summary } = start(&state)?;
    let first = sheafly_pro::risks_request(&summary, &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| {
        sheafly_pro::risks_next(body, reply, &project, settings.mask, l)
    };
    match ai::run_advice(&api, first, next, &cancel) {
        Ok(run) => Ok(Some(RiskReport {
            cards: run.answer,
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

/// What a request to fix the project found: the proposals (not tried yet: the panel asks
/// `ai_trial` for each), the model's note when it had none, and what it cost.
#[derive(Debug, Serialize)]
pub(crate) struct FixReport {
    proposals: Vec<Proposal>,
    note: String,
    input_tokens: u64,
    output_tokens: u64,
    requests: u32,
    truncated: bool,
    omitted_tasks: usize,
    omitted_resources: usize,
}

/// Propose fixes for overallocated people and missed deadlines (kind 2). Gated as `ai_risks`.
/// The engine tries every proposal on a copy; the ones it refuses go back to the model once
/// with its words. Nothing in the project changes: only `ai_apply` does.
#[tauri::command(async)]
pub(crate) fn ai_fixes(state: State<'_, AppState>) -> Result<Option<FixReport>, String> {
    let l = lang(&state);
    let Start { settings, api, cancel, project, summary } = start(&state)?;
    state.ai_trials.lock().unwrap_or_else(|err| err.into_inner()).clear();
    let first = sheafly_pro::fixes_request(&summary, &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| {
        sheafly_pro::fixes_next(body, reply, &project, settings.mask, l)
    };
    let retry = |body: &serde_json::Value, reply: &serde_json::Value, refused: &[(String, String)]| {
        sheafly_pro::fixes_retry(body, reply, refused, l)
    };
    let refuses = |proposal: &Proposal| trial_proposal(&project, proposal).err().map(|error| error.message(l));
    match ai::run_fixes(&api, first, next, retry, refuses, &cancel) {
        Ok(run) => Ok(Some(FixReport {
            proposals: run.fixes.proposals,
            note: run.fixes.note,
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

/// Runs of the finish forecast kinds 3 and 5 read: enough for criticality and P80 in well
/// under a second on a large project.
const FORECAST_RUNS: u32 = 500;

/// The forecast for kinds 3 and 5, stopped with the request (`Err`); a forecast that fails
/// otherwise is left out and the request goes without it.
fn forecast_for(project: &sheafly_engine::Project, cancel: &AtomicBool) -> Result<Option<Forecast>, ()> {
    match forecast(project, FORECAST_RUNS, cancel) {
        Ok(found) => Ok(Some(found)),
        Err(EngineError::Cancelled) => Err(()),
        Err(_) => Ok(None),
    }
}

/// A request that did not answer: `None` when stopped, else its error in the language.
fn run_error<T>(error: RunError, l: sheafly_engine::Lang) -> Result<Option<T>, String> {
    match error {
        RunError::Cancelled => Ok(None),
        RunError::Ai(error) => Err(error.message(l).to_string()),
        RunError::Reply(message) => Err(message),
    }
}

/// What checking the estimates found (kind 3): cards as kind 1's and proposals as kind 2's,
/// not tried yet (the panel asks `ai_trial` for each).
#[derive(Debug, Serialize)]
pub(crate) struct EstimateReport {
    cards: Vec<RiskCard>,
    proposals: Vec<Proposal>,
    note: String,
    input_tokens: u64,
    output_tokens: u64,
    requests: u32,
    truncated: bool,
    omitted_tasks: usize,
    omitted_resources: usize,
}

/// Check the estimates and logic of the project (kind 3): gated as `ai_risks`, with the
/// finish forecast run first and the health check's findings added by the Pro crate. The
/// proposals are tried and retried as kind 2's; nothing in the project changes.
#[tauri::command(async)]
pub(crate) fn ai_estimates(state: State<'_, AppState>) -> Result<Option<EstimateReport>, String> {
    let l = lang(&state);
    let Start { settings, api, cancel, project, summary } = start(&state)?;
    state.ai_trials.lock().unwrap_or_else(|err| err.into_inner()).clear();
    let Ok(forecast) = forecast_for(&project, &cancel) else {
        return Ok(None);
    };
    let first = sheafly_pro::estimates_request(&summary, &project, forecast.as_ref(), &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| {
        sheafly_pro::estimates_next(body, reply, &project, settings.mask, l)
    };
    let retry = |body: &serde_json::Value, reply: &serde_json::Value, refused: &[(String, String)]| {
        sheafly_pro::estimates_retry(body, reply, refused, l)
    };
    let refuses = |proposal: &Proposal| trial_proposal(&project, proposal).err().map(|error| error.message(l));
    match ai::run_fixes(&api, first, next, retry, refuses, &cancel) {
        Ok(run) => Ok(Some(EstimateReport {
            cards: run.fixes.cards,
            proposals: run.fixes.proposals,
            note: run.fixes.note,
            input_tokens: run.input_tokens,
            output_tokens: run.output_tokens,
            requests: run.requests,
            truncated: summary.truncated,
            omitted_tasks: summary.omitted_tasks,
            omitted_resources: summary.omitted_resources,
        })),
        Err(error) => run_error(error, l),
    }
}

/// A work breakdown the model built (kind 4): the rows of one `import_rows`, people by their
/// real names. The dialog checks them with `check_import` and imports them only on the
/// user's click.
#[derive(Debug, Serialize)]
pub(crate) struct WbsReport {
    tasks: Vec<ImportTask>,
    note: String,
    input_tokens: u64,
    output_tokens: u64,
    requests: u32,
    truncated: bool,
    omitted_tasks: usize,
    omitted_resources: usize,
}

/// Build a work breakdown from the user's description (kind 4). Gated as `ai_risks`; the
/// description goes to the provider as typed. Nothing in the project changes.
#[tauri::command(async)]
pub(crate) fn ai_wbs(state: State<'_, AppState>, description: String) -> Result<Option<WbsReport>, String> {
    let l = lang(&state);
    let Start { settings, api, cancel, project, summary } = start(&state)?;
    let first = sheafly_pro::wbs_request(&description, &summary, &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| sheafly_pro::wbs_next(body, reply, &project, l);
    match ai::run_advice(&api, first, next, &cancel) {
        Ok(run) => Ok(Some(WbsReport {
            tasks: run.answer.tasks,
            note: run.answer.note,
            input_tokens: run.input_tokens,
            output_tokens: run.output_tokens,
            requests: run.requests,
            truncated: summary.truncated,
            omitted_tasks: summary.omitted_tasks,
            omitted_resources: summary.omitted_resources,
        })),
        Err(error) => run_error(error, l),
    }
}

/// A status report for stakeholders (kind 5), with what it cost.
#[derive(Debug, Serialize)]
pub(crate) struct StatusAnswer {
    report: StatusReport,
    input_tokens: u64,
    output_tokens: u64,
    requests: u32,
    truncated: bool,
    omitted_tasks: usize,
    omitted_resources: usize,
}

/// Write a status report from the engine's earned value, slack and forecast (kind 5, read
/// only). Gated as `ai_risks`; the forecast runs first and stops with the request.
#[tauri::command(async)]
pub(crate) fn ai_status(state: State<'_, AppState>) -> Result<Option<StatusAnswer>, String> {
    let l = lang(&state);
    let Start { settings, api, cancel, project, summary } = start(&state)?;
    let Ok(forecast) = forecast_for(&project, &cancel) else {
        return Ok(None);
    };
    let first = sheafly_pro::status_request(&summary, &project, forecast.as_ref(), &settings.model, l)?;
    let next = |body: &serde_json::Value, reply: &serde_json::Value| {
        sheafly_pro::status_next(body, reply, &project, settings.mask, l)
    };
    match ai::run_advice(&api, first, next, &cancel) {
        Ok(run) => Ok(Some(StatusAnswer {
            report: run.answer,
            input_tokens: run.input_tokens,
            output_tokens: run.output_tokens,
            requests: run.requests,
            truncated: summary.truncated,
            omitted_tasks: summary.omitted_tasks,
            omitted_resources: summary.omitted_resources,
        })),
        Err(error) => run_error(error, l),
    }
}

/// Try a proposal on a copy of the project as it is now (decision D5) and remember the
/// trial for `ai_apply`. Engine work only, no network, so no gate. An error is the engine
/// refusing a command, in the interface language.
#[tauri::command(async)]
pub(crate) fn ai_trial(state: State<'_, AppState>, proposal: Proposal) -> Result<ProposalTrial, String> {
    let project = lock(&state).project.clone();
    let trial = trial_proposal(&project, &proposal).map_err(engine_message(&state))?;
    state
        .ai_trials
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(proposal.id, (project, trial.commands.clone()));
    Ok(trial)
}

/// Apply the commands of a proposal's last trial as one undo step (`ApplyProposal`). Refused
/// when the project or the commands are not the ones that trial saw (decision D6).
#[tauri::command(async)]
pub(crate) fn ai_apply(state: State<'_, AppState>, id: String, commands: Vec<Command>) -> Result<serde_json::Value, String> {
    let mut doc = lock(&state);
    ensure_no_dialog(&state)?;
    let mut trials = state.ai_trials.lock().unwrap_or_else(|err| err.into_inner());
    let matches = trials
        .get(&id)
        .is_some_and(|(project, tried)| *project == doc.project && *tried == commands);
    if !matches {
        return Err(i18n::text(lang(&state)).proposal_outdated.to_string());
    }
    let command = Command::ApplyProposal { commands };
    ensure_licensed(&state, &command)?;
    let snapshot = doc.apply(command).map_err(engine_message(&state))?;
    trials.remove(&id);
    mark_edited(&state);
    Ok(state.delta.patch(snapshot))
}

/// Stop the AI request running (the Stop button, or the panel or dialog closing).
#[tauri::command]
pub(crate) fn ai_cancel(state: State<'_, AppState>) {
    state.ai_cancel.lock().unwrap_or_else(|err| err.into_inner()).store(true, Ordering::Relaxed);
}
