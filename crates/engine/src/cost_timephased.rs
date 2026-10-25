//! Costs over time, day by day (phase 25, sheafly-pro#5): what the S-curves of cost and the
//! cash flow's money out share.
//!
//! Every series follows the EVM panel (`evm.rs`): a leaf's fixed cost is spread by its accrual
//! (all at the start, all at the finish, or with the working time) and its labour with the
//! working time of its segments, so split gaps and days off carry nothing.
//! - `Scheduled`: the current plan, each leaf's cost over its segments.
//! - `Baseline`: baseline n's cost over its segments, its fixed part (`Baseline::fixed_part`)
//!   by the accrual, as PV does.
//! - `Actual`: each leaf's actual cost over its progress windows (`evm::progress_intervals`)
//!   cut at the status date, labour with the working time and the fixed cost by the accrual,
//!   as AC does; with no status date nothing is cut.
//! - `Remaining`: cost minus actual cost of each leaf, spent on the work still to do: the
//!   segments after the progress point and after the status date. The fixed part not yet
//!   spent follows the accrual (`Start`: at the start when not started yet; `End`: at the
//!   finish; prorated: with the remaining working time). Work left before the status date
//!   that the schedule still shows there is spent on the status date.
//! - `Forecast`: actual up to the status date plus remaining after it.
//!
//! Each series adds up to its cost: the leaves' cost, baseline cost, actual cost, and the
//! sum of `max(cost − actual cost, 0)`. Nothing here changes `compute_costs`.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::calendar::Calendar;
use crate::evm::progress_intervals;
use crate::model::{Accrual, Baseline, Project, Task};
use crate::split;
use crate::timeutil::{civil, compose};

/// Which costs to spread over time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CostSeries {
    Scheduled,
    Baseline { number: u8 },
    Actual,
    Remaining,
    Forecast,
}

/// Where spread amounts go: one day and an amount at a time.
pub(crate) type Sink<'a> = dyn FnMut(NaiveDate, f64) + 'a;

/// The status date and its day, which actual costs are cut at.
pub(crate) type Status = (i64, NaiveDate);

/// The costs of `series` by day, from `from` to `to` (both included): the days that have
/// any, in order. Costs on days outside the span are left out.
pub fn cost_timephased(
    project: &Project,
    series: CostSeries,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, f64)> {
    let mut days: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let status = project.status_date.map(|s| (s, civil(s).0));
    {
        let mut add = |day: NaiveDate, amount: f64| {
            if day >= from && day <= to && amount != 0.0 {
                *days.entry(day).or_default() += amount;
            }
        };
        spread_series(project, series, status, true, &mut add);
    }
    days.into_iter()
        .filter(|(_, amount)| *amount != 0.0)
        .collect()
}

/// Spreads the costs of `series` over every leaf, into `add`. `keep_late` puts actual cost
/// whose progress has no working time up to the status date on the status date, so the
/// series keeps its total; the S-curves leave it out, as they always have.
pub(crate) fn spread_series(
    project: &Project,
    series: CostSeries,
    status: Option<Status>,
    keep_late: bool,
    add: &mut Sink,
) {
    let leaves = project.tasks.iter().filter(|t| !t.summary);
    let labour = || labour_done(project);
    match series {
        CostSeries::Scheduled => leaves.for_each(|task| spread_scheduled(project, task, add)),
        CostSeries::Baseline { number } => {
            for task in leaves {
                if let Some(b) = task.baselines.iter().find(|b| b.number == number) {
                    spread_baseline(project, task, b, add);
                }
            }
        }
        CostSeries::Actual => {
            let cut = status.unwrap_or((i64::MAX, NaiveDate::MAX));
            let labour = labour();
            leaves.for_each(|task| spread_actual(project, task, &labour, cut, keep_late, add));
        }
        CostSeries::Remaining => {
            let labour = labour();
            leaves.for_each(|task| spread_remaining(project, task, &labour, status, add));
        }
        CostSeries::Forecast => {
            spread_series(project, CostSeries::Actual, status, keep_late, add);
            spread_series(project, CostSeries::Remaining, status, keep_late, add);
        }
    }
}

