use serde::{Deserialize, Serialize};

use crate::availability::{self, AvailabilityPeriod};
use crate::calendar::{Calendar, TimeRange, WorkWeek};
use crate::calendar_base;
use crate::clipboard::{self, FillField, TaskClipboard};
use crate::error::{EngineError, Invalid};
use crate::focus;
use crate::fields::{self, FieldKind, FieldValue, OptionRename};
use crate::import::{self, ImportResource, ImportTask};
use crate::payments::{self, CashFlowSettings, PaymentInput, Receipt};
use crate::plan;
use crate::pull::{self, PullTask};
use crate::push::{self, PushedEntry};
use crate::sync::Resolution;
use crate::level_order::{self, SortKey};
use crate::level::level_project;
use crate::model::{
    Accrual, AssignmentBaseline, Baseline, ConstraintType, DepKind, Dependency, ForecastSettings, Project, Resource,
    ResourceKind, Task, TaskType,
};
use crate::recurrence::{self, Recurrence, RecurrenceSpec};
use crate::schedule::normalize_outline;
use crate::skill_levels::{self, SkillLevelInput, SkillLevelOf};
use crate::skills::{self, AssignmentRule, TaskAssignment};
use crate::source::{self, Source};
use crate::split::{self, Split};
use crate::timeutil::{parse_date_key, MINUTES_PER_DAY, MINUTES_PER_HOUR};
use crate::tracking;
use crate::work::{self, Changed};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    RenameProject {
        name: String,
    },
    SetProjectStart {
        start: i64,
    },
    SetStatusDate {
        date: Option<i64>,
    },
    AddTask {
        name: String,
        after_uid: Option<u32>,
    },
    AddMilestone {
        name: String,
        after_uid: Option<u32>,
    },
    DeleteTask {
        uid: u32,
    },
    RenameTask {
        uid: u32,
        name: String,
    },
    SetDuration {
        uid: u32,
        minutes: i64,
    },
    SetManual {
        uid: u32,
        manual: bool,
        start: Option<i64>,
        finish: Option<i64>,
    },
    SetConstraint {
        uid: u32,
        constraint_type: ConstraintType,
        date: Option<i64>,
    },
    Indent {
        uid: u32,
    },
    Outdent {
        uid: u32,
    },
    SetPredecessors {
        uid: u32,
        text: String,
    },
    AddDependency {
        predecessor_uid: u32,
        successor_uid: u32,
        kind: DepKind,
        lag: i64,
    },
    RemoveDependency {
        uid: u32,
    },
    SetNotes {
        uid: u32,
        notes: String,
    },
    SetPriority {
        uid: u32,
        priority: i32,
    },
    SetTaskCalendar {
        uid: u32,
        calendar_id: Option<u32>,
    },
    AddHoliday {
        date: String,
    },
    RemoveHoliday {
        date: String,
    },
    CreateCalendar {
        name: String,
        /// The new calendar is based on this one (older logs have none: a copy of the
        /// standard calendar).
        #[serde(default)]
        base_id: Option<u32>,
    },
    /// Make another calendar the project calendar.
    SetProjectCalendar {
        calendar_id: u32,
    },
    /// Base a calendar on another, or make it stand alone (`None`); it keeps its hours.
    SetCalendarBase {
        calendar_id: u32,
        base_id: Option<u32>,
    },
    RenameCalendar {
        calendar_id: u32,
        name: String,
    },
    SetCalendarWeek {
        calendar_id: u32,
        week: Vec<Vec<TimeRange>>,
    },
    SetCalendarExceptions {
        calendar_id: u32,
        from: String,
        to: String,
        ranges: Vec<TimeRange>,
    },
    ClearCalendarExceptions {
        calendar_id: u32,
        from: String,
        to: String,
    },
    /// Hours for every day in `[from, to]` that replace the default week (and are
    /// replaced by exceptions). Replaces the work week that starts on `from`.
    SetCalendarWorkWeek {
        calendar_id: u32,
        name: String,
        from: String,
        to: String,
        week: Vec<Vec<TimeRange>>,
    },
    ClearCalendarWorkWeek {
        calendar_id: u32,
        from: String,
    },
    MoveTask {
        uid: u32,
        start: i64,
    },
    ResizeTask {
        uid: u32,
        finish: i64,
    },
    AddResource {
        name: String,
        kind: ResourceKind,
    },
    UpdateResource {
        uid: u32,
        name: String,
        kind: ResourceKind,
        max_units: f64,
        calendar_id: Option<u32>,
        standard_rate: f64,
        overtime_rate: f64,
        cost_per_use: f64,
    },
    DeleteResource {
        uid: u32,
    },
    AssignResource {
        task_uid: u32,
        resource_uid: u32,
        units: f64,
    },
    UpdateAssignment {
        uid: u32,
        units: Option<f64>,
        work: Option<i64>,
        overtime_work: Option<i64>,
    },
    RemoveAssignment {
        uid: u32,
    },
    SetTaskType {
        uid: u32,
        task_type: TaskType,
        effort_driven: bool,
    },
    SetWork {
        uid: u32,
        minutes: i64,
    },
    SetPercentComplete {
        uid: u32,
        percent: f64,
    },
    SetActualStart {
        uid: u32,
        start: Option<i64>,
    },
    SetActualFinish {
        uid: u32,
        finish: Option<i64>,
    },
    SetRemainingDuration {
        uid: u32,
        minutes: i64,
    },
    SetRemainingWork {
        uid: u32,
        minutes: i64,
    },
    SetOvertimeWork {
        uid: u32,
        minutes: i64,
    },
    SetFixedCost {
        uid: u32,
        cost: f64,
        accrual: Accrual,
    },
    SetBaseline {
        number: u8,
    },
    ClearBaseline {
        number: u8,
    },
    /// Run the leveling heuristic. `Document::apply` logs the result as `ApplyLeveling`
    /// instead, so replay never reruns the heuristic; files from before phase 16 still
    /// replay this.
    LevelResources,
    /// Leveling delays chosen by a leveling run: each listed task gets its value. `splits`
    /// lists the tasks whose leveling splits changed, each with all of them (empty for
    /// none); the user's own splits stay.
    ApplyLeveling {
        delays: Vec<LevelingDelay>,
        #[serde(default)]
        splits: Vec<LevelingSplits>,
    },
    ClearLeveling,
    /// Move work that should be done by the status date but is not to after it.
    RescheduleUncompleted,
    SetLevelingDelay {
        uid: u32,
        minutes: i64,
    },
    SetDeadline {
        uid: u32,
        date: Option<i64>,
    },
    /// Turn hand-entered actual costs on or off for the whole project.
    SetEditableActualCosts {
        editable: bool,
    },
    /// Actual cost entered by hand for a task; it counts only while the project's
    /// actual costs are editable. `None` goes back to the computed value.
    SetTaskActualCost {
        uid: u32,
        cost: Option<f64>,
    },
    /// Actual cost entered by hand for one assignment; the task's own entry, if any,
    /// still wins. `None` goes back to the computed share.
    SetAssignmentActualCost {
        uid: u32,
        cost: Option<f64>,
    },
    CreateRecurringTask {
        after_uid: Option<u32>,
        spec: RecurrenceSpec,
    },
    UpdateRecurringTask {
        uid: u32,
        spec: RecurrenceSpec,
    },
    /// Interrupt the work at instant `at` for `gap` working minutes.
    SplitTask {
        uid: u32,
        at: i64,
        gap: i64,
    },
    /// Join the segments around split `index`; `None` joins every segment.
    RemoveSplit {
        uid: u32,
        index: Option<u32>,
    },
    /// Move segment `index` (1 or later) of a split task to start at `start`.
    MoveSegment {
        uid: u32,
        index: u32,
        start: i64,
    },
    /// Insert copied tasks after `after_uid` and its subtree.
    PasteTasks {
        after_uid: Option<u32>,
        clipboard: TaskClipboard,
    },
    /// Copy `field` of `from_uid` to the tasks `to_uids`.
    FillDown {
        from_uid: u32,
        to_uids: Vec<u32>,
        field: FillField,
    },
    /// Declare a custom task field; it gets the next free id.
    AddField {
        name: String,
        kind: FieldKind,
    },
    /// Rename a custom field or change its kind or choices. `renamed_options` carries
    /// choices renamed in place, so tasks keep them.
    UpdateField {
        id: u32,
        name: String,
        kind: FieldKind,
        #[serde(default)]
        renamed_options: Vec<OptionRename>,
    },
    /// Remove a custom field and its values.
    DeleteField {
        id: u32,
    },
    /// Set (or, with `None`, clear) one task's value of a custom field.
    SetFieldValue {
        uid: u32,
        field_id: u32,
        value: Option<FieldValue>,
    },
    /// Replace the order leveling places tasks in; an empty list is the default order.
    /// Dates change only when the project is leveled again.
    SetLevelingOrder {
        order: Vec<SortKey>,
    },
    /// Mark a leaf task urgent: when leveling, it goes ahead of work in progress ranked
    /// below it on the same people (`level_interrupt.rs`).
    SetInterrupts {
        uid: u32,
        interrupts: bool,
    },
    /// Set or clear (both `None`) the effort range of a leaf task for the finish forecast.
    SetWorkRange {
        uid: u32,
        low: Option<i64>,
        high: Option<i64>,
    },
    /// Replace the finish forecast's correlation settings.
    SetForecastSettings {
        settings: ForecastSettings,
    },
    /// Replace the days a resource has other units than its max units. Only
    /// overallocation and leveling read them.
    SetResourceAvailability {
        uid: u32,
        periods: Vec<AvailabilityPeriod>,
    },
    /// Name a new skill; it gets the next free id.
    AddSkill {
        name: String,
    },
    RenameSkill {
        id: u32,
        name: String,
    },
    /// Remove a skill from the project and every resource, with the rules that point at it.
    DeleteSkill {
        id: u32,
    },
    /// Replace a resource's skills, with the level of each skill that has one (absent in
    /// older logs).
    SetResourceSkills {
        uid: u32,
        skills: Vec<u32>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        levels: Vec<SkillLevelOf>,
    },
    /// Replace the scale of skill levels, lowest first (`skill_levels.rs`).
    SetSkillLevels {
        levels: Vec<SkillLevelInput>,
    },
    /// Set (or, empty, clear) a resource's group: its role or team.
    SetResourceGroup {
        uid: u32,
        group: String,
    },
    /// Set the share of time at work people give to project work (`focus.rs`).
    SetFocusFactor {
        factor: f64,
    },
    /// Set (or, `None`, clear) a resource's own productivity factor.
    SetResourceFocus {
        uid: u32,
        #[serde(default)]
        focus: Option<f64>,
    },
    /// Replace the ordered assignment rules. Only suggestions read them.
    SetAssignmentRules {
        rules: Vec<AssignmentRule>,
    },
    /// Assign many tasks in one undo step, each row as `AssignResource` does; one bad row
    /// fails them all.
    AssignTasks {
        assignments: Vec<TaskAssignment>,
    },
    /// Add people and tasks read from a table, in one undo step (`import.rs`). The tasks
    /// go after `after_uid`'s subtree, or at the end; a row with a problem fails it all.
    ImportRows {
        after_uid: Option<u32>,
        #[serde(default)]
        resources: Vec<ImportResource>,
        #[serde(default)]
        tasks: Vec<ImportTask>,
    },
    /// Set (or, empty, clear) a resource's email address.
    SetResourceEmail {
        uid: u32,
        email: String,
    },
    /// Name the external tracker the project is pulled from, or clear it (`source.rs`).
    SetSource {
        source: Option<Source>,
    },
    /// Bring in what a pull from the tracker found, in one undo step (`pull.rs`): new
    /// issues become tasks, linked ones are updated, linked ones the source no longer has
    /// are marked. `pulled_at` is when the pull ran. A pull with a problem or with
    /// nothing to change fails, so it adds no undo step.
    ApplyPull {
        rows: Vec<PullTask>,
        pulled_at: i64,
        /// Present (even empty) for a three-way pull, where a field moves only when the
        /// source changed it and Sheafly did not, and fields changed on both sides need a
        /// choice here (`sync.rs`). Absent for the original pull where the source wins,
        /// which is how older logs replay.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolutions: Option<Vec<Resolution>>,
    },
    /// Remember the days a push wrote to the tracker, on the linked tasks (`push.rs`), so a
    /// later pull knows them as its own. One undo step.
    RecordPush {
        entries: Vec<PushedEntry>,
    },
    /// A plan `plan::preview_plan` showed: assign, then the leveling it chose, in one undo
    /// step. Replay applies the data and never reruns the heuristic.
    ApplyPlan {
        #[serde(default)]
        assignments: Vec<TaskAssignment>,
        #[serde(default)]
        delays: Vec<LevelingDelay>,
        #[serde(default)]
        splits: Vec<LevelingSplits>,
    },
    /// Replace the contract value, opening balance and retention (`payments.rs`).
    SetCashFlowSettings {
        settings: CashFlowSettings,
    },
    /// Replace the payments: an entry with an id keeps that payment and its receipts, one
    /// without is new (`payments.rs`).
    SetPayments {
        payments: Vec<PaymentInput>,
    },
    /// Replace the money received for a payment.
    SetPaymentReceipts {
        id: u32,
        receipts: Vec<Receipt>,
    },
}

