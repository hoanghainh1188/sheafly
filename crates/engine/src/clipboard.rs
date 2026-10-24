//! Copy and paste of tasks, and fill-down, like Microsoft Project's Cmd+C / Cmd+V and Ctrl+D.
//!
//! A copy holds the task data itself, not the UIDs it came from, so `PasteTasks` replays
//! the same way after the originals are edited or deleted. Copied: name, relative outline
//! level, duration, manual dates, notes, priority, calendar, constraint, deadline, task
//! type, effort-driven, fixed cost, splits, assignments (resource, units, work) and the
//! links whose two ends were both copied. Not copied: progress and actuals, baselines,
//! leveling delay, and recurrence (a copied series becomes plain tasks).

use serde::{Deserialize, Serialize};

use crate::command::{apply_one, subtree_end, Command};
use crate::error::{EngineError, Invalid};
use crate::fields::{self, FieldValue};
use crate::model::{
    Accrual, Assignment, ConstraintType, DepKind, Dependency, Project, Task, TaskType,
};
use crate::schedule::normalize_outline;
use crate::split::{self, Split};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskCopy {
    pub name: String,
    /// Outline level relative to the shallowest copied task (0 for that one).
    pub level: u32,
    pub milestone: bool,
    pub manual: bool,
    pub duration: i64,
    pub work: i64,
    /// Kept for manual tasks, which hold the dates the user pinned.
    pub start: i64,
    pub finish: i64,
    pub notes: String,
    pub priority: i32,
    pub calendar_id: Option<u32>,
    /// Name of that calendar: a paste into another project matches calendars by name.
    #[serde(default)]
    pub calendar_name: Option<String>,
    pub constraint_type: ConstraintType,
    pub constraint_date: Option<i64>,
    pub deadline: Option<i64>,
    pub task_type: TaskType,
    pub effort_driven: bool,
    pub fixed_cost: f64,
    pub fixed_cost_accrual: Accrual,
    pub splits: Vec<Split>,
    /// Urgent (`Task::interrupts`).
    #[serde(default)]
    pub interrupts: bool,
    /// The effort range of the finish forecast (`Task::work_low`, `Task::work_high`).
    #[serde(default)]
    pub work_low: Option<i64>,
    #[serde(default)]
    pub work_high: Option<i64>,
    pub assignments: Vec<AssignmentCopy>,
    /// Custom field values by field name: a paste into another project keeps those whose
    /// field has the same name there and can hold the value.
    #[serde(default)]
    pub values: Vec<(String, FieldValue)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentCopy {
    pub resource_uid: u32,
    /// A paste into another project matches resources by name.
    #[serde(default)]
    pub resource_name: String,
    pub units: f64,
    pub work: i64,
}

/// A link between two copied tasks, by their positions in the copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCopy {
    pub from: u32,
    pub to: u32,
    pub kind: DepKind,
    pub lag: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskClipboard {
    pub tasks: Vec<TaskCopy>,
    pub links: Vec<LinkCopy>,
}

/// The field fill-down copies from the first selected row to the rows below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillField {
    Name,
    Duration,
    Start,
    Predecessors,
    PercentComplete,
    Calendar,
    /// A custom field, by id.
    Field(u32),
    /// The interrupt flag (`Task::interrupts`); summaries and milestones are skipped.
    Interrupts,
    /// The effort range of the finish forecast, both ends; summaries and milestones are
    /// skipped.
    WorkRange,
}

/// Copy the tasks `uids` with their subtrees, in outline order.
pub fn copy_tasks(project: &Project, uids: &[u32]) -> TaskClipboard {
    let mut picked = vec![false; project.tasks.len()];
    for uid in uids {
        if let Some(index) = project.index_of(*uid) {
            let end = subtree_end(&project.tasks, index);
            picked[index..end].iter_mut().for_each(|flag| *flag = true);
        }
    }
    let chosen: Vec<&Task> = project
        .tasks
        .iter()
        .zip(&picked)
        .filter_map(|(task, flag)| flag.then_some(task))
        .collect();
    let base = chosen
        .iter()
        .map(|task| task.outline_level)
        .min()
        .unwrap_or(1);
    let tasks = chosen
        .iter()
        .map(|task| task_copy(project, task, base))
        .collect();
    let position = |uid: u32| chosen.iter().position(|task| task.uid == uid);
    let links = project
        .dependencies
        .iter()
        .filter_map(|dep| {
            Some(LinkCopy {
                from: position(dep.predecessor_uid)? as u32,
                to: position(dep.successor_uid)? as u32,
                kind: dep.kind,
                lag: dep.lag,
            })
        })
        .collect();
    TaskClipboard { tasks, links }
}

