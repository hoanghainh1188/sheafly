use std::collections::HashMap;

use crate::model::{Accrual, Assignment, Project, Task, TaskType, UsageDay, UsageRow};
use crate::focus;
use crate::split;
use crate::timeutil::{civil, compose, date_key};

#[derive(Clone, Copy, Debug)]
pub enum Changed {
    Duration,
    Units,
    Work,
}

fn scale(duration: i64, units: f64) -> i64 {
    (duration as f64 * units).round().max(0.0) as i64
}

/// Keep `work = duration × units × focus` according to the task type (`focus.rs`).
pub fn apply_change(project: &mut Project, task_uid: u32, changed: Changed) {
    focus::with_rates(project, task_uid, |project| apply_change_at_rates(project, task_uid, changed));
}

/// Keep `work = duration × units` according to the task type, units being rates.
fn apply_change_at_rates(project: &mut Project, task_uid: u32, changed: Changed) {
    let Some(index) = project.index_of(task_uid) else {
        return;
    };
    if project.tasks[index].summary {
        return;
    }
    if project.tasks[index].task_type == TaskType::FixedWork {
        project.tasks[index].effort_driven = true;
    }
    let assignment_indexes: Vec<usize> = project
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.task_uid == task_uid)
        .map(|(i, _)| i)
        .collect();

    if assignment_indexes.is_empty() {
        let task = &mut project.tasks[index];
        if task.units <= 0.0 {
            task.units = 1.0;
        }
        match task.task_type {
            TaskType::FixedUnits => match changed {
                Changed::Duration => task.work = scale(task.duration, task.units),
                Changed::Work => {
                    task.duration = (task.work as f64 / task.units).round().max(0.0) as i64;
                }
                Changed::Units => {
                    if task.effort_driven && task.work > 0 {
                        task.duration = (task.work as f64 / task.units).round().max(0.0) as i64;
                    } else {
                        task.work = scale(task.duration, task.units);
                    }
                }
            },
            TaskType::FixedDuration => match changed {
                Changed::Duration | Changed::Units => {
                    task.work = scale(task.duration, task.units);
                }
                Changed::Work => {
                    if task.duration > 0 {
                        task.units = task.work as f64 / task.duration as f64;
                    }
                }
            },
            TaskType::FixedWork => match changed {
                Changed::Work | Changed::Units => {
                    task.duration = (task.work as f64 / task.units).round().max(0.0) as i64;
                }
                Changed::Duration => {
                    if task.duration > 0 {
                        task.units = task.work as f64 / task.duration as f64;
                    }
                }
            },
        }
        task.milestone = task.duration == 0;
        return;
    }

    let task_type = project.tasks[index].task_type;
    let effort_driven = project.tasks[index].effort_driven;
    let mut duration = project.tasks[index].duration;
    let units: f64 = assignment_indexes
        .iter()
        .map(|i| project.assignments[*i].units)
        .sum();
    let mut work: i64 = assignment_indexes
        .iter()
        .map(|i| project.assignments[*i].work)
        .sum();

    match task_type {
        TaskType::FixedUnits => match changed {
            Changed::Duration => {
                for i in &assignment_indexes {
                    let units = project.assignments[*i].units;
                    project.assignments[*i].work = scale(duration, units);
                }
                work = assignment_indexes
                    .iter()
                    .map(|i| project.assignments[*i].work)
                    .sum();
            }
            Changed::Work => {
                if units > 0.0 {
                    duration = (work as f64 / units).round().max(0.0) as i64;
                }
                for i in &assignment_indexes {
                    let units = project.assignments[*i].units;
                    project.assignments[*i].work = scale(duration, units);
                }
                work = assignment_indexes
                    .iter()
                    .map(|i| project.assignments[*i].work)
                    .sum();
            }
            Changed::Units => {
                if effort_driven && work > 0 && units > 0.0 {
                    duration = (work as f64 / units).round().max(0.0) as i64;
                }
                for i in &assignment_indexes {
                    let units = project.assignments[*i].units;
                    project.assignments[*i].work = scale(duration, units);
                }
                work = assignment_indexes
                    .iter()
                    .map(|i| project.assignments[*i].work)
                    .sum();
            }
        },
        TaskType::FixedDuration => match changed {
            Changed::Duration | Changed::Units => {
                for i in &assignment_indexes {
                    let units = project.assignments[*i].units;
                    project.assignments[*i].work = scale(duration, units);
                }
                work = assignment_indexes
                    .iter()
                    .map(|i| project.assignments[*i].work)
                    .sum();
            }
            Changed::Work => {
                if duration > 0 && units > 0.0 {
                    // Keep each assignment's share of the new total work via units.
                    let target = work as f64;
                    let current_units = units;
                    for i in &assignment_indexes {
                        let share = project.assignments[*i].units / current_units;
                        project.assignments[*i].units = if duration > 0 {
                            (target * share) / duration as f64
                        } else {
                            project.assignments[*i].units
                        };
                        project.assignments[*i].work =
                            scale(duration, project.assignments[*i].units);
                    }
                    work = assignment_indexes
                        .iter()
                        .map(|i| project.assignments[*i].work)
                        .sum();
                }
            }
        },
        TaskType::FixedWork => match changed {
            Changed::Units | Changed::Work => {
                if units > 0.0 {
                    duration = (work as f64 / units).round().max(0.0) as i64;
                }
                for i in &assignment_indexes {
                    let units = project.assignments[*i].units;
                    project.assignments[*i].work = scale(duration, units);
                }
                work = assignment_indexes
                    .iter()
                    .map(|i| project.assignments[*i].work)
                    .sum();
            }
            Changed::Duration => {
                if duration > 0 && work > 0 {
                    let new_units = work as f64 / duration as f64;
                    let old_units = units.max(0.0001);
                    for i in &assignment_indexes {
                        let share = project.assignments[*i].units / old_units;
                        project.assignments[*i].units = new_units * share;
                        project.assignments[*i].work =
                            scale(duration, project.assignments[*i].units);
                    }
                    work = assignment_indexes
                        .iter()
                        .map(|i| project.assignments[*i].work)
                        .sum();
                }
            }
        },
    }

    let task = &mut project.tasks[index];
    task.duration = duration.max(0);
    task.milestone = task.duration == 0;
    task.units = assignment_indexes
        .iter()
        .map(|i| project.assignments[*i].units)
        .sum();
    task.work = work;
}