/// The splits a leveling run made in one task (`Split::leveling`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelingSplits {
    pub uid: u32,
    pub splits: Vec<Split>,
}

/// The splits of a task leveling made.
pub(crate) fn leveling_splits(task: &Task) -> Vec<Split> {
    task.splits.iter().filter(|split| split.leveling).cloned().collect()
}

/// Replace the splits leveling made in a task, keeping the user's.
pub(crate) fn set_leveling_splits(task: &mut Task, splits: &[Split]) {
    task.splits.retain(|split| !split.leveling);
    task.splits.extend(splits.iter().map(|split| Split { leveling: true, ..split.clone() }));
    split::normalize_task(task);
}

/// One task's leveling delay, in working minutes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LevelingDelay {
    pub uid: u32,
    pub minutes: i64,
}

/// The delays a leveling run would set on `project`, as a command that replays them
/// without running the heuristic again.
pub fn leveled(project: &Project) -> Result<Command, EngineError> {
    let mut trial = project.clone();
    level_project(&mut trial)?;
    let delays = trial
        .tasks
        .iter()
        .filter(|after| {
            project.task(after.uid).map(|before| before.leveling_delay) != Some(after.leveling_delay)
        })
        .map(|after| LevelingDelay { uid: after.uid, minutes: after.leveling_delay })
        .collect();
    let splits = trial
        .tasks
        .iter()
        .filter_map(|after| {
            let made = leveling_splits(after);
            let before = project.task(after.uid).map(leveling_splits).unwrap_or_default();
            (made != before).then_some(LevelingSplits { uid: after.uid, splits: made })
        })
        .collect();
    Ok(Command::ApplyLeveling { delays, splits })
}

