//! Pulling issues from an external task tracker (phase 19, one way).
//!
//! An adapter in the Tauri layer reads the tracker and hands the engine typed rows
//! (`PullTask`): what each issue says, already in project units (working minutes, instants,
//! the status as a `StatusClass`). `Command::ApplyPull` carries the rows themselves, like
//! `ImportRows` carries its tasks, so replay never touches the network. It is one undo
//! step, and fails (adding no step) when a row has a problem or when nothing would change,
//! so pulling again unchanged data leaves the project exactly as it was.
//!
//! Rows are the whole result of the pull. An issue is matched to a task by its key
//! (`Task::external`): a new key adds a task, a known one updates the task, and a linked
//! task whose key is missing from the rows is marked gone (never deleted). The source is
//! the authority for what it sends, and only for that:
//!
//! - title, due date, status and the custom field values follow the source, and so does a
//!   start date (a "start no earlier than" constraint, which links and leveling can still
//!   move past; a manual task would be out of their reach). A due date or start date the
//!   source no longer has is cleared;
//! - effort and assignee are taken when the source has one: an estimate or an owner added
//!   in Sheafly stays when the source has none. The assignee becomes the task's only
//!   assignment at 1 unit (units set later by hand stay while the person is the same);
//! - a status becomes progress: not started 0 %, done 100 %, in progress keeps the percent
//!   already between 0 and 100, else 50 %;
//! - a task with children in the pull is a group: it takes no effort, start, owner or
//!   progress from the source, they roll up from its children;
//! - a new task goes under its parent when the pull names one (at the end of its
//!   children), else at the end of the project. A task already in the project is not
//!   moved when the source changes its parent.
//!
//! `diff` runs the same code on a copy and reports what would happen, row by row, for the
//! preview; the command runs it for real.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::command::{apply_one, subtree_end, Command};
use crate::error::{EngineError, Invalid};
use crate::fields::{self, FieldValue};
use crate::lang::Lang;
use crate::model::{ConstraintType, DepKind, Dependency, Project, Task, TaskType};
use crate::schedule::{normalize_outline, schedule};
use crate::source::{ExternalLink, OnConflict, StatusClass, SyncSettings};
use crate::sync::{self, Resolution, Side, SyncField, Verdict};
use crate::timeutil::{civil, date_key};
use crate::work;

/// Issues a pull can have.
pub const MAX_PULL_ROWS: usize = 20_000;
/// The percent an in-progress issue gets when the task has none between 0 and 100.
const IN_PROGRESS_PERCENT: f64 = 50.0;

/// One issue, as the adapter read it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PullTask {
    /// The issue's key at the source, such as `PROJ-123`.
    pub key: String,
    /// The key of its parent issue; the parent's row comes before this one.
    #[serde(default)]
    pub parent_key: Option<String>,
    pub name: String,
    /// Estimated effort in working minutes.
    #[serde(default)]
    pub work: Option<i64>,
    /// Start date as an instant.
    #[serde(default)]
    pub start: Option<i64>,
    /// Due date as an instant.
    #[serde(default)]
    pub due: Option<i64>,
    #[serde(default)]
    pub status: Option<StatusClass>,
    /// Email of the person it is assigned to; they must be in the project.
    #[serde(default)]
    pub assignee_email: Option<String>,
    /// Their name, for a tracker that hides emails: used when there is no email, and it must
    /// then be the name of exactly one person of the project.
    #[serde(default)]
    pub assignee_name: Option<String>,
    /// Custom field values by field name, letter case aside; an empty value clears.
    #[serde(default)]
    pub values: Vec<(String, FieldValue)>,
    /// Keys of the issues it waits for (finish to start).
    #[serde(default)]
    pub after: Vec<String>,
    /// What the field a push writes the computed start / finish to holds now (as the day's
    /// instant), when that is a field of its own (`planned_start` / `planned_finish`). A
    /// pull never reads them; a push compares with them.
    #[serde(default)]
    pub planned_start: Option<i64>,
    #[serde(default)]
    pub planned_finish: Option<i64>,
}