/// The duration and work a task with no assignment gets when one resource is assigned to
/// it at `units`, as `add_assignment` sets them: the new assignment carries the task's
/// work, then the task type decides what follows. Suggestions use it to date a task for
/// each person they consider.
pub fn with_first_assignment(task: &Task, units: f64) -> (i64, i64) {
    let units = units.max(0.0);
    let (duration, work) = (task.duration, task.work);
    let from_work = (work as f64 / units).round().max(0.0) as i64;
    let duration = match task.task_type {
        TaskType::FixedUnits if task.effort_driven && work > 0 && units > 0.0 => from_work,
        TaskType::FixedWork if units > 0.0 => from_work,
        _ => duration,
    };
    (duration.max(0), scale(duration, units))
}

pub fn add_assignment(project: &mut Project, task_uid: u32, resource_uid: u32, units: f64) -> u32 {
    if let Some(existing) = project
        .assignments
        .iter_mut()
        .find(|a| a.task_uid == task_uid && a.resource_uid == resource_uid)
    {
        existing.units = units.max(0.0);
        let uid = existing.uid;
        apply_change(project, task_uid, Changed::Units);
        return uid;
    }
    let had = project.assignments.iter().any(|a| a.task_uid == task_uid);
    let uid = project.alloc_assignment_uid();
    let existing_work: i64 = project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == task_uid)
        .map(|assignment| assignment.work)
        .sum();
    let carried_work = if had {
        existing_work
    } else {
        project.task(task_uid).map(|task| task.work).unwrap_or(0)
    };
    project.assignments.push(Assignment {
        uid,
        task_uid,
        resource_uid,
        units: units.max(0.0),
        work: if had { 0 } else { carried_work },
        overtime_work: 0,
        actual_work: 0,
        remaining_work: 0,
        cost: 0.0,
        actual_cost: 0.0,
        manual_actual_cost: None,
        baselines: Vec::new(),
    });
    // Adding another resource to an effort-driven fixed-duration task keeps
    // duration and total work, and shares that work across the new unit weights.
    // Editing units on an existing assignment still follows the task-type table.
    let keep_work = project.task(task_uid).is_some_and(|task| {
        let effort = task.effort_driven || task.task_type == TaskType::FixedWork;
        had && effort
            && task.task_type == TaskType::FixedDuration
            && task.duration > 0
            && carried_work > 0
    });
    if keep_work {
        let duration = project
            .task(task_uid)
            .map(|task| task.duration)
            .unwrap_or(0);
        let kept = focus::with_rates(project, task_uid, |project| {
            scale_units_to_work(project, task_uid, duration, carried_work)
        });
        if kept {
            return uid;
        }
    }
    apply_change(project, task_uid, Changed::Units);
    uid
}

