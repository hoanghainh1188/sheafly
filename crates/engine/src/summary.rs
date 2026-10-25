//! The project summary sent to an AI model (phase 22, PR A, #100): a compact read of the
//! scheduled project for recommendations. A read, never logged.
//!
//! Tasks and people are always named by UID (`T42`, `R7`) so the commands a model answers
//! with refer to them the same way and nothing is mapped back (decision D3). With `mask` on,
//! the names of the project, tasks and people become `Project`, `Task 42`, `Person 7`, and
//! custom field names and their text values become `Field 3`, `Option 2`, `Value 1`; skills,
//! levels and roles stay, since they name kinds of work rather than people or clients.
//! Notes and custom field values of tasks are never included.
//!
//! Dates are project wall-clock time as `YYYY-MM-DD HH:MM`; durations, work and slack are
//! working hours. The engine computes every number here; the model only reads them.
//! A summary longer than `max_chars` (its JSON) keeps the tasks that matter most and says
//! how many it left out (`summary_fit.rs`, decision D7).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::focus::focus_of;
use crate::model::{ConstraintType, DepKind, Project, Resource, ResourceKind, Task};
use crate::skills::{rule_for, RuleTarget};
use crate::statistics::statistics;
use crate::summary_fit::fit;
use crate::summary_load::{overloaded_weeks, WeekTable};
use crate::timeutil::{format_iso, MINUTES_PER_DAY};

/// About 15,000 tokens of JSON: room for a few hundred tasks and the reply.
pub const DEFAULT_MAX_CHARS: usize = 60_000;
/// Critical tasks listed in `critical_path`; the header counts them all.
pub const CRITICAL_PATH_LIMIT: usize = 100;
/// Overloaded weeks listed per person, earliest first.
pub const OVERLOADED_WEEKS_LIMIT: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryOptions {
    /// Replace names of the project, tasks, people, fields and text values (D3).
    pub mask: bool,
    /// The most characters the summary's JSON may take (D7).
    pub max_chars: usize,
}

