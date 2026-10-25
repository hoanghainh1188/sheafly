//! Talking to the AI provider (phase 22, PR B): the Claude Messages API.
//!
//! As with task trackers, every call runs here, in the shell, never in the webview. The
//! user's own API key lives in the operating system's credential store, is read only to
//! make a request, and goes in a header to `https://api.anthropic.com` alone, with no
//! redirect followed. Errors are a fixed set with fixed texts: nothing the provider or the
//! network said reaches the user. Nothing is built, read or sent while AI is off or its data
//! notice has not been accepted (`connect`), so with AI off the app makes no network call.
//!
//! What a request holds (the project summary, the tools, the reading of a reply) is the
//! Pro crate's part from PR C on; this module only carries a body and its answer, through
//! `AiApi`, so it is tested against handwritten replies and CI makes no network call.
//! `run_advice` is the loop of one recommendation request (PR C): send, add up the
//! tokens, let the Pro crate read the reply, and stop when asked between requests.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use sheafly_engine::advice::{AdviceStep, Fixes, MAX_TOOL_ROUNDS};
use sheafly_engine::Lang;

use crate::settings::AiSettings;

/// Where the key is kept in the credential store, and under which user.
const SERVICE: &str = "com.hoanghainh.sheafly.ai";
const KEY_USER: &str = "anthropic";
const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
/// One request, from connecting to the last byte: a long answer takes a while.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
/// The default model (D8): the latest; `claude-sonnet-5-5` is the cheaper, faster choice.
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";
const MAX_MODEL_CHARS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiError {
    /// AI is turned off.
    Disabled,
    /// AI cannot be turned on before the data notice is accepted.
    NoticeNotAccepted,
    /// This computer's license does not include AI recommendations.
    NeedsPro,
    /// No API key is saved.
    NoKey,
    /// The provider refused the key (wrong, revoked, or without access).
    Rejected,
    /// A model ID the provider does not have, or one that is not a model ID at all.
    BadModel,
    /// The provider would not take the request (often: no credit left).
    Refused,
    /// The request is too large.
    TooLarge,
    RateLimited,
    /// The provider is overloaded for now.
    Overloaded,
    Server,
    Offline,
    /// An answer this app cannot read.
    BadResponse,
    /// The credential store failed.
    KeyStore,
}

impl AiError {
    pub fn message(self, lang: Lang) -> &'static str {
        let (vi, en) = match self {
            Self::Disabled => ("Gợi ý AI đang tắt (Trợ giúp → Gợi ý AI…)", "AI recommendations are off (Help → AI Recommendations…)"),
            Self::NoticeNotAccepted => (
                "Hãy đọc và đồng ý thông báo về dữ liệu trước khi bật gợi ý AI",
                "Read and accept the data notice before turning AI recommendations on",
            ),
            Self::NeedsPro => (
                "Cần Sheafly Pro để dùng gợi ý AI (Trợ giúp → License Sheafly Pro…)",
                "AI recommendations need Sheafly Pro (Help → Sheafly Pro License…)",
            ),
            Self::NoKey => ("Chưa lưu khóa API Anthropic", "No Anthropic API key is saved"),
            Self::Rejected => (
                "Anthropic không nhận khóa API này (sai khóa, đã thu hồi hoặc không đủ quyền)",
                "Anthropic did not accept this API key (wrong, revoked or not allowed)",
            ),
            Self::BadModel => (
                "Anthropic không có model này; kiểm tra ID model",
                "Anthropic has no such model; check the model ID",
            ),
            Self::Refused => (
                "Anthropic không nhận yêu cầu này (có thể tài khoản đã hết tín dụng)",
                "Anthropic did not accept this request (the account may be out of credit)",
            ),
            Self::TooLarge => ("Yêu cầu quá lớn để gửi", "The request is too large to send"),
            Self::RateLimited => (
                "Anthropic đang giới hạn số yêu cầu; thử lại sau một phút",
                "Anthropic is limiting requests; try again in a minute",
            ),
            Self::Overloaded => ("Anthropic đang quá tải; thử lại sau", "Anthropic is overloaded; try again later"),
            Self::Server => ("Anthropic báo lỗi; thử lại sau", "Anthropic reported an error; try again later"),
            Self::Offline => ("Không kết nối được tới Anthropic", "Could not reach Anthropic"),
            Self::BadResponse => ("Anthropic trả về dữ liệu không đọc được", "Anthropic returned data that cannot be read"),
            Self::KeyStore => ("Kho khóa của hệ điều hành báo lỗi", "The operating system's credential store reported an error"),
        };
        match lang {
            Lang::Vi => vi,
            Lang::En => en,
        }
    }
}

