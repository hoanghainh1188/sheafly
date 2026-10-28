//! The Sheafly Pro license of this computer: `license.key` in the app config folder,
//! checked offline with `sheafly-license` against the keys this build accepts and the
//! day it was built (`SHEAFLY_BUILD_DATE`, from `build.rs`).
//!
//! Only the shell reads it, to decide what a command may do (`Feature`); the engine and
//! the project files never depend on it. A key is saved only when it unlocks Pro on this
//! build, so a mistyped or expired key never replaces a working one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sheafly_license::{keys, verify, Day, Feature, License, Plan, Problem, Rejected};
use tauri::{AppHandle, State};

use crate::{i18n, lang, refresh_menu, AppState};

pub const FILE_NAME: &str = "license.key";

/// The day this build was made: the commit date in a git checkout, or what CI sets.
pub fn build_date() -> Day {
    // A build with no date accepts any license's updates, rather than none.
    Day::parse(env!("SHEAFLY_BUILD_DATE")).unwrap_or(Day::from_unix_days(0))
}

/// What the frontend shows of a license: the active one, or why a key does not work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LicenseView {
    /// `none` (Free, no key), `active` (Pro on this build) or `inactive` (a key that does
    /// not unlock this build, with `problem`).
    pub state: &'static str,
    pub plan: Option<Plan>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub seats: Option<u32>,
    pub updates_until: Option<String>,
    pub expires: Option<String>,
    /// `Problem::code`, or `not_saved` when the file could not be written.
    pub problem: Option<&'static str>,
    /// The day the problem is about (the end of a trial or of updates).
    pub problem_day: Option<String>,
    pub build_date: String,
    /// Feature codes the license unlocks here.
    pub features: Vec<&'static str>,
}

impl LicenseView {
    fn none() -> Self {
        Self::of(None, None)
    }

    fn of(license: Option<&License>, problem: Option<Problem>) -> Self {
        let state = match (license, problem) {
            (_, Some(_)) => "inactive",
            (Some(_), None) => "active",
            (None, None) => "none",
        };
        let problem_day = match problem {
            Some(Problem::Expired(day) | Problem::UpdatesEnded(day)) => Some(day.to_string()),
            _ => None,
        };
        let features = match (license, problem) {
            (Some(license), None) => Feature::ALL.iter().filter(|f| license.grants(**f)).map(|f| f.code()).collect(),
            _ => Vec::new(),
        };
        LicenseView {
            state,
            plan: license.map(|l| l.plan),
            name: license.map(|l| l.name.clone()),
            email: license.map(|l| l.email.clone()),
            seats: license.map(|l| l.seats),
            updates_until: license.map(|l| l.updates_until.to_string()),
            expires: license.and_then(|l| l.expires).map(|d| d.to_string()),
            problem: problem.map(Problem::code),
            problem_day,
            build_date: build_date().to_string(),
            features,
        }
    }

    fn not_saved(license: &License) -> Self {
        LicenseView { state: "inactive", problem: Some("not_saved"), ..Self::of(Some(license), None) }
    }
}

fn view_of(verdict: &Result<License, Rejected>) -> LicenseView {
    match verdict {
        Ok(license) => LicenseView::of(Some(license), None),
        Err(rejected) => LicenseView::of(rejected.license.as_deref(), Some(rejected.problem)),
    }
}

/// The license of this run.
pub struct LicenseState {
    /// `license.key`; `None` without a config folder (a key then lasts until quitting).
    path: Option<PathBuf>,
    /// The saved key's verdict, `None` with no key.
    current: Mutex<Option<Result<License, Rejected>>>,
}

impl LicenseState {
    /// Reads the saved key, if any, checking it as of today.
    pub fn load(path: Option<PathBuf>) -> Self {
        let current = path.as_deref().and_then(read_key).map(|text| check(&text, Day::today_utc()));
        LicenseState { path, current: Mutex::new(current) }
    }