/// A leaf's cost as scheduled: the fixed part by the accrual, labour with the working time.
fn spread_scheduled(project: &Project, task: &Task, add: &mut Sink) {
    if task.cost <= 0.0 {
        return;
    }
    let calendar = project.task_calendar(task);
    let intervals = split::work_intervals(task);
    let fixed = task.fixed_cost.clamp(0.0, task.cost);
    let (start, finish) = (task.start, task.finish);
    let accrual = task.fixed_cost_accrual;
    spread_accrued(calendar, accrual, &intervals, start, finish, fixed, add);
    spread(
        calendar,
        &intervals,
        task.cost - fixed,
        civil(finish).0,
        add,
    );
}

/// A leaf's baseline cost, as PV spreads it.
fn spread_baseline(project: &Project, task: &Task, b: &Baseline, add: &mut Sink) {
    if b.cost <= 0.0 {
        return;
    }
    let calendar = project.task_calendar(task);
    let intervals = baseline_intervals(b);
    let fixed = b.fixed_part(task);
    let accrual = task.fixed_cost_accrual;
    spread_accrued(calendar, accrual, &intervals, b.start, b.finish, fixed, add);
    spread(calendar, &intervals, b.cost - fixed, civil(b.finish).0, add);
}

/// Each task's assignments' actual cost, in one pass over the assignments.
fn labour_done(project: &Project) -> HashMap<u32, f64> {
    let mut labour: HashMap<u32, f64> = HashMap::new();
    for assignment in &project.assignments {
        *labour.entry(assignment.task_uid).or_default() += assignment.actual_cost;
    }
    labour
}

/// A leaf's actual cost as labour and fixed cost, as `evm::actual_parts` splits it.
fn parts(task: &Task, labour: &HashMap<u32, f64>) -> (f64, f64) {
    let actual = task.actual_cost.max(0.0);
    let done = labour
        .get(&task.uid)
        .copied()
        .unwrap_or(0.0)
        .clamp(0.0, actual);
    (done, actual - done)
}

/// A leaf's actual cost over its progress windows cut at `cut`, as AC spreads it.
fn spread_actual(
    project: &Project,
    task: &Task,
    labour: &HashMap<u32, f64>,
    cut: Status,
    keep_late: bool,
    add: &mut Sink,
) {
    let (labour, fixed) = parts(task, labour);
    if labour > 0.0 {
        spread_done(
            project,
            task,
            labour,
            Accrual::Prorated,
            cut,
            keep_late,
            add,
        );
    }
    if fixed > 0.0 {
        let accrual = task.fixed_cost_accrual;
        spread_done(project, task, fixed, accrual, cut, keep_late, add);
    }
}

/// What is left of a leaf's cost, over the work still to do (see the module header).
fn spread_remaining(
    project: &Project,
    task: &Task,
    labour: &HashMap<u32, f64>,
    status: Option<Status>,
    add: &mut Sink,
) {
    let left = (task.cost - task.actual_cost.max(0.0)).max(0.0);
    if left <= 0.0 {
        return;
    }
    let (_, fixed_done) = parts(task, labour);
    let fixed = task.fixed_cost.clamp(0.0, task.cost);
    let fixed_left = (fixed - fixed_done).max(0.0).min(left);
    let labour_left = left - fixed_left;

    // The work still to do: after the progress point and after the status date.
    let point = progress_intervals(project, task)
        .and_then(|windows| windows.iter().map(|(_, high)| *high).max());
    let begin = match (point, status) {
        (Some(p), Some((s, _))) => p.max(s),
        (Some(p), None) => p,
        (None, Some((s, _))) => s,
        (None, None) => i64::MIN,
    };
    let windows: Vec<(i64, i64)> = split::work_intervals(task)
        .into_iter()
        .map(|(low, high)| (low.max(begin), high))
        .filter(|(low, high)| high > low)
        .collect();
    let not_before = |instant: i64| status.map_or(instant, |(s, _)| instant.max(s));
    let fallback = civil(not_before(task.finish)).0;
    let calendar = project.task_calendar(task);
    match task.fixed_cost_accrual {
        Accrual::Start => add(civil(not_before(task.start)).0, fixed_left),
        Accrual::End => add(fallback, fixed_left),
        Accrual::Prorated => spread(calendar, &windows, fixed_left, fallback, add),
    }
    spread(calendar, &windows, labour_left, fallback, add);
}

