//! The shapes of AI recommendations (phase 22, PR C, D and E, sheafly-pro#1), shared by the Pro crate
//! that reads a model's replies, its public stub and the shell that runs the requests.
//! Nothing here talks to a model: the Pro crate builds requests and reads replies into these,
//! the shell sends them (decision D7).
//!
//! Tasks and people are named as in the project summary, `T42` and `R7` (decision D3); the
//! interface puts the real names in when it shows a card.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::command::Command;
use crate::import::ImportTask;

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

/// What to do after reading a reply: `T` is the answer of the kind asked for, the risk cards
/// (kind 1) by default, or `Fixes` (kind 2).
#[derive(Clone, Debug, PartialEq)]
pub enum AdviceStep<T = Vec<RiskCard>> {
    /// Send this body next: the conversation so far with the answers to the model's tools.
    Ask(Value),
    /// The model answered.
    Done(T),
}

/// What a proposal sets out to improve (decision D5): the measure its trial is judged by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Goal {
    /// Finish this task earlier, toward its deadline.
    Deadline { task: u32 },
    /// Take work off this person where they are overallocated.
    Overallocation { resource: u32 },
    /// Finish the project earlier.
    Finish,
    /// Put right this task's estimate or logic (kind 3, check estimates): no measure of its
    /// own, so the trial judges it by the four measures alone.
    Estimate { task: u32 },
}

/// A change the model proposes (kind 2, phase 22 PR D): commands the engine tries on a copy
/// before the user sees them, and that only `ApplyProposal` applies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// The model's tool call it came from, to answer it on a retry.
    pub id: String,
    pub goal: Goal,
    /// Only commands `allowed_in_proposal` accepts, never `ApplyLeveling`: see `level`.
    pub commands: Vec<Command>,
    /// Level the people after the commands, as `LevelResources` would; the trial turns the
    /// result into an `ApplyLeveling` at the end of the commands (decision D1).
    #[serde(default)]
    pub level: bool,
    /// Why, in the interface language, with tasks and people as `T42` and `R7`.
    pub reason: String,
}

/// The answer of kind 2: the proposals, or none and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fixes {
    pub proposals: Vec<Proposal>,
    /// What the model said when it had nothing to propose; empty otherwise.
    #[serde(default)]
    pub note: String,
}

/// The answer of kind 3, check estimates (phase 22 PR E): what looks wrong, as cards, and
/// proposals to put some of it right, tried like those of kind 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EstimateReview {
    pub cards: Vec<RiskCard>,
    #[serde(default)]
    pub proposals: Vec<Proposal>,
    /// What the model said when it found nothing; empty otherwise.
    #[serde(default)]
    pub note: String,
}

/// The answer of kind 4, a work breakdown built from a description: the rows of one
/// `ImportRows`, people by their real names, for the user to check and import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wbs {
    pub tasks: Vec<ImportTask>,
    /// What the model wants the user to know (assumptions, what it left out); may be empty.
    #[serde(default)]
    pub note: String,
}

/// The answer of kind 5, a status report for stakeholders, as Markdown with tasks and
/// people as `T42` and `R7`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReport {
    pub title: String,
    pub markdown: String,
}