pub fn apply_one(project: &mut Project, command: &Command) -> Result<(), EngineError> {
    let edited = edited_task(project, command);
    let anchored = payments::anchored_dates(project);
    apply_inner(project, command)?;
    payments::detach_gone(project, &anchored);
    if let Some(calendar_id) = edited_calendar(project, command) {
        calendar_base::after_edit(project, calendar_id);
    }
    if let Some(task) = edited.and_then(|uid| project.task_mut(uid)) {
        if task.series_uid.is_some() {
            task.detached = true;
        }
    }
    Ok(())
}

/// The calendar whose hours a command edits.
fn edited_calendar(project: &Project, command: &Command) -> Option<u32> {
    match command {
        Command::AddHoliday { .. } | Command::RemoveHoliday { .. } => Some(project.calendar_id),
        Command::SetCalendarWeek { calendar_id, .. }
        | Command::SetCalendarExceptions { calendar_id, .. }
        | Command::ClearCalendarExceptions { calendar_id, .. }
        | Command::SetCalendarWorkWeek { calendar_id, .. }
        | Command::ClearCalendarWorkWeek { calendar_id, .. } => Some(*calendar_id),
        _ => None,
    }
}

/// The single task a command edits, so an edited occurrence can leave its series.
/// Recording progress is not an edit: a meeting that took place stays in its series.
fn edited_task(project: &Project, command: &Command) -> Option<u32> {
    match command {
        Command::RenameTask { uid, .. }
        | Command::SetDuration { uid, .. }
        | Command::SetManual { uid, .. }
        | Command::SetConstraint { uid, .. }
        | Command::SetNotes { uid, .. }
        | Command::SetPriority { uid, .. }
        | Command::SetTaskCalendar { uid, .. }
        | Command::MoveTask { uid, .. }
        | Command::ResizeTask { uid, .. }
        | Command::SetTaskType { uid, .. }
        | Command::SetWork { uid, .. }
        | Command::SetOvertimeWork { uid, .. }
        | Command::SetFixedCost { uid, .. }
        | Command::SetLevelingDelay { uid, .. }
        | Command::SetDeadline { uid, .. }
        | Command::SplitTask { uid, .. }
        | Command::RemoveSplit { uid, .. }
        | Command::MoveSegment { uid, .. }
        | Command::SetFieldValue { uid, .. }
        | Command::SetInterrupts { uid, .. }
        | Command::SetWorkRange { uid, .. } => Some(*uid),
        // Before any progress these set the duration or work: that is an edit.
        Command::SetRemainingDuration { uid, .. } | Command::SetRemainingWork { uid, .. } => project
            .task(*uid)
            .filter(|task| {
                !recurrence::has_progress(task)
                    && !recurrence::with_assignment_entries(project).contains(&task.uid)
            })
            .map(|task| task.uid),
        Command::AssignResource { task_uid, .. } => Some(*task_uid),
        Command::UpdateAssignment { uid, .. } | Command::RemoveAssignment { uid } => project
            .assignments
            .iter()
            .find(|assignment| assignment.uid == *uid)
            .map(|assignment| assignment.task_uid),
        _ => None,
    }
}