fn task_copy(project: &Project, task: &Task, base: u32) -> TaskCopy {
    TaskCopy {
        name: task.name.clone(),
        level: task.outline_level.saturating_sub(base),
        milestone: task.milestone,
        manual: task.manual,
        duration: task.duration,
        work: task.work,
        start: task.start,
        finish: task.finish,
        notes: task.notes.clone(),
        priority: task.priority,
        calendar_id: task.calendar_id,
        calendar_name: task
            .calendar_id
            .and_then(|id| project.calendars.iter().find(|calendar| calendar.id == id))
            .map(|calendar| calendar.name.clone()),
        constraint_type: task.constraint_type,
        constraint_date: task.constraint_date,
        deadline: task.deadline,
        task_type: task.task_type,
        effort_driven: task.effort_driven,
        fixed_cost: task.fixed_cost,
        fixed_cost_accrual: task.fixed_cost_accrual,
        // Leveling's own splits stay behind: they belong to that run, not to the task.
        splits: task.splits.iter().filter(|split| !split.leveling).cloned().collect(),
        interrupts: task.interrupts,
        work_low: task.work_low,
        work_high: task.work_high,
        assignments: project
            .assignments
            .iter()
            .filter(|assignment| assignment.task_uid == task.uid)
            .map(|assignment| AssignmentCopy {
                resource_uid: assignment.resource_uid,
                resource_name: project
                    .resources
                    .iter()
                    .find(|resource| resource.uid == assignment.resource_uid)
                    .map(|resource| resource.name.clone())
                    .unwrap_or_default(),
                units: assignment.units,
                work: assignment.work,
            })
            .collect(),
        values: project
            .fields
            .iter()
            .filter_map(|field| Some((field.name.clone(), task.values.get(&field.id)?.clone())))
            .collect(),
    }
}

/// Insert the copies after `after_uid` and its subtree (at the end when `None`), at its level.
/// After an occurrence of a recurring task they go after the whole series, so they do not
/// join it. Resources and calendars are matched by UID and name, then by name alone (a
/// paste into another project); one that is not found is left out. Links between copies
/// are kept.
pub fn paste_tasks(
    project: &mut Project,
    after_uid: Option<u32>,
    clipboard: &TaskClipboard,
) -> Result<(), EngineError> {
    if clipboard.tasks.is_empty() {
        return Err(Invalid::NothingToPaste.into());
    }
    normalize_outline(project);
    let (mut index, level) = match after_uid {
        Some(uid) => {
            let anchor = project.task(uid).ok_or(EngineError::TaskNotFound)?;
            let anchor = anchor.series_uid.unwrap_or(uid);
            let position = project.index_of(anchor).ok_or(EngineError::TaskNotFound)?;
            let level = project.tasks[position].outline_level;
            (subtree_end(&project.tasks, position), level)
        }
        None => (
            project.tasks.len(),
            project.tasks.last().map_or(1, |task| task.outline_level),
        ),
    };
    let mut uids = Vec::with_capacity(clipboard.tasks.len());
    let mut previous_level = level.saturating_sub(1);
    for copy in &clipboard.tasks {
        let uid = project.alloc_task_uid();
        // A level may go at most one deeper than the row above it.
        let wanted = level + copy.level;
        let task_level = wanted.min(previous_level + 1).max(1);
        previous_level = task_level;
        let mut task = pasted_task(project, uid, copy, task_level);
        let kept: Vec<(u32, &AssignmentCopy)> = copy
            .assignments
            .iter()
            .filter_map(|assignment| Some((resource_for(project, assignment)?, assignment)))
            .collect();
        // Work belongs to the assignments; without the dropped ones the task has less.
        if !copy.assignments.is_empty() {
            task.work = kept
                .iter()
                .map(|(_, assignment)| assignment.work.max(0))
                .sum();
            task.remaining_work = task.work;
        }
        project.tasks.insert(index, task);
        index += 1;
        uids.push(uid);
        for (resource_uid, assignment) in kept {
            push_assignment(project, uid, resource_uid, assignment);
        }
    }
    for link in &clipboard.links {
        let (Some(from), Some(to)) = (uids.get(link.from as usize), uids.get(link.to as usize))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        let dep_uid = project.alloc_dependency_uid();
        project.dependencies.push(Dependency {
            uid: dep_uid,
            predecessor_uid: *from,
            successor_uid: *to,
            kind: link.kind,
            lag: link.lag,
        });
    }
    Ok(())
}

