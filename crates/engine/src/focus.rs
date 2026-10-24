//! The productivity factor (PFD: personal time, fatigue, delays).
//!
//! People give only part of their time at work to project work. `Project.focus_factor`
//! (default 1.0) says how much, and `Resource.focus` overrides it for one resource;
//! machines have 1.0 unless they say otherwise. The factor slows the rate at which work is
//! done: an assignment at `units` works `units × focus` per working minute, so 48 hours of
//! work for one person at 1.0 units and 0.8 take 60 working hours (7.5 days of 8).
//!
//! Units stay what the user typed (a person assigned full time is still at 1.0, and
//! overallocation and leveling compare units as before); only the work ↔ duration rules of
//! `work.rs`, the usage rows and the load map's capacity read the rate. `with_rates` runs
//! those rules on rates and puts the units back, so the rules themselves are unchanged and
//! a project whose factors are all 1.0 schedules exactly as before.
//!
//! A task with no assignment works at the project's factor. Changing a factor reapplies
//! the rules, as a change of units would, to the tasks it reaches that have not started;
//! started and finished work keeps its dates and work. MSPDI has no factor: export writes
//! `units × focus` as the assignment's units, so Microsoft Project gets the same dates, and
//! import reads every factor as 1.0.

use std::collections::HashMap;

use crate::error::{EngineError, Invalid};
use crate::model::{Project, Resource, ResourceKind};
use crate::work::{self, Changed};

/// The lowest factor a project or resource can have.
pub const MIN_FOCUS: f64 = 0.5;
/// The highest: all of the time at work.
pub const MAX_FOCUS: f64 = 1.0;

/// The factor of a project or resource that says nothing: all of the time.
pub fn full_focus() -> f64 {
    MAX_FOCUS
}

/// The factor `resource` works at.
pub fn focus_of(project: &Project, resource: &Resource) -> f64 {
    resource.focus.unwrap_or(match resource.kind {
        ResourceKind::Person => project.focus_factor,
        ResourceKind::Machine => MAX_FOCUS,
    })
}

/// The factor of the resource with `uid`; 1.0 for one the project lacks.
pub fn focus_of_uid(project: &Project, uid: u32) -> f64 {
    project
        .resources
        .iter()
        .find(|resource| resource.uid == uid)
        .map_or(MAX_FOCUS, |resource| focus_of(project, resource))
}

fn checked(factor: f64) -> Result<f64, EngineError> {
    if !factor.is_finite() || !(MIN_FOCUS..=MAX_FOCUS).contains(&factor) {
        return Err(Invalid::FocusFactor.into());
    }
    Ok(factor)
}

/// Set the project's factor and reapply the rules to the tasks it reaches.
pub(crate) fn set_project_focus(project: &mut Project, factor: f64) -> Result<(), EngineError> {
    let factor = checked(factor)?;
    if project.focus_factor == factor {
        return Ok(());
    }
    // People without a factor of their own follow the project's.
    let following: Vec<u32> = project
        .resources
        .iter()
        .filter(|resource| resource.focus.is_none() && resource.kind == ResourceKind::Person)
        .map(|resource| resource.uid)
        .collect();
    let reached = tasks_reached(project, |assigned| {
        assigned.is_empty() || assigned.iter().any(|uid| following.contains(uid))
    });
    project.focus_factor = factor;
    reapply(project, &reached);
    Ok(())
}

/// Set (or, `None`, clear) a resource's own factor and reapply the rules to its tasks.
pub(crate) fn set_resource_focus(
    project: &mut Project,
    uid: u32,
    focus: Option<f64>,
) -> Result<(), EngineError> {
    let focus = focus.map(checked).transpose()?;
    let position = project
        .resources
        .iter()
        .position(|resource| resource.uid == uid)
        .ok_or(EngineError::ResourceNotFound)?;
    let before = focus_of(project, &project.resources[position]);
    project.resources[position].focus = focus;
    if focus_of(project, &project.resources[position]) != before {
        reapply_resource(project, uid);
    }
    Ok(())
}

/// A resource's factor may have changed some other way (its kind): reapply its tasks.
pub(crate) fn reapply_resource(project: &mut Project, uid: u32) {
    let reached = tasks_reached(project, |assigned| assigned.contains(&uid));
    reapply(project, &reached);
}

/// The uids of the leaf tasks not started yet for which `reaches(assigned)` holds,
/// `assigned` being the uids of the resources on the task.
fn tasks_reached(project: &Project, reaches: impl Fn(&[u32]) -> bool) -> Vec<u32> {
    let mut assigned: HashMap<u32, Vec<u32>> = HashMap::new();
    for assignment in &project.assignments {
        assigned
            .entry(assignment.task_uid)
            .or_default()
            .push(assignment.resource_uid);
    }
    project
        .tasks
        .iter()
        .filter(|task| !task.summary && task.actual_start.is_none())
        .filter(|task| reaches(assigned.get(&task.uid).map_or(&[], Vec::as_slice)))
        .map(|task| task.uid)
        .collect()
}

/// Reapply the work rules to `tasks` as a change of units does.
fn reapply(project: &mut Project, tasks: &[u32]) {
    for &uid in tasks {
        work::apply_change(project, uid, Changed::Units);
    }
}

/// Run `rules` on `task_uid` with each of its assignments' units (or, with none, the
/// task's units) multiplied by their factor, then put the units back. A unit the rules
/// left as it was gets its exact old value back, so nothing drifts by rounding; one they
/// changed is divided by the factor again. With every factor at 1.0 nothing is touched.
pub(crate) fn with_rates<R>(
    project: &mut Project,
    task_uid: u32,
    rules: impl FnOnce(&mut Project) -> R,
) -> R {
    // (assignment uid, units as typed, factor, units as rate)
    let scaled: Vec<(u32, f64, f64, f64)> = project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == task_uid)
        .map(|assignment| {
            let focus = focus_of_uid(project, assignment.resource_uid);
            (
                assignment.uid,
                assignment.units,
                focus,
                assignment.units * focus,
            )
        })
        .collect();
    let unassigned = scaled.is_empty();
    let task_focus = project.focus_factor;
    let full = if unassigned {
        task_focus == MAX_FOCUS
    } else {
        scaled.iter().all(|(_, _, focus, _)| *focus == MAX_FOCUS)
    };
    if full {
        return rules(project);
    }
    for assignment in project
        .assignments
        .iter_mut()
        .filter(|assignment| assignment.task_uid == task_uid)
    {
        if let Some((_, _, _, rate)) = scaled.iter().find(|entry| entry.0 == assignment.uid) {
            assignment.units = *rate;
        }
    }
    let task_units = if unassigned {
        project.task_mut(task_uid).map(|task| {
            if task.units <= 0.0 {
                task.units = 1.0;
            }
            let nominal = task.units;
            task.units = nominal * task_focus;
            (nominal, task.units)
        })
    } else {
        None
    };

    let result = rules(project);

    let mut total = 0.0;
    for assignment in project
        .assignments
        .iter_mut()
        .filter(|assignment| assignment.task_uid == task_uid)
    {
        if let Some((_, nominal, focus, rate)) =
            scaled.iter().find(|entry| entry.0 == assignment.uid)
        {
            assignment.units = if assignment.units == *rate {
                *nominal
            } else {
                assignment.units / focus
            };
        }
        total += assignment.units;
    }
    if let Some(task) = project.task_mut(task_uid) {
        match task_units {
            Some((nominal, rate)) => {
                task.units = if task.units == rate {
                    nominal
                } else {
                    task.units / task_focus
                };
            }
            None if !unassigned => task.units = total,
            None => {}
        }
    }
    result
}
