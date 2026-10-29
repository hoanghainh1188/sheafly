//! The types of a project comparison and variance report (sheafly-pro#4).
//!
//! A pure comparison over two scheduled `Project` structures (`baseline` and `current`).
//! The diff engine itself is implemented in the private Pro crate (`sheafly_pro::project_diff`);
//! these public types are defined here so the stub in `sheafly/crates/pro`, the private Pro crate,
//! the Tauri IPC shell, and the frontend types share the exact same definition (identical to
//! `health.rs` and `cash_flow.rs`).

use serde::{Deserialize, Serialize};

/// High-level status of a task when comparing two project states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskDiffKind {
    /// Task exists in the current project but was not present in the baseline.
    Added,
    /// Task was present in the baseline but was removed in the current project.
    Removed,
    /// Task is present in both projects but has modified dates, duration, work, cost, links, or assignments.
    Modified,
    /// Task is identical across both projects in all major schedule attributes.
    Unchanged,
}

/// Changes in dependency links connected to a task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkDiff {
    pub other_task_uid: u32,
    pub other_task_name: String,
    pub is_predecessor: bool,
    /// "Added", "Removed", or "Modified"
    pub change: String,
    pub old_detail: Option<String>,
    pub new_detail: Option<String>,
}

/// Changes in resource assignments on a task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentDiff {
    pub resource_uid: u32,
    pub resource_name: String,
    /// "Added", "Removed", or "UnitsChanged"
    pub change: String,
    pub old_units: Option<f64>,
    pub new_units: Option<f64>,
}

/// Itemized variance analysis for a single task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskDiff {
    pub uid: u32,
    pub id: u32,
    pub wbs: String,
    pub name: String,
    pub kind: TaskDiffKind,

    // Dates (unix timestamps in ms or seconds as per model; model uses i64 timestamps)
    pub old_start: Option<i64>,
    pub new_start: Option<i64>,
    pub old_finish: Option<i64>,
    pub new_finish: Option<i64>,
    /// Shift in finish date (in working days or calendar days; positive = slipped late).
    pub finish_slip_days: f64,

    // Duration & Work (minutes)
    pub old_duration: Option<i64>,
    pub new_duration: Option<i64>,
    pub duration_delta: i64,
    pub old_work: Option<i64>,
    pub new_work: Option<i64>,
    pub work_delta: i64,

    // Costs
    pub old_cost: Option<f64>,
    pub new_cost: Option<f64>,
    pub cost_delta: f64,

    // Changes in relationships & staffing
    pub link_diffs: Vec<LinkDiff>,
    pub assignment_diffs: Vec<AssignmentDiff>,
}

/// Aggregate summary counts of project variance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub total_current_tasks: usize,
    pub total_baseline_tasks: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub modified_count: usize,
    pub unchanged_count: usize,
    /// Tasks whose finish date moved later than baseline.
    pub slipped_count: usize,
}

/// The complete report comparing two projects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectDiffReport {
    pub base_name: String,
    pub current_name: String,
    pub base_finish: Option<i64>,
    pub current_finish: Option<i64>,
    pub finish_delta_days: f64,
    pub cost_delta: f64,
    pub work_delta_hours: f64,
    pub summary: DiffSummary,
    pub tasks: Vec<TaskDiff>,
}