fn pasted_task(project: &Project, uid: u32, copy: &TaskCopy, level: u32) -> Task {
    let mut task = Task::new(uid, copy.name.clone());
    task.outline_level = level;
    task.milestone = copy.milestone;
    task.manual = copy.manual;
    task.duration = copy.duration.max(0);
    task.remaining_duration = task.duration;
    task.work = copy.work.max(0);
    task.remaining_work = task.work;
    task.start = if copy.manual {
        copy.start
    } else {
        project.start
    };
    task.finish = if copy.manual {
        copy.finish
    } else {
        project.start
    };
    task.notes = copy.notes.clone();
    task.priority = copy.priority;
    task.calendar_id = calendar_for(project, copy);
    task.constraint_type = copy.constraint_type;
    task.constraint_date = copy.constraint_date;
    task.deadline = copy.deadline;
    task.task_type = copy.task_type;
    task.effort_driven = copy.effort_driven;
    task.fixed_cost = copy.fixed_cost;
    task.fixed_cost_accrual = copy.fixed_cost_accrual;
    task.splits = copy.splits.iter().map(|split| Split { leveling: false, ..split.clone() }).collect();
    split::normalize_task(&mut task);
    task.interrupts = copy.interrupts && !task.summary && !task.milestone;
    if !task.summary && !task.milestone {
        task.work_low = copy.work_low;
        task.work_high = copy.work_high;
    }
    for (name, value) in &copy.values {
        let name = name.to_lowercase();
        let field = project.fields.iter().find(|field| field.name.to_lowercase() == name);
        if let Some(field) = field {
            if let Some(value) = fields::converted(value.clone(), &field.kind) {
                task.values.insert(field.id, value);
            }
        }
    }
    task
}

fn calendar_for(project: &Project, copy: &TaskCopy) -> Option<u32> {
    let id = copy.calendar_id?;
    let name = copy.calendar_name.as_deref();
    let same =
        |calendar: &&crate::calendar::Calendar| name.is_none_or(|name| calendar.name == name);
    project
        .calendars
        .iter()
        .find(|calendar| calendar.id == id && same(calendar))
        .or_else(|| {
            name.and_then(|name| {
                project
                    .calendars
                    .iter()
                    .find(|calendar| calendar.name == name)
            })
        })
        .map(|calendar| calendar.id)
}

fn resource_for(project: &Project, copy: &AssignmentCopy) -> Option<u32> {
    let named = !copy.resource_name.is_empty();
    project
        .resources
        .iter()
        .find(|r| r.uid == copy.resource_uid && (!named || r.name == copy.resource_name))
        .or_else(|| {
            named
                .then(|| {
                    project
                        .resources
                        .iter()
                        .find(|r| r.name == copy.resource_name)
                })
                .flatten()
        })
        .map(|resource| resource.uid)
}

fn push_assignment(project: &mut Project, task_uid: u32, resource_uid: u32, copy: &AssignmentCopy) {
    let uid = project.alloc_assignment_uid();
    let work = copy.work.max(0);
    project.assignments.push(Assignment {
        uid,
        task_uid,
        resource_uid,
        units: copy.units.max(0.0),
        work,
        overtime_work: 0,
        actual_work: 0,
        remaining_work: work,
        cost: 0.0,
        actual_cost: 0.0,
        manual_actual_cost: None,
        baselines: Vec::new(),
    });
}

/// Copy `field` of `from_uid` to each task in `to_uids`, as the edit that field's cell makes.
/// Summary tasks only take a name or a custom field value; the rest of their fields roll
/// up from their tasks. The interrupt flag and the effort range go only to leaves that are
/// not milestones, and a summary or milestone source fills nothing rather than clearing
/// them below; a leaf source without a range clears the targets'.
pub fn fill_down(
    project: &mut Project,
    from_uid: u32,
    to_uids: &[u32],
    field: FillField,
) -> Result<(), EngineError> {
    let source = project
        .task(from_uid)
        .ok_or(EngineError::TaskNotFound)?
        .clone();
    let leaf_only = matches!(field, FillField::Interrupts | FillField::WorkRange);
    if leaf_only && (source.summary || source.milestone) {
        return Ok(());
    }
    for uid in to_uids {
        let target = project.task(*uid).ok_or(EngineError::TaskNotFound)?;
        let summary_takes = matches!(field, FillField::Name | FillField::Field(_));
        let flag_refused = leaf_only && target.milestone;
        if *uid == from_uid || (target.summary && !summary_takes) || flag_refused {
            continue;
        }
        let command = fill_command(&source, *uid, field);
        apply_one(project, &command)?;
    }
    Ok(())
}

fn fill_command(source: &Task, uid: u32, field: FillField) -> Command {
    match field {
        FillField::Name => Command::RenameTask {
            uid,
            name: source.name.clone(),
        },
        FillField::Duration => Command::SetDuration {
            uid,
            minutes: source.duration,
        },
        FillField::Start => Command::MoveTask {
            uid,
            start: source.start,
        },
        FillField::Predecessors => Command::SetPredecessors {
            uid,
            text: source.predecessors.clone(),
        },
        FillField::PercentComplete => Command::SetPercentComplete {
            uid,
            percent: source.percent_complete,
        },
        FillField::Calendar => Command::SetTaskCalendar {
            uid,
            calendar_id: source.calendar_id,
        },
        FillField::Field(field_id) => Command::SetFieldValue {
            uid,
            field_id,
            value: source.values.get(&field_id).cloned(),
        },
        FillField::Interrupts => Command::SetInterrupts {
            uid,
            interrupts: source.interrupts,
        },
        FillField::WorkRange => Command::SetWorkRange {
            uid,
            low: source.work_low,
            high: source.work_high,
        },
    }
}