fn apply_inner(project: &mut Project, command: &Command) -> Result<(), EngineError> {
    match command {
        Command::RenameProject { name } => project.name = name.clone(),
        Command::SetProjectStart { start } => {
            project.start = project.calendar().next_work_start(*start);
        }
        Command::SetStatusDate { date } => project.status_date = *date,
        Command::AddTask { name, after_uid } => {
            let label = if name.trim().is_empty() {
                format!("Công việc {}", project.next_task_uid)
            } else {
                name.clone()
            };
            let uid = project.alloc_task_uid();
            insert_task(project, *after_uid, Task::new(uid, label))?;
        }
        Command::AddMilestone { name, after_uid } => {
            let label = if name.trim().is_empty() {
                "Mốc".into()
            } else {
                name.clone()
            };
            let uid = project.alloc_task_uid();
            let mut task = Task::new(uid, label);
            task.duration = 0;
            task.work = 0;
            task.remaining_duration = 0;
            task.remaining_work = 0;
            task.milestone = true;
            insert_task(project, *after_uid, task)?;
        }
        Command::DeleteTask { uid } => delete_task(project, *uid)?,
        Command::RenameTask { uid, name } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            if let Some(series) = task.recurrence.as_mut() {
                series.spec.name = name.clone();
                recurrence::regenerate(project, *uid)?;
            } else {
                task.name = name.clone();
            }
        }
        Command::SetDuration { uid, minutes } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            if task.summary {
                return Ok(());
            }
            task.duration = (*minutes).max(0);
            task.milestone = task.duration == 0;
            work::apply_change(project, *uid, Changed::Duration);
        }
        Command::SetManual {
            uid,
            manual,
            start,
            finish,
        } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            if task.summary {
                return Ok(());
            }
            task.manual = *manual;
            if *manual {
                if let Some(start) = start {
                    task.start = *start;
                }
                if let Some(finish) = finish {
                    task.finish = *finish;
                }
            }
        }
        Command::SetConstraint {
            uid,
            constraint_type,
            date,
        } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.constraint_type = *constraint_type;
            task.constraint_date = *date;
        }
        Command::Indent { uid } => indent(project, *uid)?,
        Command::Outdent { uid } => outdent(project, *uid)?,
        Command::SetPredecessors { uid, text } => set_predecessors(project, *uid, text)?,
        Command::AddDependency {
            predecessor_uid,
            successor_uid,
            kind,
            lag,
        } => {
            if predecessor_uid == successor_uid {
                return Err(EngineError::DependencyCycle);
            }
            if project.task(*predecessor_uid).is_none() || project.task(*successor_uid).is_none() {
                return Err(EngineError::TaskNotFound);
            }
            if let Some(existing) = project.dependencies.iter_mut().find(|dep| {
                dep.predecessor_uid == *predecessor_uid && dep.successor_uid == *successor_uid
            }) {
                existing.kind = *kind;
                existing.lag = *lag;
            } else {
                let uid = project.alloc_dependency_uid();
                project.dependencies.push(Dependency {
                    uid,
                    predecessor_uid: *predecessor_uid,
                    successor_uid: *successor_uid,
                    kind: *kind,
                    lag: *lag,
                });
            }
        }
        Command::RemoveDependency { uid } => {
            project.dependencies.retain(|dep| dep.uid != *uid);
        }
        Command::SetNotes { uid, notes } => {
            project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?.notes = notes.clone();
        }
        Command::SetPriority { uid, priority } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.priority = (*priority).clamp(0, 1000);
        }
        Command::SetTaskCalendar { uid, calendar_id } => {
            if let Some(id) = calendar_id {
                if project.calendar_by_id(*id).is_none() {
                    return Err(EngineError::from(Invalid::NoSuchCalendar));
                }
            }
            project
                .task_mut(*uid)
                .ok_or(EngineError::TaskNotFound)?
                .calendar_id = *calendar_id;
        }
        Command::AddHoliday { date } => {
            let naive = parse_date_key(date).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            project.calendar_mut().set_day_off(naive);
        }
        Command::RemoveHoliday { date } => {
            let naive = parse_date_key(date).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            if calendar_base::inherits_exception(project, project.calendar_id, naive, naive) {
                return Err(Invalid::InheritedFromBase.into());
            }
            project.calendar_mut().clear_exception(naive);
        }
        Command::CreateCalendar { name, base_id } => {
            if base_id.is_some_and(|id| project.calendar_by_id(id).is_none()) {
                return Err(Invalid::NoSuchCalendar.into());
            }
            let uid = project.alloc_calendar_uid();
            let mut calendar = Calendar::standard();
            calendar.id = uid;
            let label = name.trim();
            calendar.name = if label.is_empty() {
                format!("Lịch {uid}")
            } else {
                label.to_string()
            };
            if let Some(base) = base_id.and_then(|id| project.calendar_by_id(id)) {
                // Start as the base, owning nothing.
                calendar.week = base.week.clone();
                calendar.exceptions = base.exceptions.clone();
                calendar.work_weeks = base.work_weeks.clone();
                calendar.base = Some(calendar_base::capture(&calendar, base));
            }
            project.calendars.push(calendar);
        }
        Command::SetProjectCalendar { calendar_id } => {
            let calendar = project
                .calendar_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            if !calendar.has_working_time() {
                return Err(EngineError::NoWorkingTime);
            }
            project.calendar_id = *calendar_id;
        }
        Command::SetCalendarBase { calendar_id, base_id } => {
            calendar_base::set_base(project, *calendar_id, *base_id)?;
        }
        Command::RenameCalendar { calendar_id, name } => {
            let label = name.trim();
            if label.is_empty() {
                return Err(Invalid::EmptyCalendarName.into());
            }
            project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?
                .name = label.to_string();
        }
        Command::SetCalendarWeek { calendar_id, week } => {
            if week.len() != 7 {
                return Err(Invalid::WeekNeedsSevenDays.into());
            }
            let mut days: [Vec<TimeRange>; 7] = Default::default();
            for (index, day) in week.iter().enumerate() {
                days[index] = normalize_ranges(day)?;
            }
            let calendar = project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            calendar.week = days;
            if !calendar.has_working_time() {
                return Err(EngineError::NoWorkingTime);
            }
        }
        Command::SetCalendarExceptions {
            calendar_id,
            from,
            to,
            ranges,
        } => {
            let from_date = parse_date_key(from)
                .ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            let to_date =
                parse_date_key(to).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            let ranges = normalize_ranges(ranges)?;
            let calendar = project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            if !calendar.set_exception_span(from_date, to_date, ranges) {
                return Err(Invalid::DateRangeTooLong.into());
            }
            if !calendar.has_working_time() {
                return Err(EngineError::NoWorkingTime);
            }
        }
        Command::ClearCalendarExceptions {
            calendar_id,
            from,
            to,
        } => {
            let from_date = parse_date_key(from)
                .ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            let to_date =
                parse_date_key(to).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            if calendar_base::inherits_exception(project, *calendar_id, from_date, to_date) {
                return Err(Invalid::InheritedFromBase.into());
            }
            let calendar = project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            if !calendar.clear_exception_span(from_date, to_date) {
                return Err(Invalid::DateRangeTooLong.into());
            }
            if !calendar.has_working_time() {
                return Err(EngineError::NoWorkingTime);
            }
        }
        Command::SetCalendarWorkWeek {
            calendar_id,
            name,
            from,
            to,
            week,
        } => {
            if week.len() != 7 {
                return Err(Invalid::WeekNeedsSevenDays.into());
            }
            let from = parse_date_key(from).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            let to = parse_date_key(to).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            let mut days: [Vec<TimeRange>; 7] = Default::default();
            for (index, day) in week.iter().enumerate() {
                days[index] = normalize_ranges(day)?;
            }
            calendar_base::make_room_for_work_week(project, *calendar_id, from, to);
            let calendar = project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            calendar.set_work_week(WorkWeek {
                name: name.trim().to_string(),
                from,
                to,
                week: days,
            })?;
            if !calendar.has_working_time() {
                return Err(EngineError::NoWorkingTime);
            }
        }
        Command::ClearCalendarWorkWeek { calendar_id, from } => {
            let from = parse_date_key(from).ok_or_else(|| EngineError::from(Invalid::BadDate))?;
            if calendar_base::inherits_work_week(project, *calendar_id, from) {
                return Err(Invalid::InheritedFromBase.into());
            }
            let calendar = project
                .calendar_mut_by_id(*calendar_id)
                .ok_or_else(|| EngineError::from(Invalid::NoSuchCalendar))?;
            if !calendar.remove_work_week(from) {
                return Err(Invalid::NoSuchWorkWeek.into());
            }
        }
        Command::MoveTask { uid, start } => move_task(project, *uid, *start)?,
        Command::ResizeTask { uid, finish } => resize_task(project, *uid, *finish)?,
        Command::AddResource { name, kind } => {
            let uid = project.alloc_resource_uid();
            let label = if name.trim().is_empty() {
                format!("Nguồn lực {uid}")
            } else {
                name.clone()
            };
            project.resources.push(Resource::new(uid, label, *kind));
        }
        Command::UpdateResource {
            uid,
            name,
            kind,
            max_units,
            calendar_id,
            standard_rate,
            overtime_rate,
            cost_per_use,
        } => {
            if let Some(id) = calendar_id {
                if project.calendar_by_id(*id).is_none() {
                    return Err(EngineError::from(Invalid::NoSuchCalendar));
                }
            }
            let resource = project
                .resources
                .iter_mut()
                .find(|resource| resource.uid == *uid)
                .ok_or(EngineError::ResourceNotFound)?;
            let kind_changed = resource.kind != *kind;
            resource.name = name.clone();
            resource.kind = *kind;
            resource.max_units = (*max_units).max(0.0);
            resource.calendar_id = *calendar_id;
            resource.standard_rate = *standard_rate;
            resource.overtime_rate = *overtime_rate;
            resource.cost_per_use = *cost_per_use;
            // The kind decides whether the project's factor applies.
            if kind_changed && resource.focus.is_none() && project.focus_factor != 1.0 {
                focus::reapply_resource(project, *uid);
            }
        }
        Command::DeleteResource { uid } => {
            project.resources.retain(|resource| resource.uid != *uid);
            skills::drop_resource(project, *uid);
            let assignment_uids: Vec<u32> = project
                .assignments
                .iter()
                .filter(|assignment| assignment.resource_uid == *uid)
                .map(|assignment| assignment.uid)
                .collect();
            for assignment_uid in assignment_uids {
                work::remove_assignment(project, assignment_uid);
            }
        }
        Command::AssignResource {
            task_uid,
            resource_uid,
            units,
        } => {
            if project.task(*task_uid).is_none() {
                return Err(EngineError::TaskNotFound);
            }
            if project
                .resources
                .iter()
                .all(|resource| resource.uid != *resource_uid)
            {
                return Err(EngineError::ResourceNotFound);
            }
            work::add_assignment(project, *task_uid, *resource_uid, *units);
        }
        Command::UpdateAssignment {
            uid,
            units,
            work,
            overtime_work,
        } => {
            let task_uid = project
                .assignments
                .iter()
                .find(|assignment| assignment.uid == *uid)
                .map(|assignment| assignment.task_uid)
                .ok_or(EngineError::AssignmentNotFound)?;
            if let Some(overtime_work) = overtime_work {
                if let Some(assignment) = project
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.uid == *uid)
                {
                    assignment.overtime_work = (*overtime_work).max(0);
                }
            }
            if let Some(units) = units {
                if let Some(assignment) = project
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.uid == *uid)
                {
                    assignment.units = (*units).max(0.0);
                }
                work::apply_change(project, task_uid, Changed::Units);
            }
            if let Some(work_minutes) = work {
                if let Some(assignment) = project
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.uid == *uid)
                {
                    assignment.work = (*work_minutes).max(0);
                }
                work::apply_change(project, task_uid, Changed::Work);
            }
        }
        Command::RemoveAssignment { uid } => {
            if !work::remove_assignment(project, *uid) {
                return Err(EngineError::AssignmentNotFound);
            }
        }
        Command::SetTaskType {
            uid,
            task_type,
            effort_driven,
        } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.task_type = *task_type;
            task.effort_driven = if *task_type == TaskType::FixedWork {
                true
            } else {
                *effort_driven
            };
        }
        Command::SetWork { uid, minutes } => set_work(project, *uid, *minutes)?,
        Command::SetPercentComplete { uid, percent } => {
            tracking::set_percent_complete(project, *uid, *percent)?
        }
        Command::SetActualStart { uid, start } => tracking::set_actual_start(project, *uid, *start)?,
        Command::SetActualFinish { uid, finish } => {
            tracking::set_actual_finish(project, *uid, *finish)?
        }
        Command::SetRemainingDuration { uid, minutes } => {
            tracking::set_remaining_duration(project, *uid, *minutes)?
        }
        Command::SetRemainingWork { uid, minutes } => {
            tracking::set_remaining_work(project, *uid, *minutes, set_work)?
        }
        Command::RescheduleUncompleted => tracking::reschedule_uncompleted(project)?,
        Command::SetOvertimeWork { uid, minutes } => {
            set_overtime_work(project, *uid, *minutes)?;
        }
        Command::SetFixedCost { uid, cost, accrual } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.fixed_cost = *cost;
            task.fixed_cost_accrual = *accrual;
        }
        Command::SetBaseline { number } => {
            let number = (*number).min(10);
            for task in &mut project.tasks {
                task.baselines.retain(|baseline| baseline.number != number);
                task.baselines.push(Baseline {
                    number,
                    start: task.start,
                    finish: task.finish,
                    work: task.work,
                    cost: task.cost,
                    segments: task.segments.clone(),
                    fixed_cost: Some(task.fixed_cost),
                });
            }
            for assignment in &mut project.assignments {
                assignment.baselines.retain(|baseline| baseline.number != number);
                assignment.baselines.push(AssignmentBaseline {
                    number,
                    work: assignment.work,
                    cost: assignment.cost,
                });
            }
        }
        Command::ClearBaseline { number } => {
            let number = (*number).min(10);
            for task in &mut project.tasks {
                task.baselines.retain(|baseline| baseline.number != number);
            }
            for assignment in &mut project.assignments {
                assignment.baselines.retain(|baseline| baseline.number != number);
            }
        }
        Command::LevelResources => level_project(project)?,
        Command::ApplyLeveling { delays, splits } => {
            for delay in delays {
                if let Some(task) = project.task_mut(delay.uid) {
                    task.leveling_delay = delay.minutes.max(0);
                }
            }
            for made in splits {
                if let Some(task) = project.task_mut(made.uid) {
                    set_leveling_splits(task, &made.splits);
                }
            }
        }
        Command::ClearLeveling => {
            for task in &mut project.tasks {
                task.leveling_delay = 0;
                task.splits.retain(|split| !split.leveling);
            }
        }
        Command::SetLevelingDelay { uid, minutes } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.leveling_delay = (*minutes).max(0);
        }
        Command::SetDeadline { uid, date } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            task.deadline = *date;
        }
        Command::SetEditableActualCosts { editable } => project.editable_actual_costs = *editable,
        Command::SetTaskActualCost { uid, cost } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            if task.summary {
                return Ok(());
            }
            task.manual_actual_cost = cost.map(|cost| cost.max(0.0));
        }
        Command::SetAssignmentActualCost { uid, cost } => {
            let assignment = project
                .assignments
                .iter_mut()
                .find(|assignment| assignment.uid == *uid)
                .ok_or(EngineError::AssignmentNotFound)?;
            assignment.manual_actual_cost = cost.map(|cost| cost.max(0.0));
        }
        Command::CreateRecurringTask { after_uid, spec } => {
            let spec = recurrence::clean_spec(spec);
            recurrence::validate_spec(project, &spec)?;
            // A new series inside another series goes after that whole series.
            let after_uid = after_uid.map(|uid| {
                project
                    .task(uid)
                    .and_then(|task| task.series_uid)
                    .unwrap_or(uid)
            });
            let uid = project.alloc_task_uid();
            let mut summary = Task::new(uid, spec.name.clone());
            summary.recurrence = Some(Recurrence {
                spec,
                skipped: Vec::new(),
            });
            insert_task(project, after_uid, summary)?;
            recurrence::regenerate(project, uid)?;
        }
        Command::UpdateRecurringTask { uid, spec } => {
            let spec = recurrence::clean_spec(spec);
            recurrence::validate_spec(project, &spec)?;
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            let current = task
                .recurrence
                .as_mut()
                .ok_or_else(|| EngineError::from(Invalid::NotRecurring))?;
            current.spec = spec;
            recurrence::regenerate(project, *uid)?;
        }
        Command::SplitTask { uid, at, gap } => split::split_task(project, *uid, *at, *gap)?,
        Command::RemoveSplit { uid, index } => split::remove_split(project, *uid, *index)?,
        Command::MoveSegment { uid, index, start } => {
            split::move_segment(project, *uid, *index, *start)?
        }
        Command::PasteTasks { after_uid, clipboard } => {
            clipboard::paste_tasks(project, *after_uid, clipboard)?
        }
        Command::FillDown { from_uid, to_uids, field } => {
            clipboard::fill_down(project, *from_uid, to_uids, *field)?
        }
        Command::AddField { name, kind } => {
            fields::add_field(project, name, kind)?;
        }
        Command::UpdateField { id, name, kind, renamed_options } => {
            fields::update_field(project, *id, name, kind, renamed_options)?
        }
        Command::DeleteField { id } => fields::delete_field(project, *id)?,
        Command::SetFieldValue { uid, field_id, value } => {
            fields::set_value(project, *uid, *field_id, value.as_ref())?
        }
        Command::SetLevelingOrder { order } => level_order::set_order(project, order)?,
        Command::SetResourceAvailability { uid, periods } => {
            availability::set_availability(project, *uid, periods)?
        }
        Command::SetInterrupts { uid, interrupts } => {
            let task = project.task_mut(*uid).ok_or(EngineError::TaskNotFound)?;
            if task.summary || task.milestone {
                return Err(Invalid::InterruptsLeaf.into());
            }
            task.interrupts = *interrupts;
        }
        Command::SetWorkRange { uid, low, high } => set_work_range(project, *uid, *low, *high)?,
        Command::SetForecastSettings { settings } => set_forecast_settings(project, settings)?,
        Command::AddSkill { name } => {
            skills::add_skill(project, name)?;
        }
        Command::RenameSkill { id, name } => skills::rename_skill(project, *id, name)?,
        Command::DeleteSkill { id } => skills::delete_skill(project, *id)?,
        Command::SetResourceSkills { uid, skills, levels } => {
            skills::set_resource_skills(project, *uid, skills, levels)?
        }
        Command::SetSkillLevels { levels } => skill_levels::set_levels(project, levels)?,
        Command::SetResourceGroup { uid, group } => skill_levels::set_group(project, *uid, group)?,
        Command::SetFocusFactor { factor } => focus::set_project_focus(project, *factor)?,
        Command::SetResourceFocus { uid, focus } => focus::set_resource_focus(project, *uid, *focus)?,
        Command::SetAssignmentRules { rules } => skills::set_rules(project, rules)?,
        Command::AssignTasks { assignments } => {
            if assignments.is_empty() {
                return Err(Invalid::NoAssignments.into());
            }
            for row in assignments {
                if !row.units.is_finite() || row.units <= 0.0 {
                    return Err(Invalid::UnitsNotPositive.into());
                }
                let command = Command::AssignResource {
                    task_uid: row.task_uid,
                    resource_uid: row.resource_uid,
                    units: row.units,
                };
                apply_one(project, &command)?;
            }
        }
        Command::ImportRows {
            after_uid,
            resources,
            tasks,
        } => import::import_rows(project, *after_uid, resources, tasks)?,
        Command::ApplyPlan {
            assignments,
            delays,
            splits,
        } => plan::apply_plan(project, assignments, delays, splits)?,
        Command::SetResourceEmail { uid, email } => source::set_resource_email(project, *uid, email)?,
        Command::SetSource { source } => source::set_source(project, source)?,
        Command::ApplyPull { rows, pulled_at, resolutions } => {
            pull::apply_pull(project, rows, *pulled_at, resolutions.as_deref())?
        }
        Command::RecordPush { entries } => push::record_push(project, entries)?,
        Command::SetCashFlowSettings { settings } => payments::set_settings(project, settings)?,
        Command::SetPayments { payments: list } => payments::set_payments(project, list)?,
        Command::SetPaymentReceipts { id, receipts } => {
            payments::set_receipts(project, *id, receipts)?
        }
    }
    Ok(())
}