/// What a row changes in its task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PullField {
    Name,
    Work,
    Start,
    Due,
    Assignee,
    Status,
    Fields,
    Links,
    /// The issue is back at the source after being gone.
    Present,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PullKind {
    Add,
    Change,
    Same,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PullRowDiff {
    pub row: u32,
    pub key: String,
    pub name: String,
    /// The task it is, or will be.
    pub uid: u32,
    pub kind: PullKind,
    /// What changes; empty unless `kind` is `Change`.
    pub fields: Vec<PullField>,
}

/// A linked task whose issue the source no longer has.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GoneTask {
    pub uid: u32,
    pub key: String,
    pub name: String,
}

/// The cell of a row a problem is in; `Row` for the row as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PullColumn {
    Row,
    Key,
    Name,
    Parent,
    Work,
    Assignee,
    Field(String),
    After,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PullError {
    /// The row, counting from 0; `None` for the pull as a whole.
    pub row: Option<u32>,
    pub key: String,
    pub column: PullColumn,
    pub error: EngineError,
}

impl PullError {
    pub fn message(&self, lang: Lang) -> String {
        self.error.message(lang)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PullDiff {
    pub rows: Vec<PullRowDiff>,
    pub gone: Vec<GoneTask>,
    pub errors: Vec<PullError>,
    /// Fields changed on both sides that no resolution settled (three-way pulls only).
    pub conflicts: Vec<PullConflict>,
    /// A base was updated without a task changing (a conflict resolved by keeping Sheafly,
    /// or both sides changed to the same value): worth recording on its own.
    pub bases_changed: bool,
}

/// A field that changed at the source and in Sheafly since the last sync, to different
/// values. Values are the canonical text of `sync.rs`; `None` is no value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PullConflict {
    pub row: u32,
    pub key: String,
    pub name: String,
    pub uid: u32,
    /// The field's key (`SyncField::key`).
    pub field: String,
    /// What the source held at the last sync.
    pub base: Option<String>,
    pub source: Option<String>,
    pub local: Option<String>,
}

impl PullDiff {
    /// Nothing to add, change or mark.
    pub fn is_empty(&self) -> bool {
        self.gone.is_empty() && !self.bases_changed && self.rows.iter().all(|row| row.kind == PullKind::Same)
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// What `ApplyPull` would do, without changing `project`.
pub fn diff(project: &Project, rows: &[PullTask], pulled_at: i64) -> PullDiff {
    diff_with(project, rows, pulled_at, None)
}

/// `diff`, three-way when `resolutions` is given (the conflicts it settles are applied; the
/// rest are listed in `conflicts`).
pub fn diff_with(
    project: &Project,
    rows: &[PullTask],
    pulled_at: i64,
    resolutions: Option<&[Resolution]>,
) -> PullDiff {
    let mut copy = project.clone();
    let mut result = match run(&mut copy, rows, pulled_at, resolutions) {
        Ok(result) => result,
        Err(error) => return whole(error),
    };
    if !result.has_errors() {
        // Links between issues can still loop in ways the rows alone do not show.
        if let Err(error) = schedule(&mut copy) {
            result.errors.push(whole_error(error));
        }
    }
    result
}

fn whole_error(error: EngineError) -> PullError {
    PullError {
        row: None,
        key: String::new(),
        column: PullColumn::Row,
        error,
    }
}

fn whole(error: EngineError) -> PullDiff {
    PullDiff {
        errors: vec![whole_error(error)],
        ..PullDiff::default()
    }
}

/// `ApplyPull`.
pub(crate) fn apply_pull(
    project: &mut Project,
    rows: &[PullTask],
    pulled_at: i64,
    resolutions: Option<&[Resolution]>,
) -> Result<(), EngineError> {
    let result = run(project, rows, pulled_at, resolutions)?;
    if result.has_errors() {
        return Err(Invalid::PullHasErrors.into());
    }
    if !result.conflicts.is_empty() {
        return Err(Invalid::PullHasConflicts.into());
    }
    if result.is_empty() {
        return Err(Invalid::NothingToPull.into());
    }
    Ok(())
}

type CellError = (PullColumn, EngineError);

/// The row after validation: what it asks for, with the people and fields resolved.
struct Checked {
    key: String,
    name: String,
    parent: Option<u32>,
    assignee: Option<u32>,
    /// The assignee as canonical text (`sync::person_text`).
    assignee_text: Option<String>,
    values: Vec<(u32, Option<FieldValue>)>,
}

struct State {
    /// Keys the pull can refer to: linked tasks of the project and the rows done so far.
    known: HashMap<String, u32>,
    people: HashMap<String, u32>,
    /// People of the project by lower-cased name; a name more than one person has is no match.
    names: HashMap<String, Vec<u32>>,
    seen: HashSet<String>,
    parents: HashSet<String>,
    assignments: Vec<(u32, u32)>,
    statuses: Vec<(u32, f64)>,
    result: PullDiff,
    /// Three-way: the user's choices; None is the legacy pull where the source wins.
    resolutions: Option<Vec<Resolution>>,
    /// The source's sync settings (the defaults without a source); only a three-way pull
    /// reads them, so the original pull replays as it was.
    sync: SyncSettings,
    /// Bases to write once every task has its final state.
    base_updates: Vec<(u32, Vec<BaseUpdate>)>,
}

/// One field of one task's base to write at the end of a three-way pull.
struct BaseUpdate {
    field: SyncField,
    /// What the source held (None: nothing).
    source: Option<String>,
    /// Record what the task holds then (false keeps the old, so a later change at the
    /// source still conflicts with a value the user chose to keep).
    refresh_local: bool,
}

/// Apply the rows to `project`, skipping what is wrong and listing it. The error is for a
/// problem with the pull as a whole.
fn run(
    project: &mut Project,
    rows: &[PullTask],
    pulled_at: i64,
    resolutions: Option<&[Resolution]>,
) -> Result<PullDiff, EngineError> {
    if rows.len() > MAX_PULL_ROWS {
        return Err(Invalid::TooManyPullRows.into());
    }
    normalize_outline(project);
    let mut known = HashMap::new();
    for task in &project.tasks {
        if let Some(link) = &task.external {
            known.entry(link.key.clone()).or_insert(task.uid);
        }
    }
    let mut state = State {
        known,
        people: project
            .resources
            .iter()
            .filter(|resource| !resource.email.is_empty())
            .map(|resource| (resource.email.to_lowercase(), resource.uid))
            .collect(),
        names: names_of(project),
        seen: HashSet::new(),
        parents: rows
            .iter()
            .filter_map(|row| row.parent_key.as_deref())
            .map(|key| key.trim().to_string())
            .collect(),
        assignments: Vec::new(),
        statuses: Vec::new(),
        result: PullDiff::default(),
        resolutions: resolutions.map(<[Resolution]>::to_vec),
        sync: project.source.as_ref().map(|source| source.sync.clone()).unwrap_or_default(),
        base_updates: Vec::new(),
    };
    let mut slots: Vec<Option<usize>> = vec![None; rows.len()];
    for (index, input) in rows.iter().enumerate() {
        let row = index as u32;
        let key = input.key.trim().to_string();
        match check(project, &mut state, input, &key) {
            Ok(checked) => {
                let diff_row = apply_row(project, &mut state, input, checked, row, pulled_at)?;
                slots[index] = Some(state.result.rows.len());
                state.result.rows.push(diff_row);
            }
            Err((column, error)) => state.result.errors.push(PullError {
                row: Some(row),
                key,
                column,
                error,
            }),
        }
    }
    for (task_uid, person) in std::mem::take(&mut state.assignments) {
        work::add_assignment(project, task_uid, person, 1.0);
    }
    add_links(project, &mut state, rows, &slots, pulled_at);
    if state.result.rows.iter().any(|row| row.kind == PullKind::Add) {
        // New tasks need their dates before progress is recorded on them.
        schedule(project)?;
    }
    for (uid, percent) in std::mem::take(&mut state.statuses) {
        apply_one(project, &Command::SetPercentComplete { uid, percent })?;
    }
    finalize_bases(project, std::mem::take(&mut state.base_updates));
    mark_gone(project, &mut state, rows, pulled_at);
    state.result.errors.sort_by_key(|error| error.row);
    Ok(state.result)
}

fn check(
    project: &Project,
    state: &mut State,
    input: &PullTask,
    key: &str,
) -> Result<Checked, CellError> {
    let fail = |column: PullColumn, reason: Invalid| Err((column, reason.into()));
    if key.is_empty() {
        return fail(PullColumn::Key, Invalid::PullKey);
    }
    if !state.seen.insert(key.to_string()) {
        return fail(PullColumn::Key, Invalid::PullKeyRepeated);
    }
    let name = input.name.trim();
    if name.is_empty() {
        return fail(PullColumn::Name, Invalid::PullName);
    }
    if input.work.is_some_and(|work| work < 0) {
        return fail(PullColumn::Work, Invalid::PullNegative);
    }
    let parent = match input.parent_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        None => None,
        Some(parent) => match state.known.get(parent) {
            Some(uid) if parent != key => Some(*uid),
            _ => return fail(PullColumn::Parent, Invalid::PullParent),
        },
    };
    let email = input.assignee_email.as_deref().map(str::trim).filter(|e| !e.is_empty());
    let named = input.assignee_name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let assignee = match (email, named) {
        (Some(email), _) => match state.people.get(&email.to_lowercase()) {
            Some(uid) => Some(*uid),
            None => return fail(PullColumn::Assignee, Invalid::PullUnknownAssignee),
        },
        (None, Some(name)) => match state.names.get(&name.to_lowercase()).map(Vec::as_slice) {
            Some([uid]) => Some(*uid),
            _ => return fail(PullColumn::Assignee, Invalid::PullUnknownAssignee),
        },
        (None, None) => None,
    };
    let assignee_text = assignee
        .and_then(|uid| project.resources.iter().find(|resource| resource.uid == uid))
        .map(sync::person_text);
    if input.after.iter().any(|after| after.trim() == key) {
        return fail(PullColumn::After, Invalid::PullLink);
    }
    let values = field_targets(project, &input.values)?;
    Ok(Checked {
        key: key.to_string(),
        name: name.to_string(),
        parent,
        assignee,
        assignee_text,
        values,
    })
}

/// The id and value each field of the row asks for; `None` clears it.
fn field_targets(
    project: &Project,
    input: &[(String, FieldValue)],
) -> Result<Vec<(u32, Option<FieldValue>)>, CellError> {
    let mut targets = Vec::new();
    for (name, value) in input {
        let column = || PullColumn::Field(name.clone());
        let lower = name.trim().to_lowercase();
        let field = project
            .fields
            .iter()
            .find(|field| field.name.to_lowercase() == lower)
            .ok_or_else(|| (column(), EngineError::from(Invalid::NoSuchField)))?;
        let value = fields::checked_value(&field.kind, value).map_err(|error| (column(), error))?;
        targets.push((field.id, value));
    }
    Ok(targets)
}

/// The instant falls on the day `pushed` (`YYYY-MM-DD`) names.
fn day_matches(instant: Option<i64>, pushed: Option<&str>) -> bool {
    match (instant, pushed) {
        (Some(instant), Some(pushed)) => date_key(civil(instant).0) == pushed,
        _ => false,
    }
}

/// The people of the project by lower-cased name.
fn names_of(project: &Project) -> HashMap<String, Vec<u32>> {
    let mut names: HashMap<String, Vec<u32>> = HashMap::new();
    for resource in &project.resources {
        names.entry(resource.name.trim().to_lowercase()).or_default().push(resource.uid);
    }
    names
}

fn percent_for(status: StatusClass, current: f64, in_progress: f64) -> f64 {
    match status {
        StatusClass::NotStarted => 0.0,
        StatusClass::Done => 100.0,
        StatusClass::InProgress if current > 0.0 && current < 100.0 => current,
        StatusClass::InProgress => in_progress,
    }
}

impl State {
    /// The percent an in-progress issue gets: the source's setting in a three-way pull, the
    /// original constant in the original pull.
    fn in_progress(&self) -> f64 {
        if self.resolutions.is_some() {
            f64::from(self.sync.in_progress_percent)
        } else {
            IN_PROGRESS_PERCENT
        }
    }