impl Default for SummaryOptions {
    fn default() -> Self {
        Self {
            mask: false,
            max_chars: DEFAULT_MAX_CHARS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project: SummaryHeader,
    pub slack: SlackSpread,
    /// Open critical leaf tasks by start, at most `CRITICAL_PATH_LIMIT`.
    pub critical_path: Vec<String>,
    /// Tasks in outline order; when cut, the ones that matter most and their groups.
    pub tasks: Vec<TaskLine>,
    pub resources: Vec<ResourceLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<SkillLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<RuleLine>,
    /// Some tasks or people were left out to stay within `max_chars`.
    pub truncated: bool,
    pub omitted_tasks: usize,
    pub omitted_resources: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummaryHeader {
    pub name: String,
    pub start: String,
    pub finish: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_date: Option<String>,
    pub percent_complete: u32,
    pub work_h: f64,
    /// Tasks that are not groups, milestones included.
    pub leaves: usize,
    pub milestones: usize,
    pub done: usize,
    /// Open leaves only, like the critical path and the order of a cut summary.
    pub critical: usize,
    pub late: usize,
    pub deadlines_missed: usize,
    /// Open leaves with work to do and nobody on them.
    pub unassigned: usize,
    pub overallocated_people: usize,
}

/// Open leaf tasks that are not milestones, by total slack.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackSpread {
    pub zero: usize,
    pub under_day: usize,
    /// One to five working days.
    pub to_week: usize,
    /// Five to twenty working days.
    pub to_month: usize,
    pub over_month: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskLine {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub summary: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub milestone: bool,
    pub start: String,
    pub finish: String,
    pub duration_h: f64,
    pub work_h: f64,
    pub slack_h: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub critical: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub late: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub percent: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<String>,
    /// Predecessors: `T41`, or `T41 SS+2h` for another kind or a lag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// `SNET 2026-01-06 08:00`, `ALAP`; absent for as soon as possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interrupts: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceLine {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub machine: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub max_units: f64,
    /// The productivity factor when it is not 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<f64>,
    /// `Backend` or `Backend:Senior`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    pub work_h: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub overallocated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overloaded_weeks: Vec<WeekLoad>,
}

/// A week (Monday to Sunday) with at least one overallocated day.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WeekLoad {
    /// The Monday, `YYYY-MM-DD`.
    pub week: String,
    pub work_h: f64,
    pub capacity_h: f64,
    /// The highest share of the person's units used on one day of the week.
    pub peak_percent: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillLine {
    pub name: String,
    pub people: usize,
    /// Unassigned leaves not started whose first matching rule asks for the skill.
    pub unassigned_tasks: usize,
    pub unassigned_work_h: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleLine {
    /// Conditions, all must hold: `Mảng = Thanh toán | Báo cáo`; empty matches every task.
    pub when: Vec<String>,
    /// `R7` or `skill Backend ≥ Senior`.
    pub to: String,
    pub units: f64,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

pub fn task_ref(uid: u32) -> String {
    format!("T{uid}")
}

pub fn resource_ref(uid: u32) -> String {
    format!("R{uid}")
}

/// The summary of a scheduled project.
pub fn project_summary(project: &Project, options: &SummaryOptions) -> ProjectSummary {
    let lookup = Lookup::new(project);
    let tasks: Vec<TaskLine> = project
        .tasks
        .iter()
        .map(|task| task_line(project, &lookup, task, options.mask))
        .collect();
    let resources: Vec<ResourceLine> = project
        .resources
        .iter()
        .map(|resource| resource_line(project, &lookup, resource, options.mask))
        .collect();
    let summary = ProjectSummary {
        project: header(project, &lookup, options.mask),
        slack: slack_spread(project),
        critical_path: critical_path(project),
        tasks: Vec::new(),
        resources: Vec::new(),
        skills: skill_lines(project, &lookup),
        rules: crate::summary_rules::rule_lines(project, options.mask),
        truncated: false,
        omitted_tasks: 0,
        omitted_resources: 0,
    };
    fit(
        project,
        &lookup,
        summary,
        tasks,
        resources,
        options.max_chars,
    )
}

/// Indexes built once per summary.
pub(crate) struct Lookup {
    /// Resource UIDs on each task, in assignment order.
    pub(crate) people: HashMap<u32, Vec<u32>>,
    /// Assigned work of each resource.
    pub(crate) work: HashMap<u32, i64>,
    /// Overallocated days of each resource: date key → units used ÷ units had.
    pub(crate) overloaded: BTreeMap<u32, BTreeMap<String, f64>>,
    pub(crate) index: HashMap<u32, usize>,
    /// Positions in `Project::dependencies` of each task's predecessor links.
    pub(crate) links: HashMap<u32, Vec<usize>>,
    /// Work and capacity of the overloaded people in the weeks they are overloaded.
    pub(crate) weeks: WeekTable,
}

impl Lookup {
    fn new(project: &Project) -> Self {
        let mut people: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut work: HashMap<u32, i64> = HashMap::new();
        for assignment in &project.assignments {
            people
                .entry(assignment.task_uid)
                .or_default()
                .push(assignment.resource_uid);
            *work.entry(assignment.resource_uid).or_default() += assignment.work;
        }
        let mut overloaded: BTreeMap<u32, BTreeMap<String, f64>> = BTreeMap::new();
        for day in &project.overallocations {
            let share = if day.max_units > 0.0 {
                day.units / day.max_units
            } else {
                f64::INFINITY
            };
            let days = overloaded.entry(day.resource_uid).or_default();
            let entry = days.entry(day.date.clone()).or_insert(0.0);
            *entry = entry.max(share);
        }
        let index = project
            .tasks
            .iter()
            .enumerate()
            .map(|(at, task)| (task.uid, at))
            .collect();
        let mut links: HashMap<u32, Vec<usize>> = HashMap::new();
        for (at, dependency) in project.dependencies.iter().enumerate() {
            links.entry(dependency.successor_uid).or_default().push(at);
        }
        let weeks = WeekTable::new(project, &overloaded);
        Self {
            people,
            work,
            overloaded,
            index,
            links,
            weeks,
        }
    }

    pub(crate) fn overallocated(&self, resource_uid: u32) -> bool {
        self.overloaded.contains_key(&resource_uid)
    }
}

/// Working minutes as hours to a tenth.
pub(crate) fn hours(minutes: i64) -> f64 {
    (minutes as f64 / 6.0).round() / 10.0
}

/// `2026-01-05 08:00`.
pub(crate) fn when(ms: i64) -> String {
    format_iso(ms)[..16].replacen('T', " ", 1)
}

pub(crate) fn is_open(task: &Task) -> bool {
    task.percent_complete < 100.0
}

fn header(project: &Project, lookup: &Lookup, mask: bool) -> SummaryHeader {
    let stats = statistics(project);
    let leaves: Vec<&Task> = project.tasks.iter().filter(|task| !task.summary).collect();
    let count = |keep: &dyn Fn(&Task) -> bool| leaves.iter().filter(|task| keep(task)).count();
    SummaryHeader {
        name: if mask {
            "Project".into()
        } else {
            project.name.clone()
        },
        start: when(stats.start),
        finish: when(stats.finish),
        status_date: project.status_date.map(when),
        percent_complete: stats.percent_complete.round().clamp(0.0, 100.0) as u32,
        work_h: hours(stats.work),
        leaves: leaves.len(),
        milestones: count(&|task| task.milestone),
        done: count(&|task| !is_open(task)),
        critical: count(&|task| task.critical && is_open(task)),
        late: count(&|task| task.late && is_open(task)),
        deadlines_missed: count(&|task| {
            is_open(task) && task.deadline.is_some_and(|deadline| task.finish > deadline)
        }),
        unassigned: count(&|task| needs_people(lookup, task)),
        overallocated_people: lookup.overloaded.len(),
    }
}

/// An open leaf with work to do and nobody on it.
fn needs_people(lookup: &Lookup, task: &Task) -> bool {
    !task.summary && !task.milestone && is_open(task) && !lookup.people.contains_key(&task.uid)
}

fn slack_spread(project: &Project) -> SlackSpread {
    let mut spread = SlackSpread::default();
    let open = project
        .tasks
        .iter()
        .filter(|task| !task.summary && !task.milestone && is_open(task));
    for task in open {
        let slack = task.total_slack;
        match slack {
            _ if slack <= 0 => spread.zero += 1,
            _ if slack < MINUTES_PER_DAY => spread.under_day += 1,
            _ if slack <= 5 * MINUTES_PER_DAY => spread.to_week += 1,
            _ if slack <= 20 * MINUTES_PER_DAY => spread.to_month += 1,
            _ => spread.over_month += 1,
        }
    }
    spread
}

fn critical_path(project: &Project) -> Vec<String> {
    let mut critical: Vec<&Task> = project
        .tasks
        .iter()
        .filter(|task| !task.summary && task.critical && is_open(task))
        .collect();
    critical.sort_by_key(|task| (task.start, task.finish, task.uid));
    critical
        .iter()
        .take(CRITICAL_PATH_LIMIT)
        .map(|task| task_ref(task.uid))
        .collect()
}

fn task_line(project: &Project, lookup: &Lookup, task: &Task, mask: bool) -> TaskLine {
    let after = lookup
        .links
        .get(&task.uid)
        .map(|links| {
            links
                .iter()
                .map(|link| {
                    link_text(
                        project.dependencies[*link].predecessor_uid,
                        project.dependencies[*link].kind,
                        project.dependencies[*link].lag,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let people = lookup
        .people
        .get(&task.uid)
        .map(|uids| uids.iter().map(|uid| resource_ref(*uid)).collect())
        .unwrap_or_default();
    TaskLine {
        id: task_ref(task.uid),
        name: if mask {
            format!("Task {}", task.uid)
        } else {
            task.name.clone()
        },
        parent: task.parent_uid.map(task_ref),
        summary: task.summary,
        milestone: task.milestone,
        start: when(task.start),
        finish: when(task.finish),
        duration_h: hours(task.duration),
        work_h: hours(task.work),
        slack_h: hours(task.total_slack),
        critical: task.critical && is_open(task),
        late: task.late,
        deadline: task.deadline.map(when),
        percent: task.percent_complete.round().clamp(0.0, 100.0) as u32,
        people,
        after,
        constraint: constraint_text(task),
        interrupts: task.interrupts,
    }
}

fn link_text(predecessor_uid: u32, kind: DepKind, lag: i64) -> String {
    let id = task_ref(predecessor_uid);
    if kind == DepKind::Fs && lag == 0 {
        return id;
    }
    let kind = match kind {
        DepKind::Fs => "FS",
        DepKind::Ss => "SS",
        DepKind::Ff => "FF",
        DepKind::Sf => "SF",
    };
    match lag {
        0 => format!("{id} {kind}"),
        _ => format!(
            "{id} {kind}{}{}h",
            if lag > 0 { "+" } else { "-" },
            hours(lag.abs())
        ),
    }
}

fn constraint_text(task: &Task) -> Option<String> {
    let kind = match task.constraint_type {
        ConstraintType::Asap => return None,
        ConstraintType::Alap => return Some("ALAP".into()),
        ConstraintType::Snet => "SNET",
        ConstraintType::Snlt => "SNLT",
        ConstraintType::Fnet => "FNET",
        ConstraintType::Fnlt => "FNLT",
        ConstraintType::Mso => "MSO",
        ConstraintType::Mfo => "MFO",
    };
    Some(match task.constraint_date {
        Some(date) => format!("{kind} {}", when(date)),
        None => kind.into(),
    })
}

fn resource_line(
    project: &Project,
    lookup: &Lookup,
    resource: &Resource,
    mask: bool,
) -> ResourceLine {
    let machine = resource.kind == ResourceKind::Machine;
    let name = match (mask, machine) {
        (false, _) => resource.name.clone(),
        (true, false) => format!("Person {}", resource.uid),
        (true, true) => format!("Machine {}", resource.uid),
    };
    let focus = focus_of(project, resource);
    ResourceLine {
        id: resource_ref(resource.uid),
        name,
        machine,
        role: Some(resource.group.clone()).filter(|group| !group.is_empty()),
        max_units: resource.max_units,
        focus: Some(focus).filter(|focus| (focus - 1.0).abs() > f64::EPSILON),
        skills: skill_names(project, resource),
        work_h: hours(lookup.work.get(&resource.uid).copied().unwrap_or(0)),
        overallocated: lookup.overallocated(resource.uid),
        overloaded_weeks: overloaded_weeks(lookup, resource.uid),
    }
}

fn skill_names(project: &Project, resource: &Resource) -> Vec<String> {
    resource
        .skills
        .iter()
        .filter_map(|id| project.skills.iter().find(|skill| skill.id == *id))
        .map(|skill| {
            let level = resource
                .skill_levels
                .get(&skill.id)
                .and_then(|level| project.skill_levels.iter().find(|known| known.id == *level));
            match level {
                Some(level) => format!("{}:{}", skill.name, level.name),
                None => skill.name.clone(),
            }
        })
        .collect()
}

fn skill_lines(project: &Project, lookup: &Lookup) -> Vec<SkillLine> {
    let mut demand: HashMap<u32, (usize, i64)> = HashMap::new();
    let waiting = project
        .tasks
        .iter()
        .filter(|task| needs_people(lookup, task) && task.actual_start.is_none());
    for task in waiting {
        let target = rule_for(project, task).map(|at| project.assignment_rules[at].target);
        if let Some(RuleTarget::Skill { id, .. }) = target {
            let entry = demand.entry(id).or_default();
            entry.0 += 1;
            entry.1 += unassigned_work(task);
        }
    }
    let holders: HashSet<(u32, u32)> = project
        .resources
        .iter()
        .flat_map(|resource| {
            resource
                .skills
                .iter()
                .map(move |skill| (*skill, resource.uid))
        })
        .collect();
    project
        .skills
        .iter()
        .map(|skill| {
            let (tasks, work) = demand.get(&skill.id).copied().unwrap_or_default();
            SkillLine {
                name: skill.name.clone(),
                people: holders.iter().filter(|(id, _)| *id == skill.id).count(),
                unassigned_tasks: tasks,
                unassigned_work_h: hours(work),
            }
        })
        .collect()
}

/// The effort an unassigned task waits with: its work, or its duration full time.
fn unassigned_work(task: &Task) -> i64 {
    if task.work > 0 {
        task.work
    } else {
        task.duration
    }
}