/// What the AI layer needs from the network: one Messages API request, answered with its JSON.
pub trait AiApi {
    fn messages(&self, body: &Value) -> Result<Value, AiError>;
}

/// A model ID as the provider writes them: letters, digits, `.`, `-`, `_`, `:`.
pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_CHARS
        && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
}

/// Check settings before saving them: a model ID, and no AI turned on without the notice.
pub fn check_settings(settings: &AiSettings) -> Result<(), AiError> {
    if !valid_model(&settings.model) {
        return Err(AiError::BadModel);
    }
    if settings.enabled && !settings.notice_accepted {
        return Err(AiError::NoticeNotAccepted);
    }
    Ok(())
}

fn entry() -> Result<keyring::Entry, AiError> {
    keyring::Entry::new(SERVICE, KEY_USER).map_err(|_| AiError::KeyStore)
}

pub fn save_key(key: &str) -> Result<(), AiError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AiError::NoKey);
    }
    entry()?.set_password(key).map_err(|_| AiError::KeyStore)
}

pub fn load_key() -> Result<Option<String>, AiError> {
    match entry()?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(AiError::KeyStore),
    }
}

/// Forget the key; nothing saved is fine.
pub fn forget_key() -> Result<(), AiError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(AiError::KeyStore),
    }
}

/// The real network: HTTPS to Anthropic only, the key in `x-api-key`, no redirects.
pub struct HttpAiApi {
    agent: ureq::Agent,
    endpoint: String,
    key: String,
}

/// The client for a call, only when AI is on, its notice accepted and the license includes
/// it; the key is read last, so a call that may not go out never touches the credential
/// store or the network.
pub fn connect(
    settings: &AiSettings,
    licensed: bool,
    load_key: impl FnOnce() -> Result<Option<String>, AiError>,
) -> Result<HttpAiApi, AiError> {
    if !settings.enabled {
        return Err(AiError::Disabled);
    }
    if !settings.notice_accepted {
        return Err(AiError::NoticeNotAccepted);
    }
    if !licensed {
        return Err(AiError::NeedsPro);
    }
    let key = load_key()?.ok_or(AiError::NoKey)?;
    Ok(HttpAiApi::new(ENDPOINT, key))
}

impl HttpAiApi {
    fn new(endpoint: &str, key: String) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .max_redirects(0)
            .https_only(true)
            .user_agent("Sheafly")
            .build()
            .into();
        Self { agent, endpoint: endpoint.to_string(), key }
    }
}

/// What a status the provider answered with means.
fn status_error(code: u16) -> AiError {
    match code {
        400 => AiError::Refused,
        401 | 403 => AiError::Rejected,
        404 => AiError::BadModel,
        413 => AiError::TooLarge,
        429 => AiError::RateLimited,
        529 => AiError::Overloaded,
        500..=599 => AiError::Server,
        _ => AiError::BadResponse,
    }
}

impl AiApi for HttpAiApi {
    fn messages(&self, body: &Value) -> Result<Value, AiError> {
        let result = self
            .agent
            .post(&self.endpoint)
            .header("x-api-key", &self.key)
            .header("anthropic-version", API_VERSION)
            .header("Accept", "application/json")
            .send_json(body);
        match result {
            Ok(mut response) => {
                let text = response
                    .body_mut()
                    .with_config()
                    .limit(MAX_RESPONSE_BYTES)
                    .read_to_string()
                    .map_err(|_| AiError::BadResponse)?;
                serde_json::from_str(&text).map_err(|_| AiError::BadResponse)
            }
            Err(ureq::Error::StatusCode(code)) => Err(status_error(code)),
            Err(_) => Err(AiError::Offline),
        }
    }
}

/// What a connection check found: the model that answered and the tokens it took.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct AiCheck {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Check the key and the model with the smallest request there is.
pub fn check(api: &dyn AiApi, model: &str) -> Result<AiCheck, AiError> {
    let body = json!({
        "model": model,
        "max_tokens": 8,
        "messages": [{ "role": "user", "content": "Reply with OK." }],
    });
    let reply = api.messages(&body)?;
    let usage = |name: &str| reply.pointer(&format!("/usage/{name}")).and_then(Value::as_u64);
    match (reply.get("type").and_then(Value::as_str), reply.get("model").and_then(Value::as_str)) {
        (Some("message"), Some(model)) => Ok(AiCheck {
            model: model.to_string(),
            input_tokens: usage("input_tokens").unwrap_or(0),
            output_tokens: usage("output_tokens").unwrap_or(0),
        }),
        _ => Err(AiError::BadResponse),
    }
}