fn insert_task(project: &mut Project, after_uid: Option<u32>, mut task: Task) -> Result<(), EngineError> {
    normalize_outline(project);
    let (index, level) = if let Some(uid) = after_uid {
        let position = project.index_of(uid).ok_or(EngineError::TaskNotFound)?;
        let level = project.tasks[position].outline_level;
        (subtree_end(&project.tasks, position), level)
    } else if let Some(last) = project.tasks.last() {
        (project.tasks.len(), last.outline_level)
    } else {
        (0, 1)
    };
    task.outline_level = level;
    task.start = project.start;
    task.finish = project.start;
    project.tasks.insert(index, task);
    Ok(())
}

fn delete_task(project: &mut Project, uid: u32) -> Result<(), EngineError> {
    normalize_outline(project);
    let index = project.index_of(uid).ok_or(EngineError::TaskNotFound)?;
    if project.tasks[index].recurrence.is_some() {
        return recurrence::delete_series(project, uid);
    }
    recurrence::skip_occurrence(project, uid);
    let end = subtree_end(&project.tasks, index);
    let uids: Vec<u32> = project.tasks[index..end].iter().map(|task| task.uid).collect();
    project.tasks.drain(index..end);
    project.dependencies.retain(|dep| {
        !uids.contains(&dep.predecessor_uid) && !uids.contains(&dep.successor_uid)
    });
    project
        .assignments
        .retain(|assignment| !uids.contains(&assignment.task_uid));
    Ok(())
}

