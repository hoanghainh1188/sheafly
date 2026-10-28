//! Talking to an external task tracker (phase 19, one way).
//!
//! Every network call of a pull runs here, in the Tauri layer, never in the webview. The
//! API key lives in the operating system's credential store (Keychain, Credential
//! Manager, Secret Service) under the tracker's site, is read only to make a request, and
//! is sent in a header, so it is in no URL, file, log or message. Errors are a fixed set of
//! kinds with fixed texts: nothing the tracker or the network said reaches the user, since
//! it could echo a request.
//!
//! An adapter (`backlog.rs`) talks to the tracker through `Api`, so its logic is tested
//! against handwritten responses and CI makes no network call.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sheafly_engine::pull::{self, GoneTask, PullColumn, PullConflict, PullRowDiff, PullTask};
use sheafly_engine::sync::Resolution;
use sheafly_engine::source::TrackerKind;
use sheafly_engine::{Lang, Project};

use crate::connector::{connector_for, PullWarning};

/// Where keys are kept in the credential store; the user is `<tracker>:<site>`.
const SERVICE: &str = "com.hoanghainh.sheafly.tracker";
/// One request, from connecting to the last byte.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// The most a response may hold: 100 issues with long descriptions.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackerError {
    /// No key is saved for the site.
    NoKey,
    /// The tracker refused the key.
    Rejected,
    /// The project (or the address) is not there.
    NotFound,
    /// Too many requests; try again in a minute.
    RateLimited,
    /// No answer: offline, or the site does not exist.
    Offline,
    /// The tracker failed.
    Server,
    /// An answer this app cannot read.
    BadResponse,
    /// A site that is not one of the tracker's.
    BadSite,
    BadProject,
    BadFilter,
    /// More issues than one pull can have.
    TooManyIssues,
    Cancelled,
    /// The credential store failed.
    KeyStore,
    /// The source refused a change (a value it does not accept, or a rule of its own).
    Refused,
    /// No one at the source has the email of the person to assign.
    NoUser,
    /// The project or the settings changed since the preview the push was made from.
    Stale,
    /// A field named in the sync settings is not in the source project, or is not the kind
    /// the sync needs there (a date for a day, a number for points).
    BadField,
}

impl TrackerError {
    pub fn message(self, lang: Lang) -> &'static str {
        let (vi, en) = match self {
            Self::NoKey => (
                "Chưa lưu khóa API cho site này",
                "No API key is saved for this site",
            ),
            Self::Rejected => (
                "Site không nhận khóa API này (sai khóa, hết hạn hoặc không đủ quyền)",
                "The site did not accept this API key (wrong, expired or not allowed)",
            ),
            Self::NotFound => (
                "Không tìm thấy dự án hoặc địa chỉ ở nguồn",
                "The project or address was not found at the source",
            ),
            Self::RateLimited => (
                "Nguồn đang giới hạn số yêu cầu; thử lại sau một phút",
                "The source is limiting requests; try again in a minute",
            ),
            Self::Offline => (
                "Không kết nối được tới nguồn",
                "Could not reach the source",
            ),
            Self::Server => (
                "Nguồn báo lỗi; thử lại sau",
                "The source reported an error; try again later",
            ),
            Self::BadResponse => (
                "Nguồn trả về dữ liệu không đọc được",
                "The source returned data that cannot be read",
            ),
            Self::BadSite => (
                "Site không phải địa chỉ của tracker đã chọn (Backlog: …backlog.com, …backlog.jp hoặc …backlogtool.com)",
                "The site is not an address of the chosen tracker (Backlog: …backlog.com, …backlog.jp or …backlogtool.com)",
            ),
            Self::BadProject => (
                "Khóa dự án chỉ gồm chữ, số, dấu gạch dưới hoặc gạch ngang",
                "The project key can only have letters, digits, underscores or hyphens",
            ),
            Self::BadFilter => (
                "Bộ lọc không hợp lệ (dạng statusId[]=1&keyword=abc, chỉ các tham số được hỗ trợ)",
                "The filter is not valid (like statusId[]=1&keyword=abc, supported parameters only)",
            ),
            Self::TooManyIssues => (
                "Một lần kéo có tối đa 20.000 công việc; thu hẹp bộ lọc",
                "A pull can have at most 20,000 issues; narrow the filter",
            ),
            Self::Cancelled => ("Đã hủy", "Cancelled"),
            Self::KeyStore => (
                "Kho khóa của hệ điều hành báo lỗi",
                "The operating system's credential store reported an error",
            ),
            Self::Refused => (
                "Nguồn từ chối thay đổi này",
                "The source refused this change",
            ),
            Self::NoUser => (
                "Không có người nào ở nguồn dùng email này",
                "No one at the source has this email",
            ),
            Self::Stale => (
                "Dự án hoặc thiết lập đã đổi từ lúc xem trước; hãy xem trước lại",
                "The project or settings changed since the preview; preview again",
            ),
            Self::BadField => (
                "Một trường trong thiết lập đồng bộ không có ở nguồn hoặc không đúng kiểu (ngày cho ngày, số cho điểm)",
                "A field in the sync settings is not at the source or is the wrong kind (a date for a day, a number for points)",
            ),
        };
        match lang {
            Lang::Vi => vi,
            Lang::En | Lang::Ja => en,
        }
    }
}