/// What one recommendation request found (`T`: the risk cards, or the proposals), and what
/// it cost.
#[derive(Debug, PartialEq)]
pub struct AdviceRun<T> {
    pub answer: T,
    /// Input tokens of every request, the cached prefix included.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub requests: u32,
    /// The last body sent and its reply, for a retry that continues the conversation.
    pub body: Value,
    pub reply: Value,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// Stopped between two requests: a newer request, or the panel closed.
    Cancelled,
    Ai(AiError),
    /// What the Pro crate said about a reply, already in the user's language.
    Reply(String),
}

/// Input and output tokens a reply says it took.
fn tokens(reply: &Value) -> (u64, u64) {
    let usage = |name: &str| reply.pointer(&format!("/usage/{name}")).and_then(Value::as_u64).unwrap_or(0);
    (
        usage("input_tokens") + usage("cache_creation_input_tokens") + usage("cache_read_input_tokens"),
        usage("output_tokens"),
    )
}

/// One recommendation request: send `first`, hand each reply to `next` (the Pro crate's
/// reading) and send the body it asks for, until it has the answer. `cancel` is checked
/// before every request; at most one more request than `MAX_TOOL_ROUNDS` goes out, since
/// the reading forces the answer after the last tool round.
pub fn run_advice<T>(
    api: &dyn AiApi,
    first: Value,
    next: impl Fn(&Value, &Value) -> Result<AdviceStep<T>, String>,
    cancel: &AtomicBool,
) -> Result<AdviceRun<T>, RunError> {
    let (mut body, mut input_tokens, mut output_tokens) = (first, 0, 0);
    for requests in 1..=MAX_TOOL_ROUNDS as u32 + 1 {
        if cancel.load(Ordering::Relaxed) {
            return Err(RunError::Cancelled);
        }
        let reply = api.messages(&body).map_err(RunError::Ai)?;
        let (input, output) = tokens(&reply);
        input_tokens += input;
        output_tokens += output;
        match next(&body, &reply).map_err(RunError::Reply)? {
            AdviceStep::Done(answer) => {
                return Ok(AdviceRun { answer, input_tokens, output_tokens, requests, body, reply })
            }
            AdviceStep::Ask(more) => body = more,
        }
    }
    Err(RunError::Ai(AiError::BadResponse))
}

/// What a request to fix the project found (kind 2), after the one retry, and what it cost.
#[derive(Debug, PartialEq)]
pub struct FixRun {
    pub fixes: Fixes,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub requests: u32,
}