    /// Whether the field is read from the source: always in the original pull, by the
    /// source's direction in a three-way one.
    fn pulls(&self, field: SyncField) -> bool {
        self.resolutions.is_none() || self.sync.direction(&field.key()).pulls()
    }
}

fn apply_row(
    project: &mut Project,
    state: &mut State,
    input: &PullTask,
    checked: Checked,
    row: u32,
    pulled_at: i64,
) -> Result<PullRowDiff, EngineError> {
    let group = state.parents.contains(&checked.key);
    let existing = state.known.get(&checked.key).copied();
    let (uid, kind, fields) = match existing {
        Some(uid) => {
            let fields = if state.resolutions.is_some() {
                update_task_sync(project, state, input, &checked, (uid, row), group, pulled_at)?
            } else {
                update_task(project, state, input, &checked, uid, group, pulled_at)?
            };
            let kind = if fields.is_empty() { PullKind::Same } else { PullKind::Change };
            (uid, kind, fields)
        }
        None => (add_task(project, state, input, &checked, group, pulled_at), PullKind::Add, Vec::new()),
    };
    state.known.insert(checked.key.clone(), uid);
    Ok(PullRowDiff {
        row,
        key: checked.key,
        name: checked.name,
        uid,
        kind,
        fields,
    })
}

fn add_task(
    project: &mut Project,
    state: &mut State,
    input: &PullTask,
    checked: &Checked,
    group: bool,
    pulled_at: i64,
) -> u32 {
    let uid = project.alloc_task_uid();
    let (index, level) = match checked.parent.and_then(|parent| project.index_of(parent)) {
        Some(position) => (
            subtree_end(&project.tasks, position),
            project.tasks[position].outline_level + 1,
        ),
        None => (project.tasks.len(), 1),
    };
    let mut task = Task::new(uid, checked.name.clone());
    task.outline_level = level;
    task.start = project.start;
    task.finish = project.start;
    let (take_work, take_start, take_due, take_status, take_assignee) = (
        state.pulls(SyncField::Work),
        state.pulls(SyncField::Start),
        state.pulls(SyncField::Due),
        state.pulls(SyncField::Status),
        state.pulls(SyncField::Assignee),
    );
    if !group {
        if let Some(work) = input.work.filter(|_| take_work) {
            task.task_type = TaskType::FixedWork;
            task.effort_driven = true;
            task.work = work;
            task.remaining_work = work;
            // Until someone is on it, the task lasts as long as one person working full time.
            task.duration = work;
            task.remaining_duration = work;
        }
        if let Some(date) = input.start.filter(|_| take_start) {
            task.constraint_type = ConstraintType::Snet;
            task.constraint_date = Some(date);
        }
    }
    if take_due {
        task.deadline = input.due;
    }
    for (id, value) in &checked.values {
        if let Some(value) = value.as_ref().filter(|_| state.pulls(SyncField::Field(*id))) {
            task.values.insert(*id, value.clone());
        }
    }
    task.external = Some(ExternalLink {
        key: checked.key.clone(),
        synced_at: pulled_at,
        gone: false,
        pushed_start: None,
        pushed_due: None,
        base: None,
    });
    project.tasks.insert(index, task);
    if state.resolutions.is_some() {
        // A new task starts in agreement with the source on everything the row says.
        let updates = provided_fields(input, checked, group)
            .into_iter()
            .filter(|field| state.pulls(*field))
            .map(|field| BaseUpdate { field, source: source_value(input, checked, field).flatten(), refresh_local: true })
            .collect();
        state.base_updates.push((uid, updates));
    }
    if !group {
        if let Some(person) = checked.assignee.filter(|_| take_assignee) {
            state.assignments.push((uid, person));
        }
        if let Some(status) = input.status.filter(|_| take_status) {
            let percent = percent_for(status, 0.0, state.in_progress());
            if percent > 0.0 {
                state.statuses.push((uid, percent));
            }
        }
    }
    uid
}

/// Bring the linked task `uid` in line with the row; the fields that changed.
/// Leave `person` the only one on the task. Their own assignment stays as it is (units,
/// baselines, an actual cost entered); anyone else's goes, and they join at 1 unit when
/// they were not on it.
fn assign_only(project: &mut Project, uid: u32, person: u32) -> Result<(), EngineError> {
    let others: Vec<u32> = project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == uid && assignment.resource_uid != person)
        .map(|assignment| assignment.uid)
        .collect();
    let kept = project
        .assignments
        .iter()
        .any(|assignment| assignment.task_uid == uid && assignment.resource_uid == person);
    // The others go first, so a new person takes over the task's work.
    for assignment in others {
        apply_one(project, &Command::RemoveAssignment { uid: assignment })?;
    }
    if !kept {
        work::add_assignment(project, uid, person, 1.0);
    }
    Ok(())
}

fn update_task(
    project: &mut Project,
    state: &mut State,
    input: &PullTask,
    checked: &Checked,
    uid: u32,
    group: bool,
    pulled_at: i64,
) -> Result<Vec<PullField>, EngineError> {
    let mut changed = Vec::new();
    let group = group || project.task(uid).is_some_and(|task| task.summary);
    let task = project.task(uid).ok_or(EngineError::TaskNotFound)?.clone();
    if task.name != checked.name {
        apply_one(project, &Command::RenameTask { uid, name: checked.name.clone() })?;
        changed.push(PullField::Name);
    }
    if !group {
        if let Some(person) = checked.assignee {
            let current: Vec<u32> = project
                .assignments
                .iter()
                .filter(|assignment| assignment.task_uid == uid)
                .map(|assignment| assignment.resource_uid)
                .collect();
            if current != [person] {
                assign_only(project, uid, person)?;
                changed.push(PullField::Assignee);
            }
        }
        if let Some(minutes) = input.work.filter(|minutes| *minutes != project.task(uid).map_or(0, |t| t.work)) {
            apply_one(project, &Command::SetWork { uid, minutes })?;
            changed.push(PullField::Work);
        }
        let wanted = input.start.map(|date| (ConstraintType::Snet, Some(date)));
        let current = (task.constraint_type, task.constraint_date);
        // A start day equal to the one a push wrote is our own, not a change at the source.
        let own_start = day_matches(input.start, task.external.as_ref().and_then(|l| l.pushed_start.as_deref()));
        match wanted {
            _ if own_start => {}
            Some(wanted) if wanted != current => {
                apply_one(
                    project,
                    &Command::SetConstraint { uid, constraint_type: wanted.0, date: wanted.1 },
                )?;
                changed.push(PullField::Start);
            }
            None if task.constraint_type == ConstraintType::Snet => {
                apply_one(
                    project,
                    &Command::SetConstraint { uid, constraint_type: ConstraintType::Asap, date: None },
                )?;
                changed.push(PullField::Start);
            }
            _ => {}
        }
        if let Some(status) = input.status {
            let percent = percent_for(status, task.percent_complete, state.in_progress());
            if (percent - task.percent_complete).abs() > 1e-9 {
                state.statuses.push((uid, percent));
                changed.push(PullField::Status);
            }
        }
    }
    let own_due = day_matches(input.due, task.external.as_ref().and_then(|l| l.pushed_due.as_deref()));
    if !own_due && task.deadline != input.due {
        apply_one(project, &Command::SetDeadline { uid, date: input.due })?;
        changed.push(PullField::Due);
    }
    let mut fields_changed = false;
    for (id, value) in &checked.values {
        if task.values.get(id) != value.as_ref() {
            let command = Command::SetFieldValue { uid, field_id: *id, value: value.clone() };
            apply_one(project, &command)?;
            fields_changed = true;
        }
    }
    if fields_changed {
        changed.push(PullField::Fields);
    }
    if task.external.as_ref().is_some_and(|link| link.gone) {
        changed.push(PullField::Present);
    }
    if !changed.is_empty() {
        if let Some(link) = project.task_mut(uid).and_then(|task| task.external.as_mut()) {
            link.synced_at = pulled_at;
            link.gone = false;
        }
    }
    Ok(changed)
}

/// The fields the row says something about, for a task that takes them: a group takes no
/// effort, start, owner or progress.
fn provided_fields(input: &PullTask, checked: &Checked, group: bool) -> Vec<SyncField> {
    let mut fields = vec![SyncField::Name];
    if !group {
        if input.work.is_some() {
            fields.push(SyncField::Work);
        }
        fields.push(SyncField::Start);
        if input.status.is_some() {
            fields.push(SyncField::Status);
        }
        if checked.assignee.is_some() {
            fields.push(SyncField::Assignee);
        }
    }
    fields.push(SyncField::Due);
    fields.extend(checked.values.iter().map(|(id, _)| SyncField::Field(*id)));
    fields
}

/// What the row holds for `field` in canonical text; `None` when the row says nothing
/// about it, `Some(None)` when it says there is no value.
fn source_value(input: &PullTask, checked: &Checked, field: SyncField) -> Option<Option<String>> {
    match field {
        SyncField::Name => Some(Some(checked.name.clone())),
        SyncField::Work => input.work.map(|work| Some(work.to_string())),
        SyncField::Start => Some(input.start.map(sync::day_text)),
        SyncField::Due => Some(input.due.map(sync::day_text)),
        SyncField::Status => input.status.map(|status| Some(sync::status_text(status).to_string())),
        SyncField::Assignee => checked.assignee_text.clone().map(Some),
        SyncField::Field(id) => checked
            .values
            .iter()
            .find(|(field_id, _)| *field_id == id)
            .map(|(_, value)| value.as_ref().map(sync::field_text)),
    }
}

fn pull_field(field: SyncField) -> PullField {
    match field {
        SyncField::Name => PullField::Name,
        SyncField::Work => PullField::Work,
        SyncField::Start => PullField::Start,
        SyncField::Due => PullField::Due,
        SyncField::Status => PullField::Status,
        SyncField::Assignee => PullField::Assignee,
        SyncField::Field(_) => PullField::Fields,
    }
}

/// Put the source's value of `field` on the task.
fn apply_field(
    project: &mut Project,
    state: &mut State,
    task: &Task,
    field: SyncField,
    input: &PullTask,
    checked: &Checked,
) -> Result<(), EngineError> {
    let uid = task.uid;
    match field {
        SyncField::Name => {
            apply_one(project, &Command::RenameTask { uid, name: checked.name.clone() })?;
        }
        SyncField::Work => {
            if let Some(minutes) = input.work {
                apply_one(project, &Command::SetWork { uid, minutes })?;
            }
        }
        SyncField::Start => match input.start {
            Some(date) => apply_one(
                project,
                &Command::SetConstraint { uid, constraint_type: ConstraintType::Snet, date: Some(date) },
            )?,
            None if task.constraint_type == ConstraintType::Snet => apply_one(
                project,
                &Command::SetConstraint { uid, constraint_type: ConstraintType::Asap, date: None },
            )?,
            None => {}
        },
        SyncField::Due => apply_one(project, &Command::SetDeadline { uid, date: input.due })?,
        SyncField::Status => {
            if let Some(status) = input.status {
                state.statuses.push((uid, percent_for(status, task.percent_complete, state.in_progress())));
            }
        }
        SyncField::Assignee => {
            if let Some(person) = checked.assignee {
                assign_only(project, uid, person)?;
            }
        }
        SyncField::Field(id) => {
            if let Some((_, value)) = checked.values.iter().find(|(field_id, _)| *field_id == id) {
                apply_one(project, &Command::SetFieldValue { uid, field_id: id, value: value.clone() })?;
            }
        }
    }
    Ok(())
}

/// The three-way version of `update_task` (`sync.rs`): a field moves only when the source
/// changed it and Sheafly did not (or the user chose the source). Returns the fields that
/// changed on the task.
fn update_task_sync(
    project: &mut Project,
    state: &mut State,
    input: &PullTask,
    checked: &Checked,
    (uid, row): (u32, u32),
    group: bool,
    pulled_at: i64,
) -> Result<Vec<PullField>, EngineError> {
    let task = project.task(uid).ok_or(EngineError::TaskNotFound)?.clone();
    let group = group || task.summary;
    let link = task.external.clone().ok_or(EngineError::TaskNotFound)?;
    let mut changed: Vec<PullField> = Vec::new();
    let mut updates = Vec::new();
    for field in provided_fields(input, checked, group) {
        if !state.pulls(field) {
            continue;
        }
        let Some(source) = source_value(input, checked, field) else { continue };
        let local = sync::local_value(project, &task, field);
        let pushed = match field {
            SyncField::Start => link.pushed_start.as_deref(),
            SyncField::Due => link.pushed_due.as_deref(),
            _ => None,
        };
        let (apply, refresh_local) = match sync::decide(link.base.as_ref(), field, &source, &local, pushed) {
            Verdict::Keep => continue,
            Verdict::Agree { real } => {
                state.result.bases_changed |= real;
                (false, true)
            }
            Verdict::Take => (true, true),
            Verdict::Conflict => {
                let key = field.key();
                // The user's own choice first; else what the source's settings say to do.
                let choice = state
                    .resolutions
                    .iter()
                    .flatten()
                    .find(|resolution| resolution.key == checked.key && resolution.field == key)
                    .map(|resolution| resolution.keep)
                    .or(match state.sync.on_conflict(&key) {
                        OnConflict::Ask => None,
                        OnConflict::Source => Some(Side::Source),
                        OnConflict::Sheafly => Some(Side::Sheafly),
                    });
                match choice {
                    Some(Side::Source) => (true, true),
                    Some(Side::Sheafly) => {
                        state.result.bases_changed = true;
                        (false, false)
                    }
                    None => {
                        state.result.conflicts.push(PullConflict {
                            row,
                            key: checked.key.clone(),
                            name: checked.name.clone(),
                            uid,
                            field: key.clone(),
                            base: link.base.as_ref().and_then(|base| base.source.get(&key).cloned()),
                            source,
                            local,
                        });
                        continue;
                    }
                }
            }
        };
        if apply {
            apply_field(project, state, &task, field, input, checked)?;
            let shown = pull_field(field);
            if !changed.contains(&shown) {
                changed.push(shown);
            }
        }
        updates.push(BaseUpdate { field, source, refresh_local });
    }
    if !updates.is_empty() {
        state.base_updates.push((uid, updates));
    }
    if link.gone {
        changed.push(PullField::Present);
    }
    if !changed.is_empty() {
        if let Some(link) = project.task_mut(uid).and_then(|task| task.external.as_mut()) {
            link.synced_at = pulled_at;
            link.gone = false;
        }
    }
    Ok(changed)
}

/// Write the bases of a three-way pull, now that every task has its final state.
fn finalize_bases(project: &mut Project, updates: Vec<(u32, Vec<BaseUpdate>)>) {
    for (uid, list) in updates {
        let Some(task) = project.task(uid).cloned() else { continue };
        let locals: Vec<Option<String>> = list
            .iter()
            .map(|update| update.refresh_local.then(|| sync::local_value(project, &task, update.field)).flatten())
            .collect();
        let Some(link) = project.task_mut(uid).and_then(|task| task.external.as_mut()) else { continue };
        let base = sync::base_of(link);
        for (update, local) in list.iter().zip(locals) {
            let key = update.field.key();
            sync::set(&mut base.source, &key, update.source.clone());
            if update.refresh_local {
                sync::set(&mut base.local, &key, local);
            }
        }
    }
}

/// Finish-to-start links from `after`, once every row has its task. An issue the pull
/// does not have is an error on the row that waits for it.
fn add_links(
    project: &mut Project,
    state: &mut State,
    rows: &[PullTask],
    slots: &[Option<usize>],
    pulled_at: i64,
) {
    for (index, input) in rows.iter().enumerate() {
        let Some(slot) = slots[index] else { continue };
        let successor = state.result.rows[slot].uid;
        let mut added = false;
        for after in &input.after {
            let Some(predecessor) = state.known.get(after.trim()).copied() else {
                state.result.errors.push(PullError {
                    row: Some(index as u32),
                    key: state.result.rows[slot].key.clone(),
                    column: PullColumn::After,
                    error: Invalid::PullLink.into(),
                });
                break;
            };
            let exists = project
                .dependencies
                .iter()
                .any(|dep| dep.predecessor_uid == predecessor && dep.successor_uid == successor);
            if !exists {
                let uid = project.alloc_dependency_uid();
                project.dependencies.push(Dependency {
                    uid,
                    predecessor_uid: predecessor,
                    successor_uid: successor,
                    kind: DepKind::Fs,
                    lag: 0,
                });
                added = true;
            }
        }
        if added {
            let row = &mut state.result.rows[slot];
            if row.kind == PullKind::Same {
                row.kind = PullKind::Change;
                if let Some(link) = project.task_mut(successor).and_then(|t| t.external.as_mut()) {
                    link.synced_at = pulled_at;
                }
            }
            if row.kind != PullKind::Add {
                row.fields.push(PullField::Links);
            }
        }
    }
}

/// Mark the linked tasks the rows no longer have.
fn mark_gone(project: &mut Project, state: &mut State, rows: &[PullTask], pulled_at: i64) {
    let present: HashSet<&str> = rows.iter().map(|row| row.key.trim()).collect();
    for task in &mut project.tasks {
        let Some(link) = task.external.as_mut() else { continue };
        if link.gone || present.contains(link.key.as_str()) {
            continue;
        }
        link.gone = true;
        link.synced_at = pulled_at;
        state.result.gone.push(GoneTask {
            uid: task.uid,
            key: link.key.clone(),
            name: task.name.clone(),
        });
    }
}