/// What an adapter needs from the network: a GET of `path` with `query`, answered with
/// the JSON body.
pub trait Api {
    fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value, TrackerError>;
    /// A PATCH of `path` with a form body, answered with the JSON body.
    fn patch(&self, path: &str, form: &[(String, String)]) -> Result<Value, TrackerError>;
    /// A POST of `path` with a JSON body, answered with the JSON body.
    fn post_json(&self, _path: &str, _body: &Value) -> Result<Value, TrackerError> {
        Err(TrackerError::BadResponse)
    }
    /// A PUT of `path` with a JSON body, answered with the JSON body (`Null` when empty).
    fn put_json(&self, _path: &str, _body: &Value) -> Result<Value, TrackerError> {
        Err(TrackerError::BadResponse)
    }
}

/// What the credential store keeps for a tracker's site: a single API key, or an account
/// (an email) and a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credentials {
    ApiKey(String),
    Basic { account: String, token: String },
}

impl Credentials {
    /// The text kept in the credential store: the key itself, or a small JSON object.
    fn to_secret(&self) -> String {
        match self {
            Self::ApiKey(key) => key.clone(),
            Self::Basic { account, token } => serde_json::json!({ "account": account, "token": token }).to_string(),
        }
    }

    /// Read what `to_secret` made. Text that is not such an object is an API key, so keys
    /// saved before there were accounts still read.
    fn from_secret(secret: &str) -> Self {
        if secret.starts_with('{')
            && let Ok(Value::Object(object)) = serde_json::from_str::<Value>(secret)
            && let (Some(account), Some(token)) = (
                object.get("account").and_then(Value::as_str),
                object.get("token").and_then(Value::as_str),
            )
        {
            return Self::Basic { account: account.to_string(), token: token.to_string() };
        }
        Self::ApiKey(secret.to_string())
    }
}

fn kind_name(kind: TrackerKind) -> &'static str {
    match kind {
        TrackerKind::Backlog => "backlog",
        TrackerKind::Jira => "jira",
    }
}

fn entry(kind: TrackerKind, site: &str) -> Result<keyring::Entry, TrackerError> {
    keyring::Entry::new(SERVICE, &format!("{}:{site}", kind_name(kind))).map_err(|_| TrackerError::KeyStore)
}

/// Save (or replace) the credentials for `site` of a tracker.
pub fn save_key(kind: TrackerKind, site: &str, credentials: &Credentials) -> Result<(), TrackerError> {
    if !connector_for(kind).allows_site(site) {
        return Err(TrackerError::BadSite);
    }
    entry(kind, site)?.set_password(&credentials.to_secret()).map_err(|_| TrackerError::KeyStore)
}

/// The credentials for `site`, if any are saved.
pub fn load_key(kind: TrackerKind, site: &str) -> Result<Option<Credentials>, TrackerError> {
    match entry(kind, site)?.get_password() {
        Ok(secret) => Ok(Some(Credentials::from_secret(&secret))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(TrackerError::KeyStore),
    }
}

/// Forget the credentials for `site`; nothing saved is fine.
pub fn forget_key(kind: TrackerKind, site: &str) -> Result<(), TrackerError> {
    match entry(kind, site)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(TrackerError::KeyStore),
    }
}

/// The real network: HTTPS to the space, the credentials in a header, no redirects followed (a
/// redirect could carry them to another host).
pub struct HttpApi {
    agent: ureq::Agent,
    base: String,
    /// The header that authorizes a request, as the connector builds it.
    authorization: (String, String),
}

