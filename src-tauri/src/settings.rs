//! `settings.json` in the app config folder, shaped like
//! `{"language":"en","theme":"dark","recent":["/path/a.sheafly", …],"ai":{"enabled":false,…}}`.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sheafly_engine::Lang;
use serde_json::{Map, Value};

pub const FILE_NAME: &str = "settings.json";

fn read_object(path: &Path) -> Map<String, Value> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// The language the user chose, or `None` when the file, the key, or a known value is missing.
pub fn read_language(path: &Path) -> Option<Lang> {
    read_object(path)
        .get("language")
        .and_then(Value::as_str)
        .and_then(Lang::from_code)
}

/// Remember `lang`, keeping any other settings.
pub fn write_language(path: &Path, lang: Lang) -> io::Result<()> {
    let mut settings = read_object(path);
    settings.insert("language".into(), Value::String(lang.code().into()));
    write_object(path, settings)
}

/// The colour theme: follow the system, or always light or dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub fn code(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }

    pub fn from_code(code: &str) -> Option<Theme> {
        match code {
            "system" => Some(Theme::System),
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
}

/// The theme the user chose; `System` when the file, the key, or a known value is missing.
pub fn read_theme(path: &Path) -> Theme {
    read_object(path)
        .get("theme")
        .and_then(Value::as_str)
        .and_then(Theme::from_code)
        .unwrap_or_default()
}

/// Remember `theme`, keeping any other settings.
pub fn write_theme(path: &Path, theme: Theme) -> io::Result<()> {
    let mut settings = read_object(path);
    settings.insert("theme".into(), Value::String(theme.code().into()));
    write_object(path, settings)
}

/// How many files File → Open Recent lists.
pub const RECENT_LIMIT: usize = 10;

/// Recently opened or saved projects, most recent first.
pub fn read_recent(path: &Path) -> Vec<String> {
    match read_object(path).get("recent") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .take(RECENT_LIMIT)
            .collect(),
        _ => Vec::new(),
    }
}

/// Put `file` first in the recent list (once), keeping at most `RECENT_LIMIT`.
pub fn remember_recent(path: &Path, file: &str) -> io::Result<Vec<String>> {
    let mut recent = read_recent(path);
    recent.retain(|item| item != file);
    recent.insert(0, file.to_string());
    recent.truncate(RECENT_LIMIT);
    write_recent(path, &recent)?;
    Ok(recent)
}

/// Drop `file` from the recent list, e.g. after it turned out to be gone.
pub fn forget_recent(path: &Path, file: &str) -> io::Result<Vec<String>> {
    let mut recent = read_recent(path);
    recent.retain(|item| item != file);
    write_recent(path, &recent)?;
    Ok(recent)
}

pub fn clear_recent(path: &Path) -> io::Result<()> {
    write_recent(path, &[])
}

fn write_recent(path: &Path, recent: &[String]) -> io::Result<()> {
    let mut settings = read_object(path);
    let list = recent.iter().cloned().map(Value::String).collect();
    settings.insert("recent".into(), Value::Array(list));
    write_object(path, settings)
}

/// AI recommendations (phase 22, D8): off by default, and never on before the data notice is
/// accepted. The model ID is the user's choice and is never written into a project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub enabled: bool,
    pub model: String,
    /// Send `Task 42` and `Person 7` instead of names.
    pub mask: bool,
    pub notice_accepted: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            model: crate::ai::DEFAULT_MODEL.to_string(),
            mask: false,
            notice_accepted: false,
        }
    }
}

/// The AI settings; the defaults when the file or the key is missing or does not read. A
/// file that says AI is on without the notice accepted reads as off.
pub fn read_ai(path: &Path) -> AiSettings {
    let settings: AiSettings = read_object(path)
        .get("ai")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    AiSettings { enabled: settings.enabled && settings.notice_accepted, ..settings }
}

/// Remember the AI settings, keeping any other settings.
pub fn write_ai(path: &Path, ai: &AiSettings) -> io::Result<()> {
    let mut settings = read_object(path);
    settings.insert("ai".into(), serde_json::to_value(ai).map_err(io::Error::other)?);
    write_object(path, settings)
}