fn indent(project: &mut Project, uid: u32) -> Result<(), EngineError> {
    normalize_outline(project);
    let index = project.index_of(uid).ok_or(EngineError::TaskNotFound)?;
    if index == 0 {
        return Ok(());
    }
    let previous = &project.tasks[index - 1];
    if project.tasks[index].series_uid.is_some()
        || (previous.series_uid.is_some()
            && project.tasks[index].outline_level >= previous.outline_level)
    {
        return Err(occurrence_level_error());
    }
    let max_level = project.tasks[index - 1].outline_level + 1;
    if project.tasks[index].outline_level >= max_level {
        return Ok(());
    }
    let end = subtree_end(&project.tasks, index);
    for task in &mut project.tasks[index..end] {
        task.outline_level += 1;
    }
    Ok(())
}

fn outdent(project: &mut Project, uid: u32) -> Result<(), EngineError> {
    normalize_outline(project);
    let index = project.index_of(uid).ok_or(EngineError::TaskNotFound)?;
    if project.tasks[index].series_uid.is_some() {
        return Err(occurrence_level_error());
    }
    if project.tasks[index].outline_level <= 1 {
        return Ok(());
    }
    let end = subtree_end(&project.tasks, index);
    for task in &mut project.tasks[index..end] {
        task.outline_level = task.outline_level.saturating_sub(1).max(1);
    }
    Ok(())
}

