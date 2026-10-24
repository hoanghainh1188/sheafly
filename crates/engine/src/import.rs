//! Importing a table of people and tasks (phase 18 P3, step 3a).
//!
//! The UI reads a CSV or spreadsheet, maps its columns to fields and converts each cell
//! (dates, numbers, units) the way the rest of the UI reads them. What reaches the engine
//! is already typed: `Command::ImportRows` holds the rows themselves, like `PasteTasks`
//! holds its tasks, so replay never needs the file. The whole import is one command and
//! so one undo step, and the project is scheduled once at the end.
//!
//! `build` adds the rows one by one and keeps going past a bad one, collecting every
//! problem with the row and column it is in. `check_import` runs it on a copy for the
//! preview; the command runs it for real and refuses when anything was wrong, so a
//! refused import changes nothing.
//!
//! People are matched by name, letter case aside. A person the import lists who is
//! already in the project is left as they are; a task naming someone the project does
//! not have (nor the import) is an error, not a new person, so a typo cannot make one.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::command::subtree_end;
use crate::error::{EngineError, Invalid};
use crate::fields::{self, FieldValue};
use crate::lang::Lang;
use crate::model::{ConstraintType, Dependency, DepKind, Project, Resource, ResourceKind, Task, TaskType};
use crate::schedule::{normalize_outline, schedule};
use crate::skill_levels;
use crate::skills;
use crate::work;

/// Rows an import can have, people and tasks together.
pub const MAX_IMPORT_ROWS: usize = 20_000;

/// One person or machine to add.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportResource {
    pub name: String,
    pub kind: ResourceKind,
    /// Empty keeps the default (1, full time).
    #[serde(default)]
    pub max_units: Option<f64>,
    #[serde(default)]
    pub standard_rate: Option<f64>,
    /// Skill names, each with an optional level as `Skill:Level`; skills and levels the
    /// project lacks are added to it (a new level at the top of the scale).
    #[serde(default)]
    pub skills: Vec<String>,
    /// Role or team; empty for none.
    #[serde(default)]
    pub group: String,
}

impl Default for ImportResource {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: ResourceKind::Person,
            max_units: None,
            standard_rate: None,
            skills: Vec::new(),
            group: String::new(),
        }
    }
}

/// One task to add, in outline order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImportTask {
    pub name: String,
    /// Outline level within the import: 0 for the outermost rows, at most one deeper than
    /// the row above. A row followed by a deeper one is a group.
    #[serde(default)]
    pub level: u32,
    /// Effort in working minutes. A task with effort is Fixed Work: its duration follows
    /// from the effort and the units of the people on it.
    #[serde(default)]
    pub work: Option<i64>,
    /// The effort range of the finish forecast: both or neither.
    #[serde(default)]
    pub work_low: Option<i64>,
    #[serde(default)]
    pub work_high: Option<i64>,
    /// Duration in working minutes, for a task without effort.
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub deadline: Option<i64>,
    /// Start no earlier than this instant.
    #[serde(default)]
    pub start_no_earlier: Option<i64>,
    /// Custom field values by field name, letter case aside.
    #[serde(default)]
    pub values: Vec<(String, FieldValue)>,
    #[serde(default)]
    pub assignees: Vec<ImportAssignee>,
    /// Rows of this import (0 for the first task row) this one waits for (finish to start).
    #[serde(default)]
    pub after: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImportAssignee {
    pub name: String,
    pub units: f64,
}

/// Which list of the import a problem is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportTable {
    Resources,
    Tasks,
}

/// The cell a problem is in; `Row` for the row as a whole (or the whole import).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportColumn {
    Row,
    Name,
    Kind,
    MaxUnits,
    Rate,
    Skills,
    Group,
    Level,
    Work,
    WorkLow,
    WorkHigh,
    Duration,
    Priority,
    Deadline,
    StartNoEarlier,
    /// A custom field, by the name the import gave.
    Field(String),
    Assignees,
    After,
}

/// What is wrong, and where. `row` counts from 0 within its table; `None` is the import
/// as a whole.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RowError {
    pub table: ImportTable,
    pub row: Option<u32>,
    pub column: ImportColumn,
    pub error: EngineError,
}

impl RowError {
    pub fn message(&self, lang: Lang) -> String {
        self.error.message(lang)
    }
}

/// Every problem the import would have, without changing `project`; empty when
/// `ImportRows` would succeed.
pub fn check_import(
    project: &Project,
    after_uid: Option<u32>,
    resources: &[ImportResource],
    tasks: &[ImportTask],
) -> Vec<RowError> {
    let mut copy = project.clone();
    let mut errors = match build(&mut copy, after_uid, resources, tasks) {
        Ok(errors) => errors,
        Err(error) => return vec![whole(table_of(resources, tasks), error)],
    };
    if errors.is_empty() {
        // Links through groups can still loop in ways the rows alone do not show.
        if let Err(error) = schedule(&mut copy) {
            errors.push(whole(ImportTable::Tasks, error));
        }
    }
    errors
}