    pub fn view(&self) -> LicenseView {
        self.current().as_ref().map(view_of).unwrap_or_else(LicenseView::none)
    }

    // The commands that need Pro check it from 20D on.
    #[cfg_attr(not(test), allow(dead_code))]
    /// Whether this run may use `feature`. A trial ending while the app runs counts from
    /// the next check of the key (the next launch, or entering one).
    pub fn grants(&self, feature: Feature) -> bool {
        matches!(&*self.current(), Some(Ok(license)) if license.grants(feature))
    }

    /// Save `text` when it unlocks Pro on this build; otherwise leave the saved key alone
    /// and answer why not. The refusal is a whole view: the dialog shows its reason.
    #[allow(clippy::result_large_err)]
    pub fn install(&self, text: &str) -> Result<LicenseView, LicenseView> {
        let text = text.trim();
        let verdict = check(text, Day::today_utc());
        let license = match &verdict {
            Ok(license) => license.clone(),
            Err(_) => return Err(view_of(&verdict)),
        };
        if let Some(path) = &self.path
            && let Err(err) = write_key(path, text)
        {
            eprintln!("Sheafly: không lưu được license: {err}");
            return Err(LicenseView::not_saved(&license));
        }
        *self.current() = Some(verdict);
        Ok(LicenseView::of(Some(&license), None))
    }

    /// Forget the key on this computer.
    pub fn remove(&self) -> io::Result<LicenseView> {
        if let Some(path) = &self.path {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err),
            }
        }
        *self.current() = None;
        Ok(LicenseView::none())
    }

    /// The active license, for the About panel.
    pub fn active(&self) -> Option<License> {
        match &*self.current() {
            Some(Ok(license)) => Some(license.clone()),
            _ => None,
        }
    }

    fn current(&self) -> std::sync::MutexGuard<'_, Option<Result<License, Rejected>>> {
        self.current.lock().unwrap_or_else(|err| err.into_inner())
    }
}

fn check(text: &str, today: Day) -> Result<License, Rejected> {
    verify(text, &keys::app_keys(), build_date(), today)
}

fn read_key(path: &Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => Some(text),
        Ok(_) => None,
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            eprintln!("Sheafly: không đọc được license: {err}");
            None
        }
    }
}