fn occurrence_level_error() -> EngineError {
    Invalid::OccurrenceOutline.into()
}

pub(crate) fn subtree_end(tasks: &[Task], index: usize) -> usize {
    let level = tasks[index].outline_level;
    let mut end = index + 1;
    while end < tasks.len() && tasks[end].outline_level > level {
        end += 1;
    }
    end
}

fn move_task(project: &mut Project, uid: u32, start: i64) -> Result<(), EngineError> {
    let manual = project.task(uid).ok_or(EngineError::TaskNotFound)?.manual;
    let summary = project.task(uid).unwrap().summary;
    if summary {
        return Ok(());
    }
    if manual {
        let span = split::span(project.task(uid).unwrap());
        let cal = project.task_calendar(project.task(uid).unwrap()).clone();
        let start = cal.next_work_start(start);
        let finish = if span <= 0 {
            start
        } else {
            cal.add_working(start, span)
        };
        let task = project.task_mut(uid).unwrap();
        task.start = start;
        task.finish = finish;
    } else {
        let task = project.task_mut(uid).unwrap();
        task.constraint_type = ConstraintType::Snet;
        task.constraint_date = Some(start);
    }
    Ok(())
}

fn resize_task(project: &mut Project, uid: u32, finish: i64) -> Result<(), EngineError> {
    let task = project.task(uid).ok_or(EngineError::TaskNotFound)?;
    if task.summary {
        return Ok(());
    }
    let cal = project.task_calendar(task).clone();
    let start = task.start;
    let working_span = cal.working_between(start, finish).max(0);
    let task = project.task_mut(uid).unwrap();
    let minutes = split::resize_duration(task, working_span);
    task.duration = minutes;
    task.milestone = minutes == 0;
    if task.manual {
        task.finish = finish.max(start);
    }
    work::apply_change(project, uid, Changed::Duration);
    Ok(())
}

fn set_work(project: &mut Project, uid: u32, minutes: i64) -> Result<(), EngineError> {
    let task = project.task(uid).ok_or(EngineError::TaskNotFound)?;
    if task.summary {
        return Ok(());
    }
    let minutes = minutes.max(0);
    let indexes: Vec<usize> = project
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, assignment)| assignment.task_uid == uid)
        .map(|(index, _)| index)
        .collect();
    if indexes.is_empty() {
        project.task_mut(uid).unwrap().work = minutes;
    } else {
        let old: i64 = indexes.iter().map(|i| project.assignments[*i].work).sum();
        if old > 0 {
            for index in &indexes {
                let share = project.assignments[*index].work as f64 / old as f64;
                project.assignments[*index].work = (share * minutes as f64).round().max(0.0) as i64;
            }
        } else if let Some(index) = indexes.first() {
            project.assignments[*index].work = minutes;
        }
    }
    work::apply_change(project, uid, Changed::Work);
    Ok(())
}