/// Drop one assignment. Effort-driven tasks keep total work: fixed duration
/// rescales units, and the other task types recompute duration.
pub fn remove_assignment(project: &mut Project, assignment_uid: u32) -> bool {
    let Some(task_uid) = project
        .assignments
        .iter()
        .find(|assignment| assignment.uid == assignment_uid)
        .map(|assignment| assignment.task_uid)
    else {
        return false;
    };
    let preserved_work: i64 = project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == task_uid)
        .map(|assignment| assignment.work)
        .sum();
    let snapshot = project.task(task_uid).map(|task| {
        (
            task.task_type,
            task.effort_driven || task.task_type == TaskType::FixedWork,
            task.duration,
        )
    });
    project
        .assignments
        .retain(|assignment| assignment.uid != assignment_uid);
    let Some((task_type, effort, duration)) = snapshot else {
        return true;
    };
    let remaining = project
        .assignments
        .iter()
        .any(|assignment| assignment.task_uid == task_uid);
    if !remaining {
        if effort {
            if let Some(task) = project.task_mut(task_uid) {
                task.work = preserved_work;
                if task.units <= 0.0 {
                    task.units = 1.0;
                }
            }
        } else {
            apply_change(project, task_uid, Changed::Units);
        }
        return true;
    }
    if effort
        && preserved_work > 0
        && task_type == TaskType::FixedDuration
        && duration > 0
        && focus::with_rates(project, task_uid, |project| {
            scale_units_to_work(project, task_uid, duration, preserved_work)
        })
    {
        return true;
    }
    if effort && preserved_work > 0 {
        focus::with_rates(project, task_uid, |project| {
            spread_work(project, task_uid, preserved_work)
        });
        apply_change(project, task_uid, Changed::Work);
        return true;
    }
    apply_change(project, task_uid, Changed::Units);
    true
}

fn assignment_indexes(project: &Project, task_uid: u32) -> Vec<usize> {
    project
        .assignments
        .iter()
        .enumerate()
        .filter(|(_, assignment)| assignment.task_uid == task_uid)
        .map(|(index, _)| index)
        .collect()
}