/// `ImportRows`: add everything or nothing.
pub(crate) fn import_rows(
    project: &mut Project,
    after_uid: Option<u32>,
    resources: &[ImportResource],
    tasks: &[ImportTask],
) -> Result<(), EngineError> {
    if resources.is_empty() && tasks.is_empty() {
        return Err(Invalid::NothingToImport.into());
    }
    if build(project, after_uid, resources, tasks)?.is_empty() {
        Ok(())
    } else {
        Err(Invalid::ImportHasErrors.into())
    }
}

fn table_of(resources: &[ImportResource], tasks: &[ImportTask]) -> ImportTable {
    if tasks.is_empty() && !resources.is_empty() {
        ImportTable::Resources
    } else {
        ImportTable::Tasks
    }
}

fn whole(table: ImportTable, error: EngineError) -> RowError {
    RowError {
        table,
        row: None,
        column: ImportColumn::Row,
        error,
    }
}

/// Add the rows to `project`, skipping what is wrong and listing it. The error is for a
/// problem with the import as a whole (an anchor that is gone).
fn build(
    project: &mut Project,
    after_uid: Option<u32>,
    resources: &[ImportResource],
    tasks: &[ImportTask],
) -> Result<Vec<RowError>, EngineError> {
    if resources.len() + tasks.len() > MAX_IMPORT_ROWS {
        return Ok(vec![whole(table_of(resources, tasks), Invalid::TooManyImportRows.into())]);
    }
    let mut errors = Vec::new();
    add_resources(project, resources, &mut errors);
    add_tasks(project, after_uid, tasks, &mut errors)?;
    Ok(errors)
}

fn row_error(table: ImportTable, row: usize, column: ImportColumn, error: impl Into<EngineError>) -> RowError {
    RowError {
        table,
        row: Some(row as u32),
        column,
        error: error.into(),
    }
}

fn add_resources(project: &mut Project, rows: &[ImportResource], errors: &mut Vec<RowError>) {
    let table = ImportTable::Resources;
    let mut seen: Vec<String> = Vec::new();
    for (row, input) in rows.iter().enumerate() {
        let name = input.name.trim();
        let key = name.to_lowercase();
        if name.is_empty() {
            errors.push(row_error(table, row, ImportColumn::Name, Invalid::ImportName));
            continue;
        }
        if seen.contains(&key) {
            errors.push(row_error(table, row, ImportColumn::Name, Invalid::ImportNameRepeated));
            continue;
        }
        seen.push(key.clone());
        if project.resources.iter().any(|resource| resource.name.trim().to_lowercase() == key) {
            continue;
        }
        if let Some(units) = input.max_units {
            if !units.is_finite() || units <= 0.0 {
                errors.push(row_error(table, row, ImportColumn::MaxUnits, Invalid::UnitsNotPositive));
                continue;
            }
        }
        if let Some(rate) = input.standard_rate {
            if !rate.is_finite() || rate < 0.0 {
                errors.push(row_error(table, row, ImportColumn::Rate, Invalid::ImportRate));
                continue;
            }
        }
        let group = match skill_levels::checked_group(&input.group) {
            Ok(group) => group,
            Err(error) => {
                errors.push(row_error(table, row, ImportColumn::Group, error));
                continue;
            }
        };
        let (skill_ids, levels) = match skill_ids(project, &input.skills) {
            Ok(ids) => ids,
            Err(error) => {
                errors.push(row_error(table, row, ImportColumn::Skills, error));
                continue;
            }
        };
        let uid = project.alloc_resource_uid();
        let mut resource = Resource::new(uid, name, input.kind);
        resource.max_units = input.max_units.unwrap_or(resource.max_units);
        resource.standard_rate = input.standard_rate.unwrap_or(resource.standard_rate);
        resource.skills = skill_ids;
        resource.skill_levels = levels;
        resource.group = group;
        project.resources.push(resource);
    }
}

