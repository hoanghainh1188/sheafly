use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::availability::AvailabilityPeriod;
use crate::calendar::Calendar;
use crate::fields::{FieldDef, FieldValue};
use crate::level_order::SortKey;
use crate::recurrence::Recurrence;
use crate::skill_levels::SkillLevel;
use crate::skills::{AssignmentRule, SkillDef};
use crate::source::{ExternalLink, Source};
use crate::split::{Segment, Split};
use crate::timeutil::MINUTES_PER_DAY;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    #[default]
    Asap,
    Alap,
    Snet,
    Snlt,
    Fnet,
    Fnlt,
    Mso,
    Mfo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepKind {
    #[default]
    Fs,
    Ss,
    Ff,
    Sf,
}

impl DepKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_uppercase().as_str() {
            "FS" => Some(Self::Fs),
            "SS" => Some(Self::Ss),
            "FF" => Some(Self::Ff),
            "SF" => Some(Self::Sf),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fs => "FS",
            Self::Ss => "SS",
            Self::Ff => "FF",
            Self::Sf => "SF",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    #[default]
    FixedUnits,
    FixedDuration,
    FixedWork,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accrual {
    Start,
    #[default]
    Prorated,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Person,
    Machine,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub number: u8,
    pub start: i64,
    pub finish: i64,
    pub work: i64,
    pub cost: f64,
    /// Segments of a split task when the baseline was set; empty when it was not split.
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// The fixed-cost part of `cost` when the baseline was set; `None` in baselines saved
    /// before it was kept (engine revision 9) and from MSPDI, where readers take the task's
    /// current fixed cost, at most `cost` (`Baseline::fixed_part`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_cost: Option<f64>,
}

impl Baseline {
    /// The fixed-cost part of the baseline cost, which follows the task's accrual; the rest
    /// is labour, spent in step with the work.
    pub fn fixed_part(&self, task: &Task) -> f64 {
        let cost = self.cost.max(0.0);
        self.fixed_cost.unwrap_or(task.fixed_cost).clamp(0.0, cost)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentBaseline {
    pub number: u8,
    pub work: i64,
    /// The assignment's cost when the baseline was set; 0 in baselines saved before it was
    /// kept (engine revision 9), where readers fall back to the work at today's rate.
    #[serde(default)]
    pub cost: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub uid: u32,
    pub id: u32,
    pub name: String,
    pub wbs: String,
    pub outline_level: u32,
    pub parent_uid: Option<u32>,
    pub notes: String,
    pub priority: i32,
    pub milestone: bool,
    pub summary: bool,
    pub manual: bool,
    pub duration: i64,
    pub constraint_type: ConstraintType,
    pub constraint_date: Option<i64>,
    pub calendar_id: Option<u32>,
    pub task_type: TaskType,
    pub effort_driven: bool,
    pub percent_complete: f64,
    pub actual_start: Option<i64>,
    pub actual_finish: Option<i64>,
    pub remaining_duration: i64,
    pub remaining_work: i64,
    pub fixed_cost: f64,
    pub fixed_cost_accrual: Accrual,
    pub leveling_delay: i64,
    pub start: i64,
    pub finish: i64,
    pub early_start: i64,
    pub early_finish: i64,
    pub late_start: i64,
    pub late_finish: i64,
    pub total_slack: i64,
    pub free_slack: i64,
    pub critical: bool,
    pub units: f64,
    pub work: i64,
    pub actual_work: i64,
    pub cost: f64,
    pub actual_cost: f64,
    pub remaining_cost: f64,
    pub conflict: bool,
    pub predecessors: String,
    pub baselines: Vec<Baseline>,
    /// Optional deadline instant. Not a constraint: scheduling never moves the task to meet it.
    #[serde(default)]
    pub deadline: Option<i64>,
    /// Actual cost entered by hand. Used only while `Project::editable_actual_costs` is on;
    /// otherwise the engine computes the actual cost from progress.
    #[serde(default)]
    pub manual_actual_cost: Option<f64>,
    /// Finish is past the status date (while incomplete), baseline 0, or the deadline.
    #[serde(default)]
    pub late: bool,
    /// Series definition; set only on the summary task of a recurring series.
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
    /// Summary UID of the series this occurrence belongs to.
    #[serde(default)]
    pub series_uid: Option<u32>,
    /// Local midnight of the date the pattern produced for this occurrence.
    #[serde(default)]
    pub occurrence_date: Option<i64>,
    /// Edited on its own; regenerating the series leaves it unchanged.
    #[serde(default)]
    pub detached: bool,
    /// Values of the project's custom fields, by field id; a field with no value is absent.
    #[serde(default)]
    pub values: BTreeMap<u32, FieldValue>,
    /// Interruptions of the work, ordered by offset. Empty when the task is not split.
    #[serde(default)]
    pub splits: Vec<Split>,
    /// Scheduled pieces of work when split; derived by the scheduler, empty otherwise.
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// Urgent: when leveling, it goes ahead of work in progress ranked below it on the same
    /// people, which is split to let it in (`level_interrupt.rs`).
    #[serde(default)]
    pub interrupts: bool,
    /// The least and most effort the task may take, in working minutes, for the finish
    /// forecast (`SetWorkRange`); both or neither. Scheduling never reads them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_low: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_high: Option<i64>,
    /// The issue in an external tracker this task was pulled from (`source.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalLink>,
}

impl Task {
    pub fn new(uid: u32, name: impl Into<String>) -> Self {
        Self {
            uid,
            id: 0,
            name: name.into(),
            wbs: String::new(),
            outline_level: 1,
            parent_uid: None,
            notes: String::new(),
            priority: 500,
            milestone: false,
            summary: false,
            manual: false,
            duration: MINUTES_PER_DAY,
            constraint_type: ConstraintType::Asap,
            constraint_date: None,
            calendar_id: None,
            task_type: TaskType::FixedUnits,
            effort_driven: true,
            percent_complete: 0.0,
            actual_start: None,
            actual_finish: None,
            remaining_duration: MINUTES_PER_DAY,
            remaining_work: MINUTES_PER_DAY,
            fixed_cost: 0.0,
            fixed_cost_accrual: Accrual::Prorated,
            leveling_delay: 0,
            start: 0,
            finish: 0,
            early_start: 0,
            early_finish: 0,
            late_start: 0,
            late_finish: 0,
            total_slack: 0,
            free_slack: 0,
            critical: false,
            units: 1.0,
            work: MINUTES_PER_DAY,
            actual_work: 0,
            cost: 0.0,
            actual_cost: 0.0,
            remaining_cost: 0.0,
            conflict: false,
            predecessors: String::new(),
            baselines: Vec::new(),
            deadline: None,
            manual_actual_cost: None,
            late: false,
            recurrence: None,
            series_uid: None,
            occurrence_date: None,
            detached: false,
            values: BTreeMap::new(),
            splits: Vec::new(),
            segments: Vec::new(),
            interrupts: false,
            work_low: None,
            work_high: None,
            external: None,
        }
    }

    /// A task is late when any one of these is true: less is done than the schedule says
    /// should be by the status date (see `tracking::behind_schedule`), the finish is
    /// after baseline 0, or the finish is after a deadline. Equality is on time.
    /// A summary is never behind on its own; `schedule::mark_late` takes that from its
    /// children.
    pub fn is_late(&self, status_date: Option<i64>, calendar: &Calendar) -> bool {
        let after_status = status_date
            .is_some_and(|status| crate::tracking::behind_schedule(self, calendar, status));
        after_status || self.misses_planned_dates()
    }

    /// The finish is after baseline 0 or after the deadline.
    pub fn misses_planned_dates(&self) -> bool {
        let after_baseline = self
            .baselines
            .iter()
            .any(|baseline| baseline.number == 0 && self.finish > baseline.finish);
        let after_deadline = self.deadline.is_some_and(|deadline| self.finish > deadline);
        after_baseline || after_deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_json_without_deadline_or_late_still_loads() {
        let task = Task::new(1, "A");
        let mut value = serde_json::to_value(&task).expect("task json");
        let object = value.as_object_mut().expect("object");
        object.remove("deadline");
        object.remove("late");
        let restored: Task = serde_json::from_value(value).expect("decode");
        assert_eq!(restored.deadline, None);
        assert!(!restored.late);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub uid: u32,
    pub predecessor_uid: u32,
    pub successor_uid: u32,
    pub kind: DepKind,
    pub lag: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub uid: u32,
    pub name: String,
    pub kind: ResourceKind,
    pub max_units: f64,
    pub calendar_id: Option<u32>,
    pub standard_rate: f64,
    pub overtime_rate: f64,
    pub cost_per_use: f64,
    /// Days with other units than `max_units` (`availability.rs`), in date order.
    #[serde(default)]
    pub availability: Vec<AvailabilityPeriod>,
    /// Ids of the project's skills this resource has, ascending.
    #[serde(default)]
    pub skills: Vec<u32>,
    /// Email address, empty when unknown; unique in the project, letter case aside. A task
    /// pulled from a tracker goes to the person whose email its assignee has.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// Role or team, a label (`skill_levels.rs`); empty when none. MSPDI `<Group>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// The level of each skill that has one: skill id → level id of `Project.skill_levels`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub skill_levels: BTreeMap<u32, u32>,
    /// Share of time at work this resource gives to project work (`focus.rs`); `None`
    /// follows the project's factor for people and 1.0 for machines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<f64>,
}

impl Resource {
    pub fn new(uid: u32, name: impl Into<String>, kind: ResourceKind) -> Self {
        Self {
            uid,
            name: name.into(),
            kind,
            max_units: 1.0,
            calendar_id: None,
            standard_rate: 0.0,
            overtime_rate: 0.0,
            cost_per_use: 0.0,
            availability: Vec::new(),
            skills: Vec::new(),
            email: String::new(),
            group: String::new(),
            skill_levels: BTreeMap::new(),
            focus: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub uid: u32,
    pub task_uid: u32,
    pub resource_uid: u32,
    pub units: f64,
    pub work: i64,
    pub overtime_work: i64,
    pub actual_work: i64,
    pub remaining_work: i64,
    pub cost: f64,
    pub actual_cost: f64,
    /// Actual cost entered by hand for this resource on this task. Used only while
    /// `Project::editable_actual_costs` is on and the task has no entry of its own.
    #[serde(default)]
    pub manual_actual_cost: Option<f64>,
    /// Saved baselines for this assignment: baseline number and work in working minutes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub baselines: Vec<AssignmentBaseline>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Overallocation {
    pub resource_uid: u32,
    pub date: String,
    pub units: f64,
    pub max_units: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageDay {
    pub date: String,
    pub hours: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageRow {
    pub task_uid: u32,
    pub assignment_uid: Option<u32>,
    pub resource_uid: Option<u32>,
    pub label: String,
    pub days: Vec<UsageDay>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub start: i64,
    pub status_date: Option<i64>,
    pub calendar_id: u32,
    pub calendars: Vec<Calendar>,
    pub tasks: Vec<Task>,
    pub dependencies: Vec<Dependency>,
    pub resources: Vec<Resource>,
    pub assignments: Vec<Assignment>,
    pub next_task_uid: u32,
    pub next_dependency_uid: u32,
    pub next_resource_uid: u32,
    pub next_assignment_uid: u32,
    pub next_calendar_uid: u32,
    pub overallocations: Vec<Overallocation>,
    pub usage: Vec<UsageRow>,
    /// Actual costs are entered by hand (MSPDI `EditableActualCosts`). Off by default:
    /// the engine computes them from progress.
    #[serde(default)]
    pub editable_actual_costs: bool,
    /// Custom task fields, in the order the user added them.
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    /// The id the next custom field gets. Ids are never reused, so an id held outside
    /// the project (a column, a group) cannot come to mean another field. 0 in files
    /// written before it existed: the next id is then one past the largest.
    #[serde(default)]
    pub next_field_id: u32,
    /// The order leveling places tasks in (`level_order.rs`); empty for the default.
    #[serde(default)]
    pub leveling_order: Vec<SortKey>,
    /// Skills resources can have (`skills.rs`), in the order the user added them.
    #[serde(default)]
    pub skills: Vec<SkillDef>,
    /// The id the next skill gets; ids are never reused, like field ids. 0 in files
    /// written before it existed: the next id is then one past the largest.
    #[serde(default)]
    pub next_skill_id: u32,
    /// Skill levels, lowest first (`skill_levels.rs`); empty when levels are not used.
    #[serde(default)]
    pub skill_levels: Vec<SkillLevel>,
    /// The id the next level gets; ids are never reused, like skill ids.
    #[serde(default)]
    pub next_skill_level_id: u32,
    /// Ordered rules from custom field values to a person or a skill; the first that
    /// matches a task is used. Only suggestions read them.
    #[serde(default)]
    pub assignment_rules: Vec<AssignmentRule>,
    /// How tasks' effort varies together in the finish forecast.
    #[serde(default)]
    pub forecast: ForecastSettings,
    /// Share of time at work people give to project work (`focus.rs`), 0.5 to 1.0; it
    /// slows the rate at which their work is done. 1.0 in files written before it existed.
    #[serde(default = "crate::focus::full_focus")]
    pub focus_factor: f64,
    /// The external task tracker this project is pulled from (`SetSource`). Written even
    /// when empty (as null), so a patch of the snapshot can clear it.
    #[serde(default)]
    pub source: Option<Source>,
    /// Contract value, opening balance and retention (`payments.rs`). Written even when
    /// empty, so a patch of the snapshot can clear it.
    #[serde(default)]
    pub cash_flow: crate::payments::CashFlowSettings,
    /// Payments by milestone or date, in the user's order (`payments.rs`).
    #[serde(default)]
    pub payments: Vec<crate::payments::Payment>,
    /// The id the next payment gets; ids are never reused, like field ids. 0 in files
    /// written before it existed: the next id is then one past the largest.
    #[serde(default)]
    pub next_payment_id: u32,
}

/// How strongly tasks' effort moves together in the finish forecast.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    None,
    #[default]
    Low,
    Medium,
    High,
}

impl Strength {
    /// The share of a task's variation it has in common with the others (a correlation).
    pub fn share(self) -> f64 {
        match self {
            Strength::None => 0.0,
            Strength::Low => 0.3,
            Strength::Medium => 0.5,
            Strength::High => 0.8,
        }
    }
}

/// The finish forecast's correlation: tasks with the same value of `field` (a text or
/// choice custom field) vary together by `group`, and every task shares `common`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastSettings {
    #[serde(default)]
    pub field: Option<u32>,
    #[serde(default = "medium")]
    pub group: Strength,
    #[serde(default)]
    pub common: Strength,
}

fn medium() -> Strength {
    Strength::Medium
}

impl Default for ForecastSettings {
    fn default() -> Self {
        Self { field: None, group: Strength::Medium, common: Strength::Low }
    }
}

impl Project {
    pub fn new(name: impl Into<String>, start: i64) -> Self {
        let calendar = Calendar::standard();
        let calendar_id = calendar.id;
        let start = calendar.next_work_start(start);
        Self {
            name: name.into(),
            start,
            status_date: None,
            calendar_id,
            calendars: vec![calendar],
            tasks: Vec::new(),
            dependencies: Vec::new(),
            resources: Vec::new(),
            assignments: Vec::new(),
            next_task_uid: 1,
            next_dependency_uid: 1,
            next_resource_uid: 1,
            next_assignment_uid: 1,
            next_calendar_uid: 2,
            overallocations: Vec::new(),
            usage: Vec::new(),
            editable_actual_costs: false,
            fields: Vec::new(),
            next_field_id: 1,
            leveling_order: Vec::new(),
            skills: Vec::new(),
            next_skill_id: 1,
            skill_levels: Vec::new(),
            next_skill_level_id: 1,
            assignment_rules: Vec::new(),
            forecast: ForecastSettings::default(),
            focus_factor: crate::focus::full_focus(),
            source: None,
            cash_flow: Default::default(),
            payments: Vec::new(),
            next_payment_id: 1,
        }
    }

    pub fn calendar(&self) -> &Calendar {
        self.calendars
            .iter()
            .find(|c| c.id == self.calendar_id)
            .or_else(|| self.calendars.first())
            .expect("project has a calendar")
    }

    pub fn calendar_mut(&mut self) -> &mut Calendar {
        let id = self.calendar_id;
        if let Some(index) = self.calendars.iter().position(|c| c.id == id) {
            return &mut self.calendars[index];
        }
        &mut self.calendars[0]
    }

    pub fn calendar_by_id(&self, id: u32) -> Option<&Calendar> {
        self.calendars.iter().find(|c| c.id == id)
    }

    pub fn calendar_mut_by_id(&mut self, id: u32) -> Option<&mut Calendar> {
        self.calendars.iter_mut().find(|calendar| calendar.id == id)
    }

    pub fn task_calendar(&self, task: &Task) -> &Calendar {
        task.calendar_id
            .and_then(|id| self.calendar_by_id(id))
            .unwrap_or_else(|| self.calendar())
    }

    pub fn resource_calendar(&self, resource: &Resource) -> &Calendar {
        resource
            .calendar_id
            .and_then(|id| self.calendar_by_id(id))
            .unwrap_or_else(|| self.calendar())
    }

    pub fn index_of(&self, uid: u32) -> Option<usize> {
        self.tasks.iter().position(|t| t.uid == uid)
    }

    pub fn task(&self, uid: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.uid == uid)
    }

    pub fn task_mut(&mut self, uid: u32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.uid == uid)
    }

    pub fn alloc_task_uid(&mut self) -> u32 {
        let uid = self.next_task_uid;
        self.next_task_uid += 1;
        uid
    }

    pub fn alloc_dependency_uid(&mut self) -> u32 {
        let uid = self.next_dependency_uid;
        self.next_dependency_uid += 1;
        uid
    }

    pub fn alloc_resource_uid(&mut self) -> u32 {
        let uid = self.next_resource_uid;
        self.next_resource_uid += 1;
        uid
    }

    pub fn alloc_assignment_uid(&mut self) -> u32 {
        let uid = self.next_assignment_uid;
        self.next_assignment_uid += 1;
        uid
    }

    pub fn alloc_calendar_uid(&mut self) -> u32 {
        let uid = self.next_calendar_uid;
        self.next_calendar_uid += 1;
        uid
    }
}