impl HttpApi {
    /// Ready to call `site` of a tracker with its saved credentials.
    pub fn for_site(kind: TrackerKind, site: &str) -> Result<Self, TrackerError> {
        let connector = connector_for(kind);
        if !connector.allows_site(site) {
            return Err(TrackerError::BadSite);
        }
        let credentials = load_key(kind, site)?.ok_or(TrackerError::NoKey)?;
        let authorization = connector.authorize(&credentials)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .max_redirects(0)
            .https_only(true)
            .user_agent("Sheafly")
            .build()
            .into();
        Ok(Self {
            agent,
            base: format!("https://{site}"),
            authorization,
        })
    }
}

/// What a status the source answered with means.
fn status_error(code: u16) -> TrackerError {
    match code {
        401 | 403 => TrackerError::Rejected,
        404 => TrackerError::NotFound,
        400 | 409 | 422 => TrackerError::Refused,
        429 => TrackerError::RateLimited,
        500..=599 => TrackerError::Server,
        _ => TrackerError::BadResponse,
    }
}

fn read_json(result: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Value, TrackerError> {
    match result {
        Ok(mut response) => {
            let text = response
                .body_mut()
                .with_config()
                .limit(MAX_RESPONSE_BYTES)
                .read_to_string()
                .map_err(|_| TrackerError::BadResponse)?;
            // A change answered with "no content" has no body.
            if text.trim().is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(&text).map_err(|_| TrackerError::BadResponse)
            }
        }
        Err(ureq::Error::StatusCode(code)) => Err(status_error(code)),
        Err(_) => Err(TrackerError::Offline),
    }
}

impl Api for HttpApi {
    fn get(&self, path: &str, query: &[(String, String)]) -> Result<Value, TrackerError> {
        let mut request = self
            .agent
            .get(format!("{}{path}", self.base))
            .header(&self.authorization.0, &self.authorization.1)
            .header("Accept", "application/json");
        for (name, value) in query {
            request = request.query(name, value);
        }
        read_json(request.call())
    }