/// Write the whole object to a temp file beside the real one and rename it over, so a
/// crash never leaves half a file.
fn write_object(path: &Path, settings: Map<String, Value>) -> io::Result<()> {
    let text = serde_json::to_string_pretty(&Value::Object(settings)).map_err(io::Error::other)?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{FILE_NAME}.{}.tmp", std::process::id()));
    let written = fs::write(&temp, text).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sheafly-settings-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn language_round_trips_through_the_file() {
        let dir = temp_dir("round-trip");
        let path = dir.join(FILE_NAME);
        assert_eq!(read_language(&path), None, "no file yet");

        write_language(&path, Lang::En).unwrap();
        assert_eq!(read_language(&path), Some(Lang::En));
        let stored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored, serde_json::json!({ "language": "en" }));

        write_language(&path, Lang::Vi).unwrap();
        assert_eq!(read_language(&path), Some(Lang::Vi));
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name != FILE_NAME)
            .collect();
        assert!(leftovers.is_empty(), "temp file left behind: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_settings_are_kept_and_bad_files_are_ignored() {
        let dir = temp_dir("other");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);

        fs::write(&path, r#"{"language":"fr","zoom":2}"#).unwrap();
        assert_eq!(read_language(&path), None, "unknown language");
        write_language(&path, Lang::En).unwrap();
        let stored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored, serde_json::json!({ "language": "en", "zoom": 2 }));

        fs::write(&path, "not json").unwrap();
        assert_eq!(read_language(&path), None);
        fs::write(&path, "{}").unwrap();
        assert_eq!(read_language(&path), None, "no language key");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn theme_round_trips_and_keeps_other_settings() {
        let dir = temp_dir("theme");
        let path = dir.join(FILE_NAME);
        assert_eq!(read_theme(&path), Theme::System, "no file yet");
        write_language(&path, Lang::En).unwrap();
        write_theme(&path, Theme::Dark).unwrap();
        assert_eq!(read_theme(&path), Theme::Dark);
        assert_eq!(read_language(&path), Some(Lang::En));
        write_theme(&path, Theme::Light).unwrap();
        let stored: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(stored, serde_json::json!({ "language": "en", "theme": "light" }));
        fs::write(&path, r#"{"theme":"sepia"}"#).unwrap();
        assert_eq!(read_theme(&path), Theme::System, "unknown theme");
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            assert_eq!(Theme::from_code(theme.code()), Some(theme));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ai_settings_round_trip_default_to_off_and_keep_other_settings() {
        let dir = temp_dir("ai");
        let path = dir.join(FILE_NAME);
        assert_eq!(read_ai(&path), AiSettings::default(), "no file yet");
        assert!(!read_ai(&path).enabled && read_ai(&path).model == crate::ai::DEFAULT_MODEL);
        write_language(&path, Lang::En).unwrap();
        let chosen = AiSettings { enabled: true, model: "claude-sonnet-5-5".into(), mask: true, notice_accepted: true };
        write_ai(&path, &chosen).unwrap();
        assert_eq!(read_ai(&path), chosen);
        assert_eq!(read_language(&path), Some(Lang::En), "other settings are kept");

        fs::write(&path, r#"{"ai":{"enabled":true}}"#).unwrap();
        assert!(!read_ai(&path).enabled, "on without the notice reads as off");
        fs::write(&path, r#"{"ai":{"mask":"yes"}}"#).unwrap();
        assert_eq!(read_ai(&path), AiSettings::default(), "a value that does not read");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recent_files_are_most_recent_first_unique_and_capped() {
        let dir = temp_dir("recent");
        let path = dir.join(FILE_NAME);
        assert!(read_recent(&path).is_empty());
        write_language(&path, Lang::En).unwrap();
        for index in 0..12 {
            remember_recent(&path, &format!("/p/{index}.sheafly")).unwrap();
        }
        let recent = remember_recent(&path, "/p/5.sheafly").unwrap();
        assert_eq!(recent.len(), RECENT_LIMIT);
        assert_eq!(recent[0], "/p/5.sheafly");
        assert_eq!(recent[1], "/p/11.sheafly");
        assert_eq!(recent.iter().filter(|item| *item == "/p/5.sheafly").count(), 1);
        assert_eq!(read_language(&path), Some(Lang::En), "other settings are kept");
        let recent = forget_recent(&path, "/p/11.sheafly").unwrap();
        assert!(!recent.contains(&"/p/11.sheafly".to_string()));
        clear_recent(&path).unwrap();
        assert!(read_recent(&path).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