/// The periods a baseline was planned to work: its segments, else its start to finish.
pub(crate) fn baseline_intervals(baseline: &Baseline) -> Vec<(i64, i64)> {
    if baseline.segments.is_empty() {
        vec![(baseline.start, baseline.finish)]
    } else {
        baseline
            .segments
            .iter()
            .map(|s| (s.start, s.finish))
            .collect()
    }
}

/// Spreads `amount` over the working minutes of `intervals`, day by day; with no working
/// minutes it all goes on `fallback`.
pub(crate) fn spread(
    calendar: &Calendar,
    intervals: &[(i64, i64)],
    amount: f64,
    fallback: NaiveDate,
    add: &mut Sink,
) {
    if amount == 0.0 {
        return;
    }
    let total = split::working_within(calendar, intervals, i64::MIN, i64::MAX);
    if total <= 0 {
        add(fallback, amount);
        return;
    }
    let (Some(first), Some(last)) = (
        intervals.iter().map(|(s, _)| *s).min(),
        intervals.iter().map(|(_, f)| *f).max(),
    ) else {
        return;
    };
    let (mut day, _) = civil(first);
    let (end, _) = civil(last);
    let mut accumulated = 0i64;
    let mut placed = 0.0f64;
    while let Some(next) = day.succ_opt() {
        accumulated +=
            split::working_within(calendar, intervals, compose(day, 0), compose(next, 0));
        let reached = amount * accumulated as f64 / total as f64;
        if reached > placed {
            add(day, reached - placed);
            placed = reached;
        }
        if day >= end {
            break;
        }
        day = next;
    }
}

/// Spreads by `accrual`: all at the start, all at the finish, or over the working time.
pub(crate) fn spread_accrued(
    calendar: &Calendar,
    accrual: Accrual,
    intervals: &[(i64, i64)],
    start: i64,
    finish: i64,
    amount: f64,
    add: &mut Sink,
) {
    match accrual {
        Accrual::Start => add(civil(start).0, amount),
        Accrual::End => add(civil(finish).0, amount),
        Accrual::Prorated => spread(calendar, intervals, amount, civil(finish).0, add),
    }
}

/// Spreads an amount done so far (actual work or cost, earned value) over the task's
/// progress windows cut at the status date: with the working time for prorated, on the
/// first day for start, on the last for end. With no working time in them it goes on
/// the actual start (or start) when that is not after the status date, or, with
/// `keep_late`, on the status date when it is.
pub(crate) fn spread_done(
    project: &Project,
    task: &Task,
    amount: f64,
    accrual: Accrual,
    (status, status_day): Status,
    keep_late: bool,
    add: &mut Sink,
) {
    let Some(intervals) = progress_intervals(project, task) else {
        return;
    };
    let cut: Vec<(i64, i64)> = intervals
        .iter()
        .map(|(low, high)| ((*low).min(status), (*high).min(status)))
        .filter(|(low, high)| high > low)
        .collect();
    let calendar = project.task_calendar(task);
    if split::working_within(calendar, &cut, i64::MIN, i64::MAX) <= 0 {
        let day = civil(task.actual_start.unwrap_or(task.start)).0;
        if day <= status_day {
            add(day, amount);
        } else if keep_late {
            add(status_day, amount);
        }
        return;
    }
    match accrual {
        Accrual::Prorated => spread(calendar, &cut, amount, status_day, add),
        Accrual::Start => {
            let first = cut.iter().map(|(low, _)| *low).min().unwrap_or(status);
            add(civil(first).0, amount);
        }
        Accrual::End => {
            let last = cut.iter().map(|(_, high)| *high).max().unwrap_or(status);
            add(civil(last).0, amount);
        }
    }
}