/// Write beside the file and rename over it, so a crash never leaves half a key.
fn write_key(path: &Path, text: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{FILE_NAME}.{}.tmp", std::process::id()));
    let written = fs::write(&temp, format!("{text}\n")).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// The license as it stands: none, active, or a saved key that does not work here.
#[tauri::command]
pub(crate) fn license_status(state: State<'_, AppState>) -> LicenseView {
    state.license.view()
}

/// Enter a key: saved and active when it unlocks Pro on this build, else the reason, with
/// the saved key left as it was.
#[tauri::command]
#[allow(clippy::result_large_err)]
pub(crate) fn license_install(app: AppHandle, state: State<'_, AppState>, key: String) -> Result<LicenseView, LicenseView> {
    let answer = state.license.install(&key);
    if answer.is_ok() {
        refresh_menu(&app);
    }
    answer
}

/// Forget the key on this computer; Sheafly goes back to Free.
#[tauri::command]
pub(crate) fn license_remove(app: AppHandle, state: State<'_, AppState>) -> Result<LicenseView, String> {
    let view = state.license.remove().map_err(|err| {
        eprintln!("Sheafly: không xóa được license: {err}");
        i18n::text(lang(&state)).license_not_removed.to_string()
    })?;
    refresh_menu(&app);
    Ok(view)
}

// These sign with the development key, which only debug builds accept: they fail under
// `cargo test --release`, as a release build should.
#[cfg(test)]
mod tests {
    use super::*;
    use sheafly_license::issue::{issue, secret_from_hex};
    use sheafly_license::{Payload, FORMAT};

    const DEV_SECRET: &str = include_str!("../../crates/license/dev-secret.txt");

    fn key(plan: Plan, expires: Option<&str>, updates_until: &str) -> String {
        let payload = Payload {
            v: FORMAT,
            kid: keys::DEV_KID.into(),
            id: "L-test".into(),
            name: "Test".into(),
            email: "t@example.com".into(),
            plan,
            seats: 1,
            features: vec![],
            issued: "2026-01-01".into(),
            updates_until: updates_until.into(),
            expires: expires.map(str::to_string),
        };
        issue(&payload, &secret_from_hex(DEV_SECRET).unwrap())
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sheafly-license-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_good_key_is_saved_and_read_back_on_the_next_launch() {
        let path = temp_dir("good").join(FILE_NAME);
        let state = LicenseState::load(Some(path.clone()));
        assert_eq!(state.view().state, "none");
        assert!(!state.grants(Feature::Trackers));
        let view = state.install(&format!("  {}\n", key(Plan::Pro, None, "9999-12-31"))).unwrap();
        assert_eq!((view.state, view.plan, view.features.as_slice()), ("active", Some(Plan::Pro), &["trackers", "capacity", "reporting", "ai", "health", "cashflow", "diff"][..]));
        assert!(state.grants(Feature::Trackers));
        let again = LicenseState::load(Some(path.clone()));
        assert!(again.grants(Feature::Trackers));
        assert_eq!(again.remove().unwrap().state, "none");
        assert!(!path.exists());
        assert!(!LicenseState::load(Some(path)).grants(Feature::Trackers));
    }

    #[test]
    fn a_bad_key_never_replaces_a_working_one() {
        let path = temp_dir("bad").join(FILE_NAME);
        let state = LicenseState::load(Some(path.clone()));
        state.install(&key(Plan::Pro, None, "9999-12-31")).unwrap();
        let refused = state.install("SHEAFLY-NOT-A-KEY").unwrap_err();
        assert_eq!((refused.state, refused.problem), ("inactive", Some("malformed")));
        let expired = state.install(&key(Plan::Trial, Some("2000-01-01"), "9999-12-31")).unwrap_err();
        assert_eq!((expired.problem, expired.problem_day.as_deref()), (Some("expired"), Some("2000-01-01")));
        assert!(state.grants(Feature::Trackers));
        assert!(LicenseState::load(Some(path)).grants(Feature::Trackers));
    }

    #[test]
    fn a_saved_key_whose_updates_ended_says_so_and_unlocks_nothing() {
        let dir = temp_dir("ended");
        let path = dir.join(FILE_NAME);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, key(Plan::Pro, None, "2000-01-01")).unwrap();
        let state = LicenseState::load(Some(path));
        let view = state.view();
        assert_eq!((view.state, view.problem, view.plan), ("inactive", Some("updates_ended"), Some(Plan::Pro)));
        assert!(view.features.is_empty());
        assert!(!state.grants(Feature::Trackers));
    }

    #[test]
    fn a_damaged_saved_key_is_inactive_with_no_plan() {
        let dir = temp_dir("damaged");
        let path = dir.join(FILE_NAME);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "SHEAFLY-12345").unwrap();
        let view = LicenseState::load(Some(path)).view();
        assert_eq!((view.state, view.problem, view.plan), ("inactive", Some("malformed"), None));
    }

    #[test]
    fn without_a_config_folder_a_key_lasts_for_the_run() {
        let state = LicenseState::load(None);
        state.install(&key(Plan::Business, None, "9999-12-31")).unwrap();
        assert!(state.grants(Feature::Trackers));
        assert_eq!(state.remove().unwrap().state, "none");
    }

    #[test]
    fn the_build_date_is_a_day() {
        assert!(Day::parse(env!("SHEAFLY_BUILD_DATE")).is_some(), "{}", env!("SHEAFLY_BUILD_DATE"));
    }
}