/// Keep `duration` and `preserved_work`, scaling assignment units by their current weights.
fn scale_units_to_work(
    project: &mut Project,
    task_uid: u32,
    duration: i64,
    preserved_work: i64,
) -> bool {
    let indexes = assignment_indexes(project, task_uid);
    if indexes.is_empty() || duration <= 0 {
        return false;
    }
    let weight: f64 = indexes
        .iter()
        .map(|index| project.assignments[*index].units.max(0.0))
        .sum();
    if weight <= 0.0 {
        return false;
    }
    let target = preserved_work as f64 / duration as f64;
    for index in &indexes {
        let share = project.assignments[*index].units.max(0.0) / weight;
        project.assignments[*index].units = (target * share).max(0.0);
        project.assignments[*index].work = scale(duration, project.assignments[*index].units);
    }
    let units: f64 = indexes
        .iter()
        .map(|index| project.assignments[*index].units)
        .sum();
    let work: i64 = indexes
        .iter()
        .map(|index| project.assignments[*index].work)
        .sum();
    if let Some(task) = project.task_mut(task_uid) {
        task.duration = duration.max(0);
        task.milestone = task.duration == 0;
        task.units = units;
        task.work = work;
    }
    true
}

fn spread_work(project: &mut Project, task_uid: u32, preserved_work: i64) {
    let indexes = assignment_indexes(project, task_uid);
    if indexes.is_empty() {
        return;
    }
    let weight: f64 = indexes
        .iter()
        .map(|index| project.assignments[*index].units.max(0.0))
        .sum();
    if weight <= 0.0 {
        project.assignments[indexes[0]].work = preserved_work.max(0);
        for index in indexes.iter().skip(1) {
            project.assignments[*index].work = 0;
        }
        return;
    }
    let mut assigned = 0i64;
    for (offset, index) in indexes.iter().enumerate() {
        let piece = if offset + 1 == indexes.len() {
            preserved_work - assigned
        } else {
            let share = (project.assignments[*index].units.max(0.0) / weight
                * preserved_work as f64)
                .round() as i64;
            assigned += share;
            share
        };
        project.assignments[*index].work = piece.max(0);
    }
}

pub fn publish_costs_and_usage(project: &mut Project) {
    let by_task = assignments_by_task(project);
    // Leaf actuals and costs have to exist before summary rollup reads them.
    refresh_tracking(project, &by_task);
    compute_costs(project, &by_task);
    rollup_summary_work(project);
    project.usage = build_usage(project, &by_task);
}

/// Assignment positions per task UID, in assignment order.
fn assignments_by_task(project: &Project) -> HashMap<u32, Vec<usize>> {
    let mut by_task: HashMap<u32, Vec<usize>> = HashMap::new();
    for (index, assignment) in project.assignments.iter().enumerate() {
        by_task.entry(assignment.task_uid).or_default().push(index);
    }
    by_task
}

fn assignments_of(by_task: &HashMap<u32, Vec<usize>>, uid: u32) -> &[usize] {
    by_task.get(&uid).map_or(&[], Vec::as_slice)
}

