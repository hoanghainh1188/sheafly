//! The shapes of AI recommendations (phase 22, PR C, sheafly-pro#1), shared by the Pro crate
//! that reads a model's replies, its public stub and the shell that runs the requests.
//! Nothing here talks to a model: the Pro crate builds requests and reads replies into these,
//! the shell sends them (decision D7).
//!
//! Tasks and people are named as in the project summary, `T42` and `R7` (decision D3); the
//! interface puts the real names in when it shows a card.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The most rounds of read tools one request may take before it must answer (decision D4).
pub const MAX_TOOL_ROUNDS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    High,
    Medium,
    Low,
}

/// One problem the model found (kind 1, explain risks): read only, no change to the project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskCard {
    pub severity: Severity,
    /// A short title, in the interface language.
    pub title: String,
    /// What is at risk: a deadline, a milestone, the finish, a person's weeks.
    pub what: String,
    /// Why, from the engine's numbers.
    pub why: String,
    /// The chain of tasks driving the risk, earliest first (`T42`).
    #[serde(default)]
    pub chain: Vec<String>,
    /// What to look at first.
    pub focus: String,
    /// Other tasks the card is about.
    #[serde(default)]
    pub tasks: Vec<String>,
    /// People the card is about (`R7`).
    #[serde(default)]
    pub people: Vec<String>,
}

/// What to do after reading a reply.
#[derive(Clone, Debug, PartialEq)]
pub enum AdviceStep {
    /// Send this body next: the conversation so far with the answers to the model's tools.
    Ask(Value),
    /// The model answered.
    Done(Vec<RiskCard>),
}