/// The ids of the skills `names` (`Skill` or `Skill:Level`) and their levels, adding the
/// skills and levels the project lacks.
fn skill_ids(
    project: &mut Project,
    names: &[String],
) -> Result<(Vec<u32>, BTreeMap<u32, u32>), EngineError> {
    let mut ids = Vec::new();
    let mut levels = BTreeMap::new();
    for text in names {
        let (name, level) = skill_levels::split_level(text);
        let lower = name.to_lowercase();
        let existing = project
            .skills
            .iter()
            .find(|skill| skill.name.to_lowercase() == lower)
            .map(|skill| skill.id);
        let id = match existing {
            Some(id) => id,
            None => skills::add_skill(project, name)?,
        };
        ids.push(id);
        if let Some(level) = level {
            levels.insert(id, skill_levels::level_id(project, level)?);
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok((ids, levels))
}

fn add_tasks(
    project: &mut Project,
    after_uid: Option<u32>,
    rows: &[ImportTask],
    errors: &mut Vec<RowError>,
) -> Result<(), EngineError> {
    if rows.is_empty() {
        return Ok(());
    }
    normalize_outline(project);
    let (mut index, base) = anchor(project, after_uid)?;
    let people: HashMap<String, u32> = project
        .resources
        .iter()
        .map(|resource| (resource.name.trim().to_lowercase(), resource.uid))
        .collect();
    let mut uids = Vec::with_capacity(rows.len());
    let mut assignments: Vec<(u32, u32, f64)> = Vec::new();
    for (row, input) in rows.iter().enumerate() {
        let next_level = rows.get(row + 1).map(|next| next.level);
        let previous_level = row.checked_sub(1).map(|previous| rows[previous].level);
        let uid = project.alloc_task_uid();
        uids.push(uid);
        let task = match task_from(project, uid, input, base, previous_level, next_level) {
            Ok(task) => task,
            Err((column, error)) => {
                errors.push(row_error(ImportTable::Tasks, row, column, error));
                continue;
            }
        };
        let mut found = Vec::new();
        for assignee in &input.assignees {
            if !assignee.units.is_finite() || assignee.units <= 0.0 {
                found.push(Err(Invalid::UnitsNotPositive));
            } else if let Some(resource) = people.get(&assignee.name.trim().to_lowercase()) {
                found.push(Ok((uid, *resource, assignee.units)));
            } else {
                found.push(Err(Invalid::ImportUnknownPerson));
            }
        }
        if let Some(Err(reason)) = found.iter().find(|item| item.is_err()) {
            errors.push(row_error(ImportTable::Tasks, row, ImportColumn::Assignees, *reason));
            continue;
        }
        assignments.extend(found.into_iter().flatten());
        project.tasks.insert(index, task);
        index += 1;
    }
    for (task_uid, resource_uid, units) in assignments {
        work::add_assignment(project, task_uid, resource_uid, units);
    }
    add_links(project, rows, &uids, errors);
    Ok(())
}

/// Where the rows go and the outline level of the outermost ones.
fn anchor(project: &Project, after_uid: Option<u32>) -> Result<(usize, u32), EngineError> {
    match after_uid {
        Some(uid) => {
            let anchor = project.task(uid).ok_or(EngineError::TaskNotFound)?;
            let anchor = anchor.series_uid.unwrap_or(uid);
            let position = project.index_of(anchor).ok_or(EngineError::TaskNotFound)?;
            Ok((subtree_end(&project.tasks, position), project.tasks[position].outline_level))
        }
        // At the very end and at the top level, so the rows join no group or series.
        None => Ok((project.tasks.len(), 1)),
    }
}

type CellError = (ImportColumn, EngineError);

fn task_from(
    project: &Project,
    uid: u32,
    input: &ImportTask,
    base: u32,
    previous_level: Option<u32>,
    next_level: Option<u32>,
) -> Result<Task, CellError> {
    let fail = |column: ImportColumn, reason: Invalid| Err((column, reason.into()));
    let name = input.name.trim();
    if name.is_empty() {
        return fail(ImportColumn::Name, Invalid::ImportName);
    }
    if input.level > previous_level.map_or(0, |level| level.saturating_add(1)) {
        return fail(ImportColumn::Level, Invalid::ImportLevel);
    }
    let group = next_level.is_some_and(|next| next > input.level);
    if input.work.is_some_and(|work| work < 0) {
        return fail(ImportColumn::Work, Invalid::ImportNegative);
    }
    if input.duration.is_some_and(|duration| duration < 0) {
        return fail(ImportColumn::Duration, Invalid::ImportNegative);
    }
    if input.work.is_some() && input.duration.is_some() {
        return fail(ImportColumn::Duration, Invalid::ImportWorkAndDuration);
    }
    if group && input.work.is_some() {
        return fail(ImportColumn::Work, Invalid::ImportGroupWork);
    }
    if group && input.duration.is_some() {
        return fail(ImportColumn::Duration, Invalid::ImportGroupWork);
    }
    if group && !input.assignees.is_empty() {
        return fail(ImportColumn::Assignees, Invalid::ImportGroupWork);
    }
    let has_range = input.work_low.is_some() || input.work_high.is_some();
    if group && has_range {
        return fail(ImportColumn::WorkLow, Invalid::ImportGroupWork);
    }
    match (input.work_low, input.work_high) {
        (None, None) => {}
        (Some(low), Some(high)) if 0 <= low && low <= high => {}
        _ => return fail(ImportColumn::WorkHigh, Invalid::BadWorkRange),
    }
    if input.priority.is_some_and(|priority| !(0..=1000).contains(&priority)) {
        return fail(ImportColumn::Priority, Invalid::ImportPriority);
    }
    let mut task = Task::new(uid, name);
    task.outline_level = base + input.level;
    task.start = project.start;
    task.finish = project.start;
    if let Some(work) = input.work {
        task.task_type = TaskType::FixedWork;
        task.effort_driven = true;
        task.work = work;
        task.remaining_work = work;
        // Until someone is on it, the task lasts as long as one person working full time.
        task.duration = work;
        task.remaining_duration = work;
    } else if let Some(duration) = input.duration {
        task.duration = duration;
        task.remaining_duration = duration;
    }
    if let Some(priority) = input.priority {
        task.priority = priority;
    }
    task.deadline = input.deadline;
    task.work_low = input.work_low;
    task.work_high = input.work_high;
    if let Some(date) = input.start_no_earlier {
        task.constraint_type = ConstraintType::Snet;
        task.constraint_date = Some(date);
    }
    task.values = values(project, &input.values)?;
    Ok(task)
}

fn values(project: &Project, input: &[(String, FieldValue)]) -> Result<BTreeMap<u32, FieldValue>, CellError> {
    let mut values = BTreeMap::new();
    for (name, value) in input {
        let column = || ImportColumn::Field(name.clone());
        let lower = name.trim().to_lowercase();
        let field = project
            .fields
            .iter()
            .find(|field| field.name.to_lowercase() == lower)
            .ok_or_else(|| (column(), EngineError::from(Invalid::NoSuchField)))?;
        if let Some(value) = fields::checked_value(&field.kind, value).map_err(|error| (column(), error))? {
            values.insert(field.id, value);
        }
    }
    Ok(values)
}

/// Finish-to-start links from `after`. A row pointing at itself or past the import is an
/// error, and so is each row of a loop.
fn add_links(project: &mut Project, rows: &[ImportTask], uids: &[u32], errors: &mut Vec<RowError>) {
    let mut bad = vec![false; rows.len()];
    for (row, input) in rows.iter().enumerate() {
        if input.after.iter().any(|&from| from as usize >= rows.len() || from as usize == row) {
            bad[row] = true;
        }
    }
    let looped = rows_in_loops(rows, &bad);
    for (row, input) in rows.iter().enumerate() {
        if bad[row] || looped[row] {
            let reason = if bad[row] { Invalid::ImportLinkRow } else { Invalid::ImportLinkLoop };
            errors.push(row_error(ImportTable::Tasks, row, ImportColumn::After, reason));
            continue;
        }
        let mut froms = input.after.clone();
        froms.sort_unstable();
        froms.dedup();
        for from in froms {
            let dep_uid = project.alloc_dependency_uid();
            project.dependencies.push(Dependency {
                uid: dep_uid,
                predecessor_uid: uids[from as usize],
                successor_uid: uids[row],
                kind: DepKind::Fs,
                lag: 0,
            });
        }
    }
    errors.sort_by_key(|error| (error.table == ImportTable::Tasks, error.row));
}

/// Rows on a cycle of `after` links, leaving out the links `bad` rows hold.
fn rows_in_loops(rows: &[ImportTask], bad: &[bool]) -> Vec<bool> {
    // Tarjan's strongly connected components, iterative so a long chain cannot overflow.
    let count = rows.len();
    let edges = |row: usize| -> Vec<usize> {
        if bad[row] {
            Vec::new()
        } else {
            rows[row].after.iter().map(|&from| from as usize).collect()
        }
    };
    let mut index = vec![usize::MAX; count];
    // `low` starts equal to `index` for each node as it is reached.
    let mut low = vec![0; count];
    let mut on_stack = vec![false; count];
    let mut stack = Vec::new();
    let mut looped = vec![false; count];
    let mut next = 0;
    for start in 0..count {
        if index[start] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, Vec<usize>, usize)> = vec![(start, edges(start), 0)];
        index[start] = next;
        low[start] = next;
        next += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some((node, targets, position)) = work.last_mut() {
            let node = *node;
            if let Some(&target) = targets.get(*position) {
                *position += 1;
                if index[target] == usize::MAX {
                    index[target] = next;
                    low[target] = next;
                    next += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    work.push((target, edges(target), 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(index[target]);
                }
                continue;
            }
            work.pop();
            if let Some((parent, _, _)) = work.last() {
                low[*parent] = low[*parent].min(low[node]);
            }
            if low[node] == index[node] {
                let mut members = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    members.push(member);
                    if member == node {
                        break;
                    }
                }
                if members.len() > 1 {
                    for member in members {
                        looped[member] = true;
                    }
                }
            }
        }
    }
    looped
}
