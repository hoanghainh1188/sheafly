//! Baseline load derivation (PR 4, issue #150).
//!
//! When a baseline is set (`Command::SetBaseline { number }`), each task stores
//! its planned start, finish, work, cost, and split segments in `Task.baselines`,
//! while each assignment stores its planned work in `Assignment.baselines`.
//!
//! This module derives the day-by-day load (in working minutes) for assignments,
//! resources, and projects from those stored baseline snapshots. Split gaps
//! and non-working calendar hours are respected without needing timephased baseline
//! bloat in storage.
//!
//! The days follow the task's calendar as it is now, not as it was when the baseline
//! was set: a calendar edited afterwards moves baseline load between days (the total
//! stays the baseline work). Microsoft Project keeps timephased baseline work instead.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use crate::model::{Assignment, Project, Task};
use crate::split;
use crate::timeutil::{civil, compose};

/// Derives the daily working minutes for an assignment under a specific baseline number.
///
/// If either the task or assignment lacks a baseline with `baseline_number`, or if
/// the assigned baseline work is zero, an empty vector is returned.
///
/// Days are calculated using the task's calendar and the task's baseline intervals
/// (its `segments` if split when baseline was set, or `[start, finish]` if undivided).
/// Work is distributed proportionally across the working days, preserving the exact
/// total `AssignmentBaseline.work` without rounding drift.
pub fn assignment_baseline_days(
    project: &Project,
    task: &Task,
    assignment: &Assignment,
    baseline_number: u8,
) -> Vec<(NaiveDate, i64)> {
    let Some(task_baseline) = task.baselines.iter().find(|b| b.number == baseline_number) else {
        return Vec::new();
    };
    let Some(assign_baseline) = assignment
        .baselines
        .iter()
        .find(|b| b.number == baseline_number)
    else {
        return Vec::new();
    };

    let total_work = assign_baseline.work;
    if total_work <= 0 {
        return Vec::new();
    }

    // Determine the working intervals at the time the baseline was recorded.
    let intervals: Vec<(i64, i64)> = if !task_baseline.segments.is_empty() {
        task_baseline
            .segments
            .iter()
            .map(|s| (s.start, s.finish))
            .collect()
    } else if task_baseline.finish > task_baseline.start {
        vec![(task_baseline.start, task_baseline.finish)]
    } else {
        return Vec::new();
    };

    let Some(min_start) = intervals.iter().map(|(s, _)| *s).min() else {
        return Vec::new();
    };
    let Some(max_finish) = intervals.iter().map(|(_, f)| *f).max() else {
        return Vec::new();
    };

    let calendar = project.task_calendar(task);
    let (mut date, _) = civil(min_start);
    let (end_date, _) = civil(max_finish);

    // Every day of the span, however long: a cap would drop the tail of the work.
    let mut day_minutes: Vec<(NaiveDate, i64)> = Vec::new();
    while let Some(next) = date.succ_opt() {
        let minutes = split::working_within(calendar, &intervals, compose(date, 0), compose(next, 0));
        if minutes > 0 {
            day_minutes.push((date, minutes));
        }
        if date >= end_date {
            break;
        }
        date = next;
    }

    let total_task_minutes: i64 = day_minutes.iter().map(|(_, m)| *m).sum();
    if total_task_minutes <= 0 {
        return Vec::new();
    }

    // Distribute total_work proportionally across days using cumulative rounding
    // to prevent any 1-minute rounding errors from accumulating.
    let mut result = Vec::with_capacity(day_minutes.len());
    let mut accumulated_minutes: i64 = 0;
    let mut previous_work: i64 = 0;

    for (d, minutes) in day_minutes {
        accumulated_minutes += minutes;
        let current_work =
            ((total_work as f64 * accumulated_minutes as f64) / total_task_minutes as f64).round() as i64;
        let day_work = current_work - previous_work;
        previous_work = current_work;

        if day_work > 0 {
            result.push((d, day_work));
        }
    }

    result
}

/// The daily baseline load (working minutes) of every resource that has some, in one
/// pass over the assignments. Charts over many people or periods (PR 5, PR 6) read this
/// once and sum the ranges they need with [`sum_in_range`], instead of asking per person.
pub fn baseline_load_by_resource(
    project: &Project,
    baseline_number: u8,
) -> BTreeMap<u32, BTreeMap<NaiveDate, i64>> {
    let mut by_resource: BTreeMap<u32, BTreeMap<NaiveDate, i64>> = BTreeMap::new();
    for_each_baseline_day(project, baseline_number, |assignment, date, minutes| {
        *by_resource
            .entry(assignment.resource_uid)
            .or_default()
            .entry(date)
            .or_insert(0) += minutes;
    });
    by_resource
}

/// Derives the daily baseline load (in working minutes) for a specific resource.
pub fn resource_baseline_load_by_day(
    project: &Project,
    resource_uid: u32,
    baseline_number: u8,
) -> BTreeMap<NaiveDate, i64> {
    let mut map: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for_each_baseline_day(project, baseline_number, |assignment, date, minutes| {
        if assignment.resource_uid == resource_uid {
            *map.entry(date).or_insert(0) += minutes;
        }
    });
    map
}

/// Derives the daily baseline load (in working minutes) for the entire project.
pub fn project_baseline_load_by_day(
    project: &Project,
    baseline_number: u8,
) -> BTreeMap<NaiveDate, i64> {
    let mut map: BTreeMap<NaiveDate, i64> = BTreeMap::new();
    for_each_baseline_day(project, baseline_number, |_, date, minutes| {
        *map.entry(date).or_insert(0) += minutes;
    });
    map
}

/// The minutes of a daily load within `[from, to]` (both included).
pub fn sum_in_range(days: &BTreeMap<NaiveDate, i64>, from: NaiveDate, to: NaiveDate) -> i64 {
    if from > to {
        return 0;
    }
    days.range(from..=to).map(|(_, &m)| m).sum()
}

/// Computes the total baseline load (in working minutes) for a resource within `[from, to]`.
pub fn resource_baseline_load_in_range(
    project: &Project,
    resource_uid: u32,
    baseline_number: u8,
    from: NaiveDate,
    to: NaiveDate,
) -> i64 {
    sum_in_range(&resource_baseline_load_by_day(project, resource_uid, baseline_number), from, to)
}

/// Computes the total project baseline load (in working minutes) within `[from, to]`.
pub fn project_baseline_load_in_range(
    project: &Project,
    baseline_number: u8,
    from: NaiveDate,
    to: NaiveDate,
) -> i64 {
    sum_in_range(&project_baseline_load_by_day(project, baseline_number), from, to)
}

/// Calls `visit` with each day of each assignment's baseline load. Tasks are looked up
/// through one index, so the cost grows with the assignments, not assignments × tasks.
fn for_each_baseline_day(
    project: &Project,
    baseline_number: u8,
    mut visit: impl FnMut(&Assignment, NaiveDate, i64),
) {
    let tasks: HashMap<u32, &Task> = project.tasks.iter().map(|task| (task.uid, task)).collect();
    for assignment in &project.assignments {
        if !assignment.baselines.iter().any(|b| b.number == baseline_number) {
            continue;
        }
        let Some(task) = tasks.get(&assignment.task_uid) else {
            continue;
        };
        for (date, minutes) in assignment_baseline_days(project, task, assignment, baseline_number) {
            visit(assignment, date, minutes);
        }
    }
}