fn rollup_summary_work(project: &mut Project) {
    for index in (0..project.tasks.len()).rev() {
        if !project.tasks[index].summary {
            continue;
        }
        let level = project.tasks[index].outline_level;
        let mut work = 0i64;
        let mut actual = 0i64;
        let mut remaining = 0i64;
        let mut cost = 0.0;
        let mut actual_cost = 0.0;
        for j in index + 1..project.tasks.len() {
            if project.tasks[j].outline_level <= level {
                break;
            }
            if project.tasks[j].outline_level == level + 1 {
                work += project.tasks[j].work;
                actual += project.tasks[j].actual_work;
                remaining += project.tasks[j].remaining_work;
                cost += project.tasks[j].cost;
                actual_cost += project.tasks[j].actual_cost;
            }
        }
        let fixed = project.tasks[index].fixed_cost;
        let accrual = project.tasks[index].fixed_cost_accrual;
        let fraction = if work > 0 {
            (actual as f64 / work as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fixed_actual = match accrual {
            Accrual::Prorated => fixed * fraction,
            Accrual::Start => {
                if fraction > 0.0 {
                    fixed
                } else {
                    0.0
                }
            }
            Accrual::End => {
                if fraction >= 1.0 {
                    fixed
                } else {
                    0.0
                }
            }
        };
        let task = &mut project.tasks[index];
        // A summary's actual cost is always the rollup; an entry there is stale.
        task.manual_actual_cost = None;
        task.work = work;
        task.actual_work = actual;
        task.remaining_work = remaining;
        task.cost = cost + fixed;
        task.actual_cost = actual_cost + fixed_actual;
        task.remaining_cost = (task.cost - task.actual_cost).max(0.0);
        if work > 0 {
            task.percent_complete = (fraction * 100.0).clamp(0.0, 100.0);
        }
    }
}

fn refresh_tracking(project: &mut Project, by_task: &HashMap<u32, Vec<usize>>) {
    for index in 0..project.tasks.len() {
        if project.tasks[index].summary {
            continue;
        }
        let uid = project.tasks[index].uid;
        let pct = project.tasks[index].percent_complete.clamp(0.0, 100.0);
        let (duration, work) = (project.tasks[index].duration, project.tasks[index].work);
        let fraction = pct / 100.0;
        let remaining_duration = (duration as f64 * (1.0 - fraction)).round().max(0.0) as i64;
        let actual_work = (work as f64 * fraction).round().max(0.0) as i64;
        let remaining_work = (work - actual_work).max(0);
        let task = &mut project.tasks[index];
        // Actual dates are the user's input: commands keep them in step with the
        // percent, and scheduling must not rewrite them from derived dates.
        task.percent_complete = pct;
        task.remaining_duration = remaining_duration;
        task.actual_work = actual_work;
        task.remaining_work = remaining_work;
        if pct >= 100.0 {
            task.remaining_duration = 0;
            task.remaining_work = 0;
            task.actual_work = work;
        }
        for &position in assignments_of(by_task, uid) {
            let assignment = &mut project.assignments[position];
            assignment.actual_work = (assignment.work as f64 * fraction).round().max(0.0) as i64;
            assignment.remaining_work = (assignment.work - assignment.actual_work).max(0);
            if pct >= 100.0 {
                assignment.actual_work = assignment.work;
                assignment.remaining_work = 0;
            }
        }
    }
}

fn compute_costs(project: &mut Project, by_task: &HashMap<u32, Vec<usize>>) {
    let editable = project.editable_actual_costs;
    let rates: Vec<(u32, f64, f64, f64)> = project
        .resources
        .iter()
        .map(|r| (r.uid, r.standard_rate, r.overtime_rate, r.cost_per_use))
        .collect();
    for assignment in &mut project.assignments {
        let Some((_, standard, overtime, per_use)) =
            rates.iter().find(|(uid, _, _, _)| *uid == assignment.resource_uid)
        else {
            continue;
        };
        let overtime_work = assignment.overtime_work.clamp(0, assignment.work);
        let regular = assignment.work - overtime_work;
        assignment.cost = regular as f64 / 60.0 * standard
            + overtime_work as f64 / 60.0 * overtime
            + per_use;
    }
    for index in 0..project.tasks.len() {
        if project.tasks[index].summary {
            continue;
        }
        let own = assignments_of(by_task, project.tasks[index].uid);
        let labor: f64 = own.iter().map(|&position| project.assignments[position].cost).sum();
        let task = &project.tasks[index];
        let total = labor + task.fixed_cost;
        let pct = task.percent_complete.clamp(0.0, 100.0) / 100.0;
        let manual = task
            .manual_actual_cost
            .filter(|_| editable)
            .map(|cost| cost.max(0.0));
        // The accrual is the fixed cost's only; labour is spent as the work is done.
        let actual = labor * pct + fixed_actual(task.fixed_cost, task.fixed_cost_accrual, pct);
        let actual = manual.unwrap_or(actual);
        let actual_labor = if manual.is_some() {
            if total > 0.0 {
                labor * (actual / total).min(1.0)
            } else {
                0.0
            }
        } else {
            labor * pct
        };
        // Assignment entries replace their computed share; the task total follows them
        // unless the task has an entry of its own.
        let mut any_entry = false;
        let mut labor_actual = 0.0;
        for &position in own {
            let assignment = &mut project.assignments[position];
            let share = if labor > 0.0 {
                actual_labor * (assignment.cost / labor)
            } else {
                0.0
            };
            // The task's own entry wins: assignments then share it.
            let entry = assignment.manual_actual_cost.filter(|_| editable && manual.is_none());
            any_entry |= entry.is_some();
            assignment.actual_cost = entry.map_or(share, |cost| cost.max(0.0));
            labor_actual += assignment.actual_cost;
        }
        let actual = if manual.is_none() && any_entry {
            actual - actual_labor + labor_actual
        } else {
            actual
        };
        let task = &mut project.tasks[index];
        task.cost = total;
        task.actual_cost = actual;
        task.remaining_cost = (total - actual).max(0.0);
    }
}

/// The part of a fixed cost spent at `pct` (0 to 1) done: by then for prorated, all of it
/// once started for start, all of it once finished for end.
pub(crate) fn fixed_actual(fixed: f64, accrual: Accrual, pct: f64) -> f64 {
    match accrual {
        Accrual::Prorated => fixed * pct,
        Accrual::Start if pct > 0.0 => fixed,
        Accrual::End if pct >= 1.0 => fixed,
        Accrual::Start | Accrual::End => 0.0,
    }
}

fn build_usage(project: &Project, by_task: &HashMap<u32, Vec<usize>>) -> Vec<UsageRow> {
    let mut rows = Vec::new();
    for task in &project.tasks {
        if task.summary {
            continue;
        }
        let cal = project.task_calendar(task);
        let assignments: Vec<&Assignment> = assignments_of(by_task, task.uid)
            .iter()
            .map(|&position| &project.assignments[position])
            .collect();
        // Each assignment works at its rate, units × the resource's factor (`focus.rs`).
        let rates: Vec<f64> = assignments
            .iter()
            .map(|a| a.units * focus::focus_of_uid(project, a.resource_uid))
            .collect();
        let total_units: f64 = rates.iter().sum();
        let mut task_days: Vec<UsageDay> = Vec::new();
        // Split gaps carry no work, so only the segments are distributed.
        let intervals = split::work_intervals(task);
        if task.finish > task.start {
            let (mut date, _) = civil(task.start);
            let (end_date, _) = civil(task.finish);
            for _ in 0..3660 {
                let day_start = compose(date, 0);
                let next = date.succ_opt().unwrap_or(date);
                let day_end = compose(next, 0);
                let minutes = split::working_within(cal, &intervals, day_start, day_end);
                if minutes > 0 {
                    let hours = if assignments.is_empty() {
                        minutes as f64 / 60.0 * project.focus_factor
                    } else {
                        rates.iter().map(|rate| minutes as f64 / 60.0 * rate).sum()
                    };
                    task_days.push(UsageDay {
                        date: date_key(date),
                        hours,
                    });
                }
                if date >= end_date {
                    break;
                }
                date = next;
            }
        }
        rows.push(UsageRow {
            task_uid: task.uid,
            assignment_uid: None,
            resource_uid: None,
            label: task.name.clone(),
            days: task_days.clone(),
        });
        for (assignment, rate) in assignments.into_iter().zip(rates) {
            let resource_name = project
                .resources
                .iter()
                .find(|r| r.uid == assignment.resource_uid)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| format!("NL {}", assignment.resource_uid));
            let days = task_days
                .iter()
                .map(|day| UsageDay {
                    date: day.date.clone(),
                    hours: {
                        // Rebuild from duration hours × units. Task row hours already include units,
                        // so recover duration hours by dividing back out when possible.
                        let duration_hours: f64 = {
                            if total_units > 0.0 {
                                day.hours / total_units
                            } else {
                                0.0
                            }
                        };
                        duration_hours * rate
                    },
                })
                .collect();
            rows.push(UsageRow {
                task_uid: task.uid,
                assignment_uid: Some(assignment.uid),
                resource_uid: Some(assignment.resource_uid),
                label: resource_name,
                days,
            });
        }
    }
    rows
}