    fn patch(&self, path: &str, form: &[(String, String)]) -> Result<Value, TrackerError> {
        let request = self
            .agent
            .patch(format!("{}{path}", self.base))
            .header(&self.authorization.0, &self.authorization.1)
            .header("Accept", "application/json");
        read_json(request.send_form(form.iter().map(|(name, value)| (name.as_str(), value.as_str()))))
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<Value, TrackerError> {
        let request = self
            .agent
            .post(format!("{}{path}", self.base))
            .header(&self.authorization.0, &self.authorization.1)
            .header("Accept", "application/json");
        read_json(request.send_json(body))
    }

    fn put_json(&self, path: &str, body: &Value) -> Result<Value, TrackerError> {
        let request = self
            .agent
            .put(format!("{}{path}", self.base))
            .header(&self.authorization.0, &self.authorization.1)
            .header("Accept", "application/json");
        read_json(request.send_json(body))
    }
}

/// A problem with one row of a pull, said in the user's language.
#[derive(Debug, Serialize)]
pub struct PullProblem {
    pub message: String,
    /// The row of the pull, from 0; none for the pull as a whole.
    pub row: Option<u32>,
    pub key: String,
    pub column: PullColumn,
}

/// What a pull would do to the project as it is now.
#[derive(Debug, Serialize)]
pub struct PullChanges {
    pub changes: Vec<PullRowDiff>,
    pub gone: Vec<GoneTask>,
    pub problems: Vec<PullProblem>,
    /// Fields changed on both sides that the resolutions given do not settle.
    pub conflicts: Vec<PullConflict>,
    /// A base would be updated without a task changing (a conflict settled by keeping
    /// Sheafly): worth applying on its own.
    pub bases_changed: bool,
}

/// What a pull found and what it would change. The frontend applies it by sending the
/// rows back in `apply_pull`, which checks them again.
#[derive(Debug, Serialize)]
pub struct PullPreview {
    pub rows: Vec<PullTask>,
    pub pulled_at: i64,
    pub warnings: Vec<PullWarning>,
    #[serde(flatten)]
    pub changes: PullChanges,
}

/// The three-way `pull::diff_with` of `rows` against `project` under `resolutions`, with the
/// problems worded for `lang`.
pub fn changes(
    project: &Project,
    rows: &[PullTask],
    pulled_at: i64,
    resolutions: &[Resolution],
    lang: Lang,
) -> PullChanges {
    // Always the three-way pull: a field moves only when the source changed it and
    // Sheafly did not, and fields changed on both sides wait for a choice.
    let diff = pull::diff_with(project, rows, pulled_at, Some(resolutions));
    PullChanges {
        problems: diff
            .errors
            .iter()
            .map(|error| PullProblem {
                message: error.message(lang),
                row: error.row,
                key: error.key.clone(),
                column: error.column.clone(),
            })
            .collect(),
        changes: diff.rows,
        gone: diff.gone,
        conflicts: diff.conflicts,
        bases_changed: diff.bases_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_through_the_secret_and_old_keys_still_read() {
        let key = Credentials::ApiKey("abc123".into());
        assert_eq!(Credentials::from_secret(&key.to_secret()), key);
        let basic = Credentials::Basic { account: "an@example.com".into(), token: "tok\"en".into() };
        assert_eq!(Credentials::from_secret(&basic.to_secret()), basic);
        // A key saved before accounts existed is plain text; so is text that merely starts with {.
        assert_eq!(Credentials::from_secret("plain"), Credentials::ApiKey("plain".into()));
        assert_eq!(Credentials::from_secret("{not json"), Credentials::ApiKey("{not json".into()));
        assert_eq!(Credentials::from_secret("{\"account\":\"a\"}"), Credentials::ApiKey("{\"account\":\"a\"}".into()));
    }

    #[test]
    fn a_host_that_does_not_answer_is_offline_and_the_key_is_not_in_the_error() {
        let api = HttpApi {
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .max_redirects(0)
                .https_only(true)
                .build()
                .into(),
            base: "https://127.0.0.1:9".into(),
            authorization: ("Backlog-API-Key".into(), "SECRET-KEY".into()),
        };
        let error = api.get("/api/v2/projects/X", &[]).unwrap_err();
        assert_eq!(error, TrackerError::Offline);
        for lang in [Lang::Vi, Lang::En] {
            assert!(!error.message(lang).contains("SECRET"));
        }
    }

    #[test]
    fn a_field_changed_on_both_sides_is_a_conflict_until_a_choice_settles_it() {
        use sheafly_engine::sync::Side;
        use sheafly_engine::{Command, Document};

        let row = || PullTask { key: "P-1".into(), name: "Ở nguồn".into(), ..PullTask::default() };
        let mut doc = Document::new_default();
        doc.apply(Command::ApplyPull { rows: vec![row()], pulled_at: 1, resolutions: Some(vec![]) }).unwrap();
        let uid = doc.project.tasks.iter().find(|task| task.external.is_some()).unwrap().uid;
        doc.apply(Command::RenameTask { uid, name: "Ở Sheafly".into() }).unwrap();
        let mut changed = row();
        changed.name = "Nguồn đổi".into();

        let open = changes(&doc.project, &[changed.clone()], 2, &[], Lang::En);
        assert_eq!(open.conflicts.len(), 1);
        assert_eq!(open.conflicts[0].field, "name");
        assert!(!open.bases_changed);

        let keep = [Resolution { key: "P-1".into(), field: "name".into(), keep: Side::Sheafly }];
        let settled = changes(&doc.project, &[changed], 2, &keep, Lang::En);
        assert!(settled.conflicts.is_empty());
        assert!(settled.bases_changed, "keeping Sheafly is itself worth recording");
    }

    #[test]
    fn statuses_mean_what_the_user_can_act_on() {
        assert_eq!(status_error(401), TrackerError::Rejected);
        assert_eq!(status_error(403), TrackerError::Rejected);
        assert_eq!(status_error(404), TrackerError::NotFound);
        assert_eq!(status_error(422), TrackerError::Refused);
        assert_eq!(status_error(429), TrackerError::RateLimited);
        assert_eq!(status_error(503), TrackerError::Server);
        assert_eq!(status_error(302), TrackerError::BadResponse);
    }

    #[test]
    fn every_error_has_both_languages() {
        use TrackerError::*;
        for error in [
            NoKey, Rejected, NotFound, RateLimited, Offline, Server, BadResponse, BadSite,
            BadProject, BadFilter, TooManyIssues, Cancelled, KeyStore, Refused, NoUser, Stale, BadField,
        ] {
            let (vi, en) = (error.message(Lang::Vi), error.message(Lang::En));
            assert!(!vi.is_empty() && !en.is_empty() && vi != en, "{error:?}");
        }
    }
}
