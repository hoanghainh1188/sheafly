//! The shape of a schedule health report (DCMA 14-point assessment, plan phase 23).
//!
//! Only the types live here. The scoring of the 14 criteria is a Pro module
//! (`sheafly_pro::schedule_health`, closed source, plan part 20G D12); this module gives the
//! stub and the private crate one definition, and the frontend's `types.ts` a public one.

use serde::{Deserialize, Serialize};

/// The whole assessment: the 14 criteria in order and the overall result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthReport {
    pub criteria: Vec<HealthCriterion>,
    /// Criteria that passed, out of `applicable` (criterion 10 has no threshold and
    /// counts in neither).
    pub passed: u32,
    pub applicable: u32,
    /// Sheafly's rating, not DCMA's (DCMA has no overall score); `None` when no criterion
    /// applies.
    pub rating: Option<HealthRating>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthRating {
    /// Every criterion that applies passes.
    Good,
    /// One or two fail, none of them 2, 7, 9 or 12.
    Acceptable,
    AtRisk,
}

/// One criterion, numbered as DCMA numbers them (1 Logic … 14 BEI).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HealthCriterion {
    pub number: u8,
    pub status: HealthStatus,
    /// Why the criterion does not apply; set only with `HealthStatus::NotApplicable`.
    pub not_applicable: Option<NotApplicable>,
    /// What fails the criterion (tasks, or links for 2–4) and what it is counted against;
    /// for 14 the tasks finished and the tasks due (the index), for 12 and 13 zero.
    pub count: u32,
    pub total: u32,
    /// The number compared with the limit: a share for 1–11 (for 4, the share of
    /// finish-to-start links), the index for 13 and 14; `None` for 12 and when not applicable.
    pub value: Option<f64>,
    pub limit: Option<HealthLimit>,
    /// The tasks to look at, in project order, each with why.
    pub tasks: Vec<HealthTask>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Pass,
    Fail,
    /// Reported without a threshold (criterion 10).
    Info,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HealthLimit {
    AtMost(f64),
    AtLeast(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotApplicable {
    /// No open leaf task to count.
    NoTasks,
    NoLinks,
    NoBaseline,
    NoStatusDate,
    NoResources,
    /// No task has a baseline finish on or before the status date.
    NothingDue,
    /// No open critical task to delay (criterion 12).
    NoCriticalTask,
    /// The last milestone has no deadline, FNLT or MFO date (criterion 13).
    NoTarget,
    /// The critical path has no length left (criterion 13).
    NoPathLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthTask {
    pub uid: u32,
    pub reason: HealthReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthReason {
    NoPredecessor,
    NoSuccessor,
    NoLinks,
    /// The successor of a link with a negative lag.
    Lead,
    /// The successor of a link with a positive lag.
    Lag,
    /// The successor of a link other than finish-to-start.
    NotFinishToStart,
    HardConstraint,
    Manual,
    HighSlack,
    NegativeSlack,
    /// Finishes after its deadline (deadlines do not make slack negative in Sheafly).
    PastDeadline,
    LongDuration,
    ActualAfterStatus,
    /// Work not done yet is planned before the status date.
    PlannedBeforeStatus,
    NoResource,
    /// Due by baseline 0 on or before the status date and not finished.
    NotFinished,
    /// Finished after its baseline 0 finish.
    FinishedLate,
    /// The task criterion 12 delayed.
    Tested,
}
