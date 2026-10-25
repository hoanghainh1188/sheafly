//! The reads behind the AI model's tools (phase 22, PR C, decision D4): one task in detail,
//! one person's load by week, and the chain of tasks driving a task. They write what they
//! find the way the project summary does (`summary.rs`): tasks and people by UID, dates as
//! `YYYY-MM-DD HH:MM`, hours of work, names masked with `mask`. Reads, never logged.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::error::{EngineError, Invalid};
use crate::load_map::{load_map, Bucket, LoadGroup};
use crate::model::{ConstraintType, DepKind, Project, Task, TaskType};
use crate::summary::{
    constraint_text, hours, is_open, link_text, resource_name, resource_ref, task_line, task_ref,
    when, Lookup, TaskLine,
};
use crate::timeutil::{date_key, parse_date_key};

/// The longest span `resource_load` reads, in weeks.
pub const RESOURCE_LOAD_WEEKS: i64 = 104;
/// The most tasks a driving chain lists.
pub const CHAIN_LIMIT: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskDetail {
    /// The task's line of the summary.
    #[serde(flatten)]
    pub line: TaskLine,
    /// Successors: `T43`, or `T43 SS+2h` for another kind or a lag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub successors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignments: Vec<AssignmentLine>,
    /// The tasks right under a group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    pub free_slack_h: f64,
    pub early_start: String,
    pub early_finish: String,
    pub late_start: String,
    pub late_finish: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_finish: Option<String>,
    /// Baseline 0, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_finish: Option<String>,
    /// `fixed_units`, `fixed_duration` or `fixed_work`.
    pub task_type: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual: bool,
    #[serde(default, skip_serializing_if = "is_zero_hours")]
    pub leveling_delay_h: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentLine {
    pub resource: String,
    pub name: String,
    pub units: f64,
    pub work_h: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceLoad {
    pub id: String,
    pub name: String,
    pub max_units: f64,
    pub weeks: Vec<LoadWeek>,
    /// The person's tasks that run in the span, by start.
    pub tasks: Vec<LoadTask>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadWeek {
    /// The Monday, `YYYY-MM-DD`.
    pub week: String,
    pub work_h: f64,
    pub capacity_h: f64,
    /// Work as a share of capacity; absent when there is no capacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadTask {
    pub id: String,
    pub name: String,
    pub start: String,
    pub finish: String,
    pub units: f64,
    pub work_h: f64,
}

/// One task of a driving chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainStep {
    pub id: String,
    pub name: String,
    pub start: String,
    pub finish: String,
    pub slack_h: f64,
    pub free_slack_h: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub critical: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    /// How the step before drives this one: `T41`, `T41 SS+2h`; absent on the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

fn is_zero_hours(value: &f64) -> bool {
    *value == 0.0
}

fn task_name(task: &Task, mask: bool) -> String {
    if mask {
        format!("Task {}", task.uid)
    } else {
        task.name.clone()
    }
}

fn task_type_text(kind: TaskType) -> &'static str {
    match kind {
        TaskType::FixedUnits => "fixed_units",
        TaskType::FixedDuration => "fixed_duration",
        TaskType::FixedWork => "fixed_work",
    }
}

/// One task in detail; `None` when the project has no task `uid`.
pub fn task_detail(project: &Project, uid: u32, mask: bool) -> Option<TaskDetail> {
    let task = project.tasks.iter().find(|task| task.uid == uid)?;
    let lookup = Lookup::without_weeks(project);
    let successors = project
        .dependencies
        .iter()
        .filter(|link| link.predecessor_uid == uid)
        .map(|link| link_text(link.successor_uid, link.kind, link.lag))
        .collect();
    let assignments = project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == uid)
        .filter_map(|assignment| {
            let resource = project
                .resources
                .iter()
                .find(|resource| resource.uid == assignment.resource_uid)?;
            Some(AssignmentLine {
                resource: resource_ref(resource.uid),
                name: resource_name(resource, mask),
                units: assignment.units,
                work_h: hours(assignment.work),
            })
        })
        .collect();
    let children = project
        .tasks
        .iter()
        .filter(|child| child.parent_uid == Some(uid))
        .map(|child| task_ref(child.uid))
        .collect();
    let baseline = task.baselines.iter().find(|baseline| baseline.number == 0);
    Some(TaskDetail {
        line: task_line(project, &lookup, task, mask),
        successors,
        assignments,
        children,
        free_slack_h: hours(task.free_slack),
        early_start: when(task.early_start),
        early_finish: when(task.early_finish),
        late_start: when(task.late_start),
        late_finish: when(task.late_finish),
        actual_start: task.actual_start.map(when),
        actual_finish: task.actual_finish.map(when),
        baseline_start: baseline.map(|baseline| when(baseline.start)),
        baseline_finish: baseline.map(|baseline| when(baseline.finish)),
        task_type: task_type_text(task.task_type).into(),
        manual: task.manual,
        leveling_delay_h: hours(task.leveling_delay),
    })
}

/// One person's work and capacity by week (Monday to Sunday) from `from` to `to`, and the
/// tasks they work on then; `Ok(None)` when the project has no resource `uid`.
pub fn resource_load(
    project: &Project,
    uid: u32,
    from: NaiveDate,
    to: NaiveDate,
    mask: bool,
) -> Result<Option<ResourceLoad>, EngineError> {
    let Some(resource) = project
        .resources
        .iter()
        .find(|resource| resource.uid == uid)
    else {
        return Ok(None);
    };
    if to < from || (to - from).num_days() >= RESOURCE_LOAD_WEEKS * 7 {
        return Err(Invalid::BadDate.into());
    }
    let map = load_map(project, from, to, Bucket::Week, LoadGroup::Person, None)?;
    let weeks = map
        .rows
        .iter()
        .find(|row| row.resource_uid == Some(uid))
        .map(|row| {
            map.buckets
                .iter()
                .zip(&row.cells)
                .map(|(week, cell)| LoadWeek {
                    week: week.clone(),
                    work_h: hours(cell.work),
                    capacity_h: hours(cell.capacity),
                    percent: (cell.capacity > 0).then(|| {
                        (cell.work as f64 * 100.0 / cell.capacity as f64)
                            .round()
                            .min(9_999.0) as u32
                    }),
                })
                .collect()
        })
        .unwrap_or_default();
    let (first, last) = (date_key(from), date_key(to));
    let tasks_by_uid: HashMap<u32, &Task> =
        project.tasks.iter().map(|task| (task.uid, task)).collect();
    let mut tasks: Vec<(&Task, f64, i64)> = project
        .assignments
        .iter()
        .filter(|assignment| assignment.resource_uid == uid)
        .filter_map(|assignment| {
            let task = tasks_by_uid.get(&assignment.task_uid)?;
            let runs = date_key_of(task.start) <= last && date_key_of(task.finish) >= first;
            runs.then_some((*task, assignment.units, assignment.work))
        })
        .collect();
    tasks.sort_by_key(|(task, _, _)| (task.start, task.uid));
    Ok(Some(ResourceLoad {
        id: resource_ref(uid),
        name: resource_name(resource, mask),
        max_units: resource.max_units,
        weeks,
        tasks: tasks
            .into_iter()
            .map(|(task, units, work)| LoadTask {
                id: task_ref(task.uid),
                name: task_name(task, mask),
                start: when(task.start),
                finish: when(task.finish),
                units,
                work_h: hours(work),
            })
            .collect(),
    }))
}

/// `YYYY-MM-DD` of an instant, comparable as text with `date_key`.
fn date_key_of(ms: i64) -> String {
    when(ms)[..10].to_string()
}

/// The chain of tasks driving `uid`'s start, earliest first and ending with the task; `None`
/// when the project has no task `uid`. From the task the chain steps back to the
/// predecessor that drives it, the one whose linked date comes last (links of the groups
/// above count too, and a group predecessor stands for its leaf that finishes last). It
/// stops at a task with no predecessor, one already started, or one a constraint date pins.
pub fn critical_chain(project: &Project, uid: u32, mask: bool) -> Option<Vec<ChainStep>> {
    let index: HashMap<u32, usize> = project
        .tasks
        .iter()
        .enumerate()
        .map(|(at, task)| (task.uid, at))
        .collect();
    let first = project.tasks[*index.get(&uid)?].uid;
    let start = last_leaf(project, &index, first);
    let mut steps: Vec<(u32, Option<String>)> = vec![(start, None)];
    let mut seen: HashSet<u32> = HashSet::from([start]);
    while steps.len() < CHAIN_LIMIT {
        let current = &project.tasks[index[&steps.last().map_or(start, |step| step.0)]];
        if current.actual_start.is_some() {
            break;
        }
        let Some((driver, via)) = driver_of(project, &index, current) else {
            break;
        };
        if !seen.insert(driver) {
            break;
        }
        if let Some(step) = steps.last_mut() {
            step.1 = Some(via);
        }
        steps.push((driver, None));
    }
    // Driver first: each step's `after` is the link from the step before it.
    steps.reverse();
    Some(
        steps
            .into_iter()
            .map(|(uid, after)| {
                let task = &project.tasks[index[&uid]];
                ChainStep {
                    id: task_ref(uid),
                    name: task_name(task, mask),
                    start: when(task.start),
                    finish: when(task.finish),
                    slack_h: hours(task.total_slack),
                    free_slack_h: hours(task.free_slack),
                    critical: task.critical && is_open(task),
                    done: !is_open(task),
                    constraint: constraint_text(task),
                    after,
                }
            })
            .collect(),
    )
}

/// A group's leaf that finishes last (the group's finish), or the task itself.
fn last_leaf(project: &Project, index: &HashMap<u32, usize>, uid: u32) -> u32 {
    let at = index[&uid];
    let task = &project.tasks[at];
    if !task.summary {
        return uid;
    }
    project.tasks[at + 1..]
        .iter()
        .take_while(|below| below.outline_level > task.outline_level)
        .filter(|below| !below.summary)
        .max_by_key(|below| (below.finish, std::cmp::Reverse(below.uid)))
        .map_or(uid, |leaf| leaf.uid)
}

/// The predecessor driving `task` and the link text, if any: the one whose linked date
/// (its finish for FS and FF, its start for SS and SF) comes last.
fn driver_of(project: &Project, index: &HashMap<u32, usize>, task: &Task) -> Option<(u32, String)> {
    if pinned(task) {
        return None;
    }
    let mut targets = vec![task.uid];
    let mut parent = task.parent_uid;
    while let Some(uid) = parent {
        targets.push(uid);
        parent = index.get(&uid).and_then(|at| project.tasks[*at].parent_uid);
    }
    project
        .dependencies
        .iter()
        .filter(|link| targets.contains(&link.successor_uid))
        .filter_map(|link| {
            index.get(&link.predecessor_uid)?;
            let leaf = last_leaf(project, index, link.predecessor_uid);
            let predecessor = &project.tasks[index[&leaf]];
            let linked = match link.kind {
                DepKind::Fs | DepKind::Ff => predecessor.finish,
                DepKind::Ss | DepKind::Sf => predecessor.start,
            };
            (predecessor.uid != task.uid).then(|| {
                let via = link_text(link.predecessor_uid, link.kind, link.lag);
                (predecessor, linked, via)
            })
        })
        .max_by_key(|(predecessor, linked, _)| {
            (
                *linked,
                std::cmp::Reverse(predecessor.total_slack),
                std::cmp::Reverse(predecessor.uid),
            )
        })
        .map(|(predecessor, _, via)| (predecessor.uid, via))
}

/// A task whose start a constraint date sets, not its links.
fn pinned(task: &Task) -> bool {
    let date = |at: i64| task.constraint_date == Some(at);
    match task.constraint_type {
        ConstraintType::Mso | ConstraintType::Snet | ConstraintType::Snlt => date(task.start),
        ConstraintType::Mfo | ConstraintType::Fnet | ConstraintType::Fnlt => date(task.finish),
        ConstraintType::Asap | ConstraintType::Alap => false,
    }
}

/// Read a `YYYY-MM-DD` date given to a tool.
pub fn tool_date(text: &str) -> Option<NaiveDate> {
    parse_date_key(text.trim())
}