fn set_overtime_work(project: &mut Project, uid: u32, minutes: i64) -> Result<(), EngineError> {
    let task = project.task(uid).ok_or(EngineError::TaskNotFound)?;
    if task.summary {
        return Ok(());
    }
    let minutes = minutes.max(0);
    let indexes: Vec<usize> = project
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, assignment)| assignment.task_uid == uid)
        .map(|(index, _)| index)
        .collect();
    if indexes.is_empty() {
        return Err(Invalid::OvertimeNeedsAssignment.into());
    }
    let old: i64 = indexes.iter().map(|i| project.assignments[*i].work).sum();
    if old <= 0 {
        project.assignments[indexes[0]].overtime_work = minutes;
        for index in indexes.iter().skip(1) {
            project.assignments[*index].overtime_work = 0;
        }
    } else {
        let mut assigned = 0i64;
        for (offset, index) in indexes.iter().enumerate() {
            let piece = if offset + 1 == indexes.len() {
                minutes - assigned
            } else {
                let share = (project.assignments[*index].work as f64 / old as f64 * minutes as f64)
                    .round() as i64;
                assigned += share;
                share
            };
            project.assignments[*index].overtime_work = piece.max(0);
        }
    }
    for index in indexes {
        let work = project.assignments[index].work.max(0);
        if project.assignments[index].overtime_work > work {
            project.assignments[index].overtime_work = work;
        }
    }
    Ok(())
}

pub fn parse_predecessor_text(text: &str) -> Result<Vec<(u32, DepKind, i64)>, EngineError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut links = Vec::new();
    for token in trimmed.split([',', ';']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        links.push(parse_predecessor_token(token)?);
    }
    Ok(links)
}

fn parse_predecessor_token(token: &str) -> Result<(u32, DepKind, i64), EngineError> {
    let mut chars = token.chars().peekable();
    let mut id_text = String::new();
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        id_text.push(chars.next().unwrap());
    }
    if id_text.is_empty() {
        return Err(EngineError::BadPredecessor(token.into()));
    }
    let id: u32 = id_text
        .parse()
        .map_err(|_| EngineError::BadPredecessor(token.into()))?;
    let rest: String = chars.collect();
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((id, DepKind::Fs, 0));
    }
    let kind = rest
        .get(..2)
        .and_then(DepKind::parse)
        .unwrap_or(DepKind::Fs);
    let after = if rest.get(..2).and_then(DepKind::parse).is_some() {
        rest[2..].trim()
    } else {
        rest
    };
    let lag = parse_lag(after).map_err(|_| EngineError::BadPredecessor(token.into()))?;
    Ok((id, kind, lag))
}

fn parse_lag(text: &str) -> Result<i64, ()> {
    let text = text.trim().replace(',', ".");
    if text.is_empty() {
        return Ok(0);
    }
    let sign = if text.starts_with('-') { -1i64 } else { 1 };
    let text = text.trim_start_matches(['+', '-']).trim();
    let mut number = String::new();
    let mut unit = String::new();
    for ch in text.chars() {
        if (ch.is_ascii_digit() || ch == '.') && unit.is_empty() {
            number.push(ch);
        } else if ch.is_whitespace() {
            continue;
        } else {
            unit.push(ch);
        }
    }
    let value: f64 = number.parse().map_err(|_| ())?;
    let minutes = match unit.to_lowercase().as_str() {
        "" | "d" | "day" | "days" | "ngày" | "ngay" => value * MINUTES_PER_DAY as f64,
        "h" | "hr" | "hour" | "hours" | "giờ" | "gio" => value * MINUTES_PER_HOUR as f64,
        "m" | "min" | "phút" | "phut" => value,
        _ => return Err(()),
    };
    Ok(sign * minutes.round() as i64)
}

fn set_predecessors(project: &mut Project, uid: u32, text: &str) -> Result<(), EngineError> {
    if project.task(uid).is_none() {
        return Err(EngineError::TaskNotFound);
    }
    normalize_outline(project);
    let links = parse_predecessor_text(text)?;
    project.dependencies.retain(|dep| dep.successor_uid != uid);
    for (id, kind, lag) in links {
        let predecessor_uid = project
            .tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.uid)
            .ok_or_else(|| EngineError::BadPredecessor(id.to_string()))?;
        if predecessor_uid == uid {
            return Err(EngineError::DependencyCycle);
        }
        let dep_uid = project.alloc_dependency_uid();
        project.dependencies.push(Dependency {
            uid: dep_uid,
            predecessor_uid,
            successor_uid: uid,
            kind,
            lag,
        });
    }
    Ok(())
}

fn normalize_ranges(ranges: &[TimeRange]) -> Result<Vec<TimeRange>, EngineError> {
    let mut ordered = ranges.to_vec();
    ordered.sort_by_key(|range| (range.start_minute, range.end_minute));
    let mut last_end = 0u16;
    for range in &ordered {
        if range.end_minute > 24 * 60 || range.start_minute >= range.end_minute {
            return Err(Invalid::BadTimeRange.into());
        }
        if range.start_minute < last_end {
            return Err(Invalid::OverlappingTimeRanges.into());
        }
        last_end = range.end_minute;
    }
    Ok(ordered)
}

/// Both amounts or neither, `0 <= low <= high`, on a task that has work of its own.
pub(crate) fn set_work_range(
    project: &mut Project,
    uid: u32,
    low: Option<i64>,
    high: Option<i64>,
) -> Result<(), EngineError> {
    let task = project.task_mut(uid).ok_or(EngineError::TaskNotFound)?;
    if task.summary || task.milestone {
        return Err(Invalid::WorkRangeLeaf.into());
    }
    match (low, high) {
        (None, None) => {}
        (Some(low), Some(high)) if 0 <= low && low <= high => {}
        _ => return Err(Invalid::BadWorkRange.into()),
    }
    task.work_low = low;
    task.work_high = high;
    Ok(())
}

/// The field, when given, must be a text or choice field of the project.
fn set_forecast_settings(project: &mut Project, settings: &ForecastSettings) -> Result<(), EngineError> {
    if let Some(id) = settings.field {
        let field = project.fields.iter().find(|field| field.id == id).ok_or(Invalid::NoSuchField)?;
        if matches!(field.kind, crate::fields::FieldKind::Number) {
            return Err(Invalid::ForecastFieldKind.into());
        }
    }
    project.forecast = settings.clone();
    Ok(())
}