/// A request to fix the project (kind 2, phase 22 PR D): `run_advice` until the model
/// proposes, then `refuses` (the engine's trial) on every proposal. When the engine refuses
/// some, `retry` builds the body that sends its words back, once (decision D5); the
/// proposals of that answer replace the refused ones. A retry that fails, or that proposes
/// nothing, keeps the refused proposals, so the user sees what the engine said. Stopping
/// during the retry stops the whole request.
pub fn run_fixes(
    api: &dyn AiApi,
    first: Value,
    next: impl Fn(&Value, &Value) -> Result<AdviceStep<Fixes>, String>,
    retry: impl Fn(&Value, &Value, &[(String, String)]) -> Result<Value, String>,
    refuses: impl Fn(&sheafly_engine::advice::Proposal) -> Option<String>,
    cancel: &AtomicBool,
) -> Result<FixRun, RunError> {
    let run = run_advice(api, first, &next, cancel)?;
    let mut done = FixRun { fixes: run.answer, input_tokens: run.input_tokens, output_tokens: run.output_tokens, requests: run.requests };
    let refused: Vec<(String, String)> = done
        .fixes
        .proposals
        .iter()
        .filter_map(|proposal| refuses(proposal).map(|message| (proposal.id.clone(), message)))
        .collect();
    if refused.is_empty() {
        return Ok(done);
    }
    let Ok(body) = retry(&run.body, &run.reply, &refused) else {
        return Ok(done);
    };
    match run_advice(api, body, &next, cancel) {
        Ok(again) => {
            done.input_tokens += again.input_tokens;
            done.output_tokens += again.output_tokens;
            done.requests += again.requests;
            if !again.answer.proposals.is_empty() {
                done.fixes.proposals.retain(|proposal| !refused.iter().any(|(id, _)| *id == proposal.id));
                done.fixes.proposals.extend(again.answer.proposals);
            }
            Ok(done)
        }
        Err(RunError::Cancelled) => Err(RunError::Cancelled),
        Err(_) => Ok(done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sheafly_engine::advice::RiskCard;
    use std::cell::RefCell;

    /// Answers with a handwritten reply and keeps the body it was sent.
    struct Fixture {
        reply: Result<Value, AiError>,
        sent: RefCell<Vec<Value>>,
    }

    impl AiApi for Fixture {
        fn messages(&self, body: &Value) -> Result<Value, AiError> {
            self.sent.borrow_mut().push(body.clone());
            self.reply.clone()
        }
    }

    fn fixture(name: &str) -> Value {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn on() -> AiSettings {
        AiSettings { enabled: true, notice_accepted: true, ..AiSettings::default() }
    }

    #[test]
    fn nothing_is_read_or_built_unless_ai_is_on_accepted_and_licensed() {
        let never = || -> Result<Option<String>, AiError> { panic!("the key must not be read") };
        assert_eq!(connect(&AiSettings::default(), true, never).err(), Some(AiError::Disabled));
        let unaccepted = AiSettings { enabled: true, ..AiSettings::default() };
        assert_eq!(connect(&unaccepted, true, never).err(), Some(AiError::NoticeNotAccepted));
        assert_eq!(connect(&on(), false, never).err(), Some(AiError::NeedsPro));
        assert_eq!(connect(&on(), true, || Ok(None)).err(), Some(AiError::NoKey));
        let api = connect(&on(), true, || Ok(Some("sk-test".into()))).unwrap();
        assert_eq!(api.endpoint, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn settings_need_a_model_id_and_the_notice_before_ai_turns_on() {
        assert_eq!(check_settings(&AiSettings::default()), Ok(()));
        assert_eq!(check_settings(&on()), Ok(()));
        let unaccepted = AiSettings { enabled: true, ..AiSettings::default() };
        assert_eq!(check_settings(&unaccepted), Err(AiError::NoticeNotAccepted));
        for model in ["", "claude opus", "a/b", &"x".repeat(101)] {
            let settings = AiSettings { model: model.to_string(), ..AiSettings::default() };
            assert_eq!(check_settings(&settings), Err(AiError::BadModel), "{model:?}");
        }
        assert!(valid_model("claude-sonnet-5-5") && valid_model("claude-3-5-haiku-20241022"));
    }

    #[test]
    fn a_check_sends_the_model_and_reads_the_reply() {
        let api = Fixture { reply: Ok(fixture("ai_check_ok.json")), sent: RefCell::new(vec![]) };
        let found = check(&api, "claude-sonnet-5-5").unwrap();
        assert_eq!(found, AiCheck { model: "claude-sonnet-5-5".into(), input_tokens: 12, output_tokens: 3 });
        let sent = api.sent.borrow();
        assert_eq!(sent[0]["model"], "claude-sonnet-5-5");
        assert!(sent[0]["max_tokens"].as_u64().unwrap() <= 16);
    }

    #[test]
    fn a_reply_that_is_not_a_message_is_unreadable_and_errors_pass_through() {
        let error_body = Fixture { reply: Ok(fixture("ai_error.json")), sent: RefCell::new(vec![]) };
        assert_eq!(check(&error_body, DEFAULT_MODEL), Err(AiError::BadResponse));
        let not_json_shape = Fixture { reply: Ok(json!([1, 2])), sent: RefCell::new(vec![]) };
        assert_eq!(check(&not_json_shape, DEFAULT_MODEL), Err(AiError::BadResponse));
        let refused = Fixture { reply: Err(AiError::Rejected), sent: RefCell::new(vec![]) };
        assert_eq!(check(&refused, DEFAULT_MODEL), Err(AiError::Rejected));
    }

    #[test]
    fn statuses_mean_what_the_user_can_act_on() {
        assert_eq!(status_error(400), AiError::Refused);
        assert_eq!(status_error(401), AiError::Rejected);
        assert_eq!(status_error(403), AiError::Rejected);
        assert_eq!(status_error(404), AiError::BadModel);
        assert_eq!(status_error(413), AiError::TooLarge);
        assert_eq!(status_error(429), AiError::RateLimited);
        assert_eq!(status_error(529), AiError::Overloaded);
        assert_eq!(status_error(500), AiError::Server);
        assert_eq!(status_error(302), AiError::BadResponse);
    }

    #[test]
    fn a_host_that_does_not_answer_is_offline_and_the_key_is_not_in_the_error() {
        let api = HttpAiApi::new("https://127.0.0.1:9/v1/messages", "SECRET-KEY".into());
        let error = api.messages(&json!({})).unwrap_err();
        assert_eq!(error, AiError::Offline);
        for lang in [Lang::Vi, Lang::En] {
            assert!(!error.message(lang).contains("SECRET"));
        }
    }

    /// Answers with handwritten replies in turn and keeps every body it was sent; sets
    /// `cancel_after` once that many requests went out.
    struct Script {
        replies: Vec<Result<Value, AiError>>,
        sent: RefCell<Vec<Value>>,
        cancel_after: Option<(usize, &'static AtomicBool)>,
    }

    impl AiApi for Script {
        fn messages(&self, body: &Value) -> Result<Value, AiError> {
            let mut sent = self.sent.borrow_mut();
            sent.push(body.clone());
            if let Some((_, flag)) = self.cancel_after.filter(|(count, _)| sent.len() >= *count) {
                flag.store(true, Ordering::Relaxed);
            }
            self.replies[(sent.len() - 1).min(self.replies.len() - 1)].clone()
        }
    }

    fn script(names: &[&str]) -> Script {
        Script { replies: names.iter().map(|name| Ok(fixture(name))).collect(), sent: RefCell::new(vec![]), cancel_after: None }
    }

    fn card() -> RiskCard {
        RiskCard {
            severity: sheafly_engine::advice::Severity::High,
            title: "T4".into(),
            what: "w".into(),
            why: "y".into(),
            chain: vec!["T1".into()],
            focus: "f".into(),
            tasks: vec![],
            people: vec![],
        }
    }

    /// Stands in for the Pro crate: the answer is done, a tool call asks for round n + 1.
    fn reading(body: &Value, reply: &Value) -> Result<AdviceStep, String> {
        if reply.pointer("/content/0/name").and_then(Value::as_str) == Some("report_risks") {
            return Ok(AdviceStep::Done(vec![card()]));
        }
        let round = body["round"].as_u64().unwrap_or(0);
        Ok(AdviceStep::Ask(json!({ "round": round + 1 })))
    }

    #[test]
    fn a_request_runs_its_rounds_and_adds_up_the_tokens() {
        let api = script(&["ai_risks_tools.json", "ai_risks_tools.json", "ai_risks_report.json"]);
        let cancel = AtomicBool::new(false);
        let run = run_advice(&api, json!({ "round": 0 }), reading, &cancel).unwrap();
        assert_eq!(run.answer, vec![card()]);
        assert_eq!(run.requests, 3);
        // Each tool reply: 2100 + 1800 cached; the answer: 400 + 1800 read from the cache.
        assert_eq!(run.input_tokens, 2 * 3900 + 2200);
        assert_eq!(run.output_tokens, 2 * 120 + 300);
        let rounds: Vec<u64> = api.sent.borrow().iter().map(|body| body["round"].as_u64().unwrap()).collect();
        assert_eq!(rounds, [0, 1, 2], "each request sends the body the reading asked for");
    }

    #[test]
    fn a_cancelled_request_stops_before_the_next_send() {
        let api = script(&["ai_risks_tools.json"]);
        let cancel = AtomicBool::new(true);
        assert_eq!(run_advice(&api, json!({}), reading, &cancel), Err(RunError::Cancelled));
        assert!(api.sent.borrow().is_empty());

        static STOP: AtomicBool = AtomicBool::new(false);
        let api = Script { cancel_after: Some((1, &STOP)), ..script(&["ai_risks_tools.json", "ai_risks_report.json"]) };
        assert_eq!(run_advice(&api, json!({}), reading, &STOP), Err(RunError::Cancelled));
        assert_eq!(api.sent.borrow().len(), 1, "the reply in flight is the last");
    }

    #[test]
    fn errors_of_the_network_and_of_the_reading_end_the_request() {
        let cancel = AtomicBool::new(false);
        let offline = Script { replies: vec![Err(AiError::Offline)], sent: RefCell::new(vec![]), cancel_after: None };
        assert_eq!(run_advice(&offline, json!({}), reading, &cancel), Err(RunError::Ai(AiError::Offline)));
        let api = script(&["ai_risks_tools.json"]);
        let refuse = |_: &Value, _: &Value| -> Result<AdviceStep, String> { Err("Không đọc được câu trả lời của AI".into()) };
        assert_eq!(run_advice(&api, json!({}), refuse, &cancel), Err(RunError::Reply("Không đọc được câu trả lời của AI".into())));
    }

    #[test]
    fn a_model_that_never_answers_stops_after_the_last_round() {
        let api = script(&["ai_risks_tools.json"]);
        let cancel = AtomicBool::new(false);
        assert_eq!(run_advice(&api, json!({}), reading, &cancel), Err(RunError::Ai(AiError::BadResponse)));
        assert_eq!(api.sent.borrow().len(), MAX_TOOL_ROUNDS + 1);
    }

    /// Stands in for the Pro crate's kind 2 reading: every call of the reply is a proposal.
    fn proposals(_: &Value, reply: &Value) -> Result<AdviceStep<Fixes>, String> {
        let proposals = reply["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|call| sheafly_engine::advice::Proposal {
                id: call["id"].as_str().unwrap().into(),
                goal: sheafly_engine::advice::Goal::Finish,
                commands: vec![],
                level: true,
                reason: "r".into(),
            })
            .collect();
        Ok(AdviceStep::Done(Fixes { proposals, note: String::new() }))
    }

    fn retry(body: &Value, _: &Value, refused: &[(String, String)]) -> Result<Value, String> {
        Ok(json!({ "retry_of": body, "refused": refused.iter().map(|(id, message)| json!([id, message])).collect::<Vec<_>>() }))
    }

    /// The engine refuses `toolu_41` of `ai_fixes_refused.json`.
    fn refuses(proposal: &sheafly_engine::advice::Proposal) -> Option<String> {
        (proposal.id == "toolu_41").then(|| "Không tìm thấy công việc".to_string())
    }

    fn ids(run: &FixRun) -> Vec<&str> {
        run.fixes.proposals.iter().map(|proposal| proposal.id.as_str()).collect()
    }

    #[test]
    fn a_refused_proposal_goes_back_once_and_its_correction_replaces_it() {
        let api = script(&["ai_fixes_refused.json", "ai_fixes_retried.json"]);
        let cancel = AtomicBool::new(false);
        let run = run_fixes(&api, json!({ "first": true }), proposals, retry, refuses, &cancel).unwrap();
        assert_eq!(ids(&run), ["toolu_42", "toolu_51"]);
        assert_eq!(run.requests, 2);
        assert_eq!(run.input_tokens, 2 * (2400 + 1900));
        let sent = api.sent.borrow();
        assert_eq!(sent[1]["refused"], json!([["toolu_41", "Không tìm thấy công việc"]]));
        assert_eq!(sent[1]["retry_of"], json!({ "first": true }), "the retry continues the last body");
    }

    #[test]
    fn nothing_refused_means_no_retry_and_a_failed_retry_keeps_what_was_refused() {
        let cancel = AtomicBool::new(false);
        let api = script(&["ai_fixes_retried.json"]);
        let run = run_fixes(&api, json!({}), proposals, retry, refuses, &cancel).unwrap();
        assert_eq!((ids(&run), run.requests), (vec!["toolu_51"], 1));

        let api = Script { replies: vec![Ok(fixture("ai_fixes_refused.json")), Err(AiError::Overloaded)], ..script(&[]) };
        let run = run_fixes(&api, json!({}), proposals, retry, refuses, &cancel).unwrap();
        assert_eq!(ids(&run), ["toolu_41", "toolu_42"], "the user still sees what the engine refused");

        static STOP: AtomicBool = AtomicBool::new(false);
        let api = Script { cancel_after: Some((1, &STOP)), ..script(&["ai_fixes_refused.json", "ai_fixes_retried.json"]) };
        assert_eq!(run_fixes(&api, json!({}), proposals, retry, refuses, &STOP), Err(RunError::Cancelled));
    }

    #[test]
    fn every_error_has_both_languages() {
        use AiError::*;
        for error in [
            Disabled, NoticeNotAccepted, NeedsPro, NoKey, Rejected, BadModel, Refused, TooLarge,
            RateLimited, Overloaded, Server, Offline, BadResponse, KeyStore,
        ] {
            let (vi, en) = (error.message(Lang::Vi), error.message(Lang::En));
            assert!(!vi.is_empty() && !en.is_empty() && vi != en, "{error:?}");
        }
    }
}
