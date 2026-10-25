//! S-curves and periodic curves calculation (PR 6, issue #150).
//!
//! Provides the engine read `curves(measure, bucket, filter, baseline)`:
//! - Measures:
//!   - `Cost`: Currency amount, planned (PV), scheduled, actual cost (AC), and earned value (EV).
//!   - `Work`: Working hours (minutes / 60.0), planned, scheduled, and actual work.
//!   - `Progress`: Percentage (0.0 to 100.0%) relative to total baseline work (or scheduled work without baseline).
//! - Buckets: `Day`, `Week`, or `Month` calendar periods.
//! - Filters:
//!   - `Project`: Whole project (all leaf tasks and assignments).
//!   - `Person`: A specific resource UID.
//!   - `Group` (Role): All resources belonging to a role/group name.
//!   - `Skill`: All resources possessing a skill ID (with optional minimum skill level rank).
//! - Series:
//!   - `baseline`: Planned curve from the chosen baseline number; none when no baseline is
//!     chosen (EV, which is measured against a baseline, is then left out too).
//!   - `scheduled`: Current early CPM schedule.
//!   - `actual`: Actual cost/work/progress up to the project status date (None after status date).
//!
//! Each bucket provides both the period value (for histogram/bar chart) and the cumulative value
//! (for S-curves), allowing instantaneous UI switching without re-computation.
//!
//! Costs follow the EVM panel: for the whole project a task's fixed cost is spread by the
//! task's accrual and its labour with the working time (`evm.rs` does the same for PV and
//! AC; a baseline's fixed part is `Baseline::fixed_part`). Filtered by people, each matching
//! assignment brings its own cost (`Assignment.cost`, `actual_cost`, and the cost saved in its
//! baseline), so the curves of all people add up to the labour part of the project's.
//!
//! A task baselined while it had people but whose assignments hold no baseline (files saved
//! before engine revision 7 kept none) is read from the task's own baseline instead: its work
//! is shared among its assignments by their current work.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::baseline_load::assignment_baseline_days;
use crate::calendar::Calendar;
use crate::error::EngineError;
use crate::evm::{actual_parts, progress_intervals};
use crate::load_map::{compute_buckets, Buckets};
use crate::model::{Accrual, Assignment, Baseline, Project, Task, UsageDay};
use crate::skill_levels::qualifies;
use crate::split;
use crate::timeutil::{civil, compose, parse_date_key};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurveMeasure {
    Cost,
    Work,
    Progress,
}

pub use crate::load_map::Bucket as CurveBucket;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveFilter {
    #[default]
    Project,
    Person {
        uid: u32,
    },
    #[serde(alias = "role")]
    Group {
        name: String,
    },
    Skill {
        id: u32,
        #[serde(default)]
        min_level: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvePoint {
    pub date: i64,
    pub label: String,
    pub baseline_period: Option<f64>,
    pub scheduled_period: f64,
    pub actual_period: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earned_period: Option<f64>,
    pub baseline_cumulative: Option<f64>,
    pub scheduled_cumulative: f64,
    pub actual_cumulative: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earned_cumulative: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CurvesReport {
    pub measure: CurveMeasure,
    pub bucket: CurveBucket,
    pub filter: CurveFilter,
    pub baseline: Option<u8>,
    pub status_date: Option<i64>,
    pub baseline_finish: Option<i64>,
    pub scheduled_finish: Option<i64>,
    pub total_baseline: Option<f64>,
    pub total_scheduled: f64,
    pub total_actual: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_earned: Option<f64>,
    pub points: Vec<CurvePoint>,
}

/// Helper to get baseline for a task by baseline number.
fn task_baseline(task: &Task, baseline_number: u8) -> Option<&Baseline> {
    task.baselines.iter().find(|b| b.number == baseline_number)
}

/// The periods a baseline was planned to work: its segments, else its start to finish.
fn baseline_intervals(baseline: &Baseline) -> Vec<(i64, i64)> {
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

/// Collects target resource UIDs matching the given filter.
fn resolve_filter_resources(project: &Project, filter: &CurveFilter) -> Option<HashSet<u32>> {
    match filter {
        CurveFilter::Project => None,
        CurveFilter::Person { uid } => Some(HashSet::from([*uid])),
        CurveFilter::Group { name } => {
            let uids = project
                .resources
                .iter()
                .filter(|r| r.group == *name)
                .map(|r| r.uid)
                .collect();
            Some(uids)
        }
        CurveFilter::Skill { id, min_level } => {
            let uids = project
                .resources
                .iter()
                .filter(|r| qualifies(project, r, *id, *min_level))
                .map(|r| r.uid)
                .collect();
            Some(uids)
        }
    }
}

/// What the curves read, built once: lookups by uid and the chosen baseline.
struct Context<'a> {
    project: &'a Project,
    buckets: &'a Buckets,
    /// The people of the filter; `None` for the whole project.
    people: Option<&'a HashSet<u32>>,
    /// The chosen baseline, when one is chosen and some task holds it.
    baseline: Option<u8>,
    status: Option<(i64, NaiveDate)>,
    tasks: HashMap<u32, &'a Task>,
    /// Leaf tasks with the chosen baseline none of whose assignments holds it: their
    /// baseline is read from the task's own (see the module header).
    task_level_baseline: HashSet<u32>,
    /// The current work of each task's assignments, for sharing a task-level baseline.
    assigned_work: HashMap<u32, (i64, usize)>,
}

impl<'a> Context<'a> {
    fn new(
        project: &'a Project,
        buckets: &'a Buckets,
        people: Option<&'a HashSet<u32>>,
        baseline: Option<u8>,
    ) -> Self {
        let tasks: HashMap<u32, &Task> = project.tasks.iter().map(|t| (t.uid, t)).collect();
        let mut assigned_work: HashMap<u32, (i64, usize)> = HashMap::new();
        let mut with_assignment_baseline = HashSet::new();
        for assignment in &project.assignments {
            let entry = assigned_work.entry(assignment.task_uid).or_default();
            entry.0 += assignment.work.max(0);
            entry.1 += 1;
            if baseline.is_some_and(|n| assignment.baselines.iter().any(|b| b.number == n)) {
                with_assignment_baseline.insert(assignment.task_uid);
            }
        }
        let task_level_baseline = match baseline {
            Some(n) => project
                .tasks
                .iter()
                .filter(|t| !t.summary && task_baseline(t, n).is_some())
                .filter(|t| !with_assignment_baseline.contains(&t.uid))
                .map(|t| t.uid)
                .collect(),
            None => HashSet::new(),
        };
        Self {
            project,
            buckets,
            people,
            baseline,
            status: project.status_date.map(|s| (s, civil(s).0)),
            tasks,
            task_level_baseline,
            assigned_work,
        }
    }

    fn leaves(&self) -> impl Iterator<Item = &'a Task> {
        self.project.tasks.iter().filter(|t| !t.summary)
    }

    /// The assignments of the filter's people, with their task.
    fn filtered_assignments(&self) -> impl Iterator<Item = (&'a Assignment, &'a Task)> + '_ {
        let people = self.people;
        self.project
            .assignments
            .iter()
            .filter(move |a| people.is_some_and(|set| set.contains(&a.resource_uid)))
            .filter_map(|a| self.tasks.get(&a.task_uid).map(|task| (a, *task)))
    }

    fn add(&self, target: &mut [f64], day: NaiveDate, amount: f64) {
        if let Some(idx) = (self.buckets.index_of)(day) {
            target[idx] += amount;
        }
    }

    /// Spreads `amount` over the working minutes of `intervals`, day by day; with no working
    /// minutes it all goes on `fallback`.
    fn spread(
        &self,
        calendar: &Calendar,
        intervals: &[(i64, i64)],
        amount: f64,
        fallback: NaiveDate,
        target: &mut [f64],
    ) {
        let total = split::working_within(calendar, intervals, i64::MIN, i64::MAX);
        if total <= 0 {
            self.add(target, fallback, amount);
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
                self.add(target, day, reached - placed);
                placed = reached;
            }
            if day >= end {
                break;
            }
            day = next;
        }
    }

    /// Spreads by `accrual`: all at the start, all at the finish, or over the working time.
    #[allow(clippy::too_many_arguments)]
    fn spread_accrued(
        &self,
        calendar: &Calendar,
        accrual: Accrual,
        intervals: &[(i64, i64)],
        start: i64,
        finish: i64,
        amount: f64,
        target: &mut [f64],
    ) {
        match accrual {
            Accrual::Start => self.add(target, civil(start).0, amount),
            Accrual::End => self.add(target, civil(finish).0, amount),
            Accrual::Prorated => self.spread(calendar, intervals, amount, civil(finish).0, target),
        }
    }

    /// Spreads an amount done so far (actual work or cost, earned value) over the task's
    /// progress windows cut at the status date: with the working time for prorated, on the
    /// first day for start, on the last for end. With no working time in them it goes on
    /// the actual start (or start) when that is not after the status date.
    fn spread_done(&self, task: &Task, amount: f64, accrual: Accrual, target: &mut [f64]) {
        let Some((status, status_day)) = self.status else {
            return;
        };
        let Some(intervals) = progress_intervals(self.project, task) else {
            return;
        };
        let cut: Vec<(i64, i64)> = intervals
            .iter()
            .map(|(low, high)| ((*low).min(status), (*high).min(status)))
            .filter(|(low, high)| high > low)
            .collect();
        let calendar = self.project.task_calendar(task);
        if split::working_within(calendar, &cut, i64::MIN, i64::MAX) <= 0 {
            let day = civil(task.actual_start.unwrap_or(task.start)).0;
            if day <= status_day {
                self.add(target, day, amount);
            }
            return;
        }
        match accrual {
            Accrual::Prorated => self.spread(calendar, &cut, amount, status_day, target),
            Accrual::Start => {
                let first = cut.iter().map(|(low, _)| *low).min().unwrap_or(status);
                self.add(target, civil(first).0, amount);
            }
            Accrual::End => {
                let last = cut.iter().map(|(_, high)| *high).max().unwrap_or(status);
                self.add(target, civil(last).0, amount);
            }
        }
    }

    /// An assignment's baseline: its work by day (minutes) and its cost. A task read at task
    /// level shares the task's baseline work by the assignments' current work.
    fn assignment_baseline(
        &self,
        assignment: &Assignment,
        task: &Task,
    ) -> Option<(Vec<(NaiveDate, f64)>, f64)> {
        let n = self.baseline?;
        let task_base = task_baseline(task, n)?;
        let rate = effective_rate(self.project, assignment);
        if self.task_level_baseline.contains(&task.uid) {
            let (work_sum, count) = self.assigned_work.get(&task.uid).copied().unwrap_or((0, 1));
            let share = if work_sum > 0 {
                assignment.work.max(0) as f64 / work_sum as f64
            } else {
                1.0 / count.max(1) as f64
            };
            let work = task_base.work.max(0) as f64 * share;
            if work <= 0.0 {
                return None;
            }
            let mut days = vec![0.0; self.buckets.count];
            let calendar = self.project.task_calendar(task);
            let fallback = civil(task_base.finish).0;
            self.spread(
                calendar,
                &baseline_intervals(task_base),
                work,
                fallback,
                &mut days,
            );
            let per_bucket = days
                .iter()
                .enumerate()
                .filter(|(_, m)| **m > 0.0)
                .map(|(i, m)| (bucket_day(self.buckets, i), *m))
                .collect();
            return Some((per_bucket, work / 60.0 * rate));
        }
        let saved = assignment.baselines.iter().find(|b| b.number == n)?;
        let days: Vec<(NaiveDate, f64)> =
            assignment_baseline_days(self.project, task, assignment, n)
                .into_iter()
                .map(|(d, m)| (d, m as f64))
                .collect();
        let cost = if saved.cost > 0.0 {
            saved.cost
        } else {
            saved.work.max(0) as f64 / 60.0 * rate
        };
        Some((days, cost))
    }
}

/// The first day of bucket `index` (its key).
fn bucket_day(buckets: &Buckets, index: usize) -> NaiveDate {
    parse_date_key(&buckets.keys[index]).unwrap_or(buckets.first)
}

/// What an hour of the assignment costs now: its cost over its work, else the resource's
/// standard rate. Used for baselines that kept no cost.
fn effective_rate(project: &Project, assignment: &Assignment) -> f64 {
    if assignment.work > 0 && assignment.cost > 0.0 {
        return assignment.cost / (assignment.work as f64 / 60.0);
    }
    project
        .resources
        .iter()
        .find(|r| r.uid == assignment.resource_uid)
        .map_or(0.0, |r| r.standard_rate)
}

/// Calculates S-curve and periodic values according to measure, bucket, filter, and baseline.
/// `baseline` None draws no baseline (and no earned value).
pub fn curves(
    project: &Project,
    measure: CurveMeasure,
    bucket: CurveBucket,
    filter: CurveFilter,
    baseline: Option<u8>,
) -> Result<CurvesReport, EngineError> {
    let target_resources = resolve_filter_resources(project, &filter);
    let baseline = baseline.filter(|n| {
        project
            .tasks
            .iter()
            .filter(|t| !t.summary)
            .any(|t| task_baseline(t, *n).is_some())
    });

    // 1. Determine timeline boundaries (min_start and max_finish).
    let mut min_start: Option<i64> = None;
    let mut max_finish: Option<i64> = None;
    let mut baseline_finish_ts: Option<i64> = None;
    let mut scheduled_finish_ts: Option<i64> = None;
    let mut widen = |from: i64, to: i64| {
        min_start = Some(min_start.map_or(from, |s| s.min(from)));
        max_finish = Some(max_finish.map_or(to, |f| f.max(to)));
    };

    for task in project.tasks.iter().filter(|t| !t.summary) {
        let task_matches = target_resources.as_ref().is_none_or(|res_set| {
            project
                .assignments
                .iter()
                .any(|a| a.task_uid == task.uid && res_set.contains(&a.resource_uid))
        });
        if !task_matches {
            continue;
        }
        widen(task.start, task.finish);
        scheduled_finish_ts = Some(scheduled_finish_ts.map_or(task.finish, |f| f.max(task.finish)));
        if let Some(b) = baseline.and_then(|n| task_baseline(task, n)) {
            widen(b.start, b.finish);
            baseline_finish_ts = Some(baseline_finish_ts.map_or(b.finish, |f| f.max(b.finish)));
        }
        if let Some(start) = task.actual_start {
            widen(start, start);
        }
        if let Some(finish) = task.actual_finish {
            widen(finish, finish);
        }
    }
    if let (Some(status), true) = (project.status_date, scheduled_finish_ts.is_some()) {
        widen(status, status);
    }

    let (Some(start_ts), Some(finish_ts)) = (min_start, max_finish) else {
        return Ok(CurvesReport {
            measure,
            bucket,
            filter,
            baseline,
            status_date: project.status_date,
            baseline_finish: None,
            scheduled_finish: None,
            total_baseline: None,
            total_scheduled: 0.0,
            total_actual: None,
            total_earned: None,
            points: Vec::new(),
        });
    };

    let from_date = civil(start_ts).0;
    let buckets = compute_buckets(from_date, civil(finish_ts).0, bucket)?;
    let count = buckets.count;
    let cx = Context::new(project, &buckets, target_resources.as_ref(), baseline);

    let mut scheduled_periods = vec![0.0f64; count];
    let mut baseline_periods = baseline.map(|_| vec![0.0f64; count]);
    let mut actual_periods = project.status_date.map(|_| vec![0.0f64; count]);
    let mut earned_periods =
        (baseline.is_some() && project.status_date.is_some() && measure == CurveMeasure::Cost)
            .then(|| vec![0.0f64; count]);

    match measure {
        CurveMeasure::Work | CurveMeasure::Progress => {
            work_periods(
                &cx,
                &mut scheduled_periods,
                &mut baseline_periods,
                &mut actual_periods,
            );
            if measure == CurveMeasure::Progress {
                to_percent(
                    &mut scheduled_periods,
                    &mut baseline_periods,
                    &mut actual_periods,
                );
            }
        }
        CurveMeasure::Cost => cost_periods(
            &cx,
            &mut scheduled_periods,
            &mut baseline_periods,
            &mut actual_periods,
            &mut earned_periods,
        ),
    }

    // Build cumulative series and final points.
    let status_day = cx.status.map(|(_, day)| day);
    let mut scheduled_cum = 0.0f64;
    let mut baseline_cum = 0.0f64;
    let mut actual_cum = 0.0f64;
    let mut earned_cum = 0.0f64;
    let mut points = Vec::with_capacity(count);

    for i in 0..count {
        let bucket_date = bucket_day(&buckets, i);
        // Actual values only exist up to the status date's bucket.
        let is_past_status = status_day.is_some_and(|sd| bucket_date > sd);

        let sched_p = scheduled_periods[i];
        scheduled_cum += sched_p;
        let base_p = baseline_periods.as_ref().map(|v| v[i]);
        if let Some(bp) = base_p {
            baseline_cum += bp;
        }
        let act_p = actual_periods
            .as_ref()
            .filter(|_| !is_past_status)
            .map(|v| v[i]);
        if let Some(ap) = act_p {
            actual_cum += ap;
        }
        let earn_p = earned_periods
            .as_ref()
            .filter(|_| !is_past_status)
            .map(|v| v[i]);
        if let Some(ep) = earn_p {
            earned_cum += ep;
        }

        points.push(CurvePoint {
            date: compose(bucket_date, 0),
            label: buckets.keys[i].clone(),
            baseline_period: base_p,
            scheduled_period: sched_p,
            actual_period: act_p,
            earned_period: earn_p,
            baseline_cumulative: base_p.map(|_| baseline_cum),
            scheduled_cumulative: scheduled_cum,
            actual_cumulative: act_p.map(|_| actual_cum),
            earned_cumulative: earn_p.map(|_| earned_cum),
        });
    }

    Ok(CurvesReport {
        measure,
        bucket,
        filter,
        baseline,
        status_date: project.status_date,
        baseline_finish: baseline_finish_ts,
        scheduled_finish: scheduled_finish_ts,
        total_baseline: baseline_periods.as_ref().map(|_| baseline_cum),
        total_scheduled: scheduled_cum,
        total_actual: actual_periods.as_ref().map(|_| actual_cum),
        total_earned: earned_periods.as_ref().map(|_| earned_cum),
        points,
    })
}

/// Work hours by bucket for the scheduled, baseline and actual series.
fn work_periods(
    cx: &Context,
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
) {
    let project = cx.project;

    // Scheduled: the usage rows, a task's row for the project, the assignments' when filtered.
    for row in &project.usage {
        let is_target = match cx.people {
            None => row.assignment_uid.is_none(),
            Some(set) => row.resource_uid.is_some_and(|uid| set.contains(&uid)),
        };
        if !is_target {
            continue;
        }
        for day in &row.days {
            if let Some(d) = parse_date_key(&day.date) {
                cx.add(scheduled, d, day.hours);
            }
        }
    }

    // Baseline.
    if let (Some(n), Some(base)) = (cx.baseline, baseline.as_mut()) {
        if cx.people.is_some() {
            for (assignment, task) in cx.filtered_assignments() {
                if let Some((days, _)) = cx.assignment_baseline(assignment, task) {
                    for (d, minutes) in days {
                        cx.add(base, d, minutes / 60.0);
                    }
                }
            }
        } else {
            for assignment in &project.assignments {
                let Some(task) = cx.tasks.get(&assignment.task_uid) else {
                    continue;
                };
                if cx.task_level_baseline.contains(&task.uid) {
                    continue;
                }
                for (d, minutes) in assignment_baseline_days(project, task, assignment, n) {
                    cx.add(base, d, minutes as f64 / 60.0);
                }
            }
            // Tasks with no assignment baseline (unassigned, or baselined before revision 7).
            for task in cx
                .leaves()
                .filter(|t| cx.task_level_baseline.contains(&t.uid))
            {
                let Some(b) = task_baseline(task, n) else {
                    continue;
                };
                if b.work > 0 {
                    let calendar = project.task_calendar(task);
                    let hours = b.work as f64 / 60.0;
                    cx.spread(
                        calendar,
                        &baseline_intervals(b),
                        hours,
                        civil(b.finish).0,
                        base,
                    );
                }
            }
        }
    }

    // Actual.
    if let Some(act) = actual.as_mut() {
        if cx.people.is_some() {
            for (assignment, task) in cx.filtered_assignments() {
                if assignment.actual_work > 0 {
                    cx.spread_done(
                        task,
                        assignment.actual_work as f64 / 60.0,
                        Accrual::Prorated,
                        act,
                    );
                }
            }
        } else {
            for task in cx.leaves().filter(|t| t.actual_work > 0) {
                cx.spread_done(task, task.actual_work as f64 / 60.0, Accrual::Prorated, act);
            }
        }
    }
}

/// Costs by bucket for the scheduled, baseline, actual and earned series.
fn cost_periods(
    cx: &Context,
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
    earned: &mut Option<Vec<f64>>,
) {
    let project = cx.project;

    if cx.people.is_some() {
        // Each assignment's cost spread like its hours in the usage rows.
        let hours: HashMap<u32, &[UsageDay]> = project
            .usage
            .iter()
            .filter_map(|row| row.assignment_uid.map(|uid| (uid, row.days.as_slice())))
            .collect();
        for (assignment, task) in cx.filtered_assignments() {
            if assignment.cost > 0.0 {
                let days = hours.get(&assignment.uid).copied().unwrap_or(&[]);
                let total: f64 = days.iter().map(|d| d.hours).sum();
                if total > 0.0 {
                    for day in days {
                        if let Some(d) = parse_date_key(&day.date) {
                            cx.add(scheduled, d, assignment.cost * day.hours / total);
                        }
                    }
                } else {
                    cx.add(scheduled, civil(task.start).0, assignment.cost);
                }
            }
            let base = cx.assignment_baseline(assignment, task);
            if let (Some(target), Some((days, cost))) = (baseline.as_mut(), base.as_ref()) {
                let minutes: f64 = days.iter().map(|(_, m)| m).sum();
                if minutes > 0.0 {
                    for (d, m) in days {
                        cx.add(target, *d, cost * m / minutes);
                    }
                }
            }
            if let Some(target) = actual.as_mut() {
                if assignment.actual_cost > 0.0 {
                    cx.spread_done(task, assignment.actual_cost, Accrual::Prorated, target);
                }
            }
            if let (Some(target), Some((_, cost))) = (earned.as_mut(), base) {
                let pct = task.percent_complete.clamp(0.0, 100.0);
                if pct > 0.0 && cost > 0.0 {
                    cx.spread_done(task, cost * pct / 100.0, Accrual::Prorated, target);
                }
            }
        }
        return;
    }

    // The accrual is the fixed cost's; labour is spent with the working time.
    for task in cx.leaves() {
        let calendar = project.task_calendar(task);
        let accrual = task.fixed_cost_accrual;
        if task.cost > 0.0 {
            let intervals = split::work_intervals(task);
            let fixed = task.fixed_cost.clamp(0.0, task.cost);
            let (start, finish) = (task.start, task.finish);
            cx.spread_accrued(
                calendar, accrual, &intervals, start, finish, fixed, scheduled,
            );
            let labour = task.cost - fixed;
            cx.spread(calendar, &intervals, labour, civil(finish).0, scheduled);
        }
        let base = cx
            .baseline
            .and_then(|n| task_baseline(task, n))
            .filter(|b| b.cost > 0.0);
        if let (Some(target), Some(b)) = (baseline.as_mut(), base) {
            let intervals = baseline_intervals(b);
            let fixed = b.fixed_part(task);
            cx.spread_accrued(
                calendar, accrual, &intervals, b.start, b.finish, fixed, target,
            );
            cx.spread(
                calendar,
                &intervals,
                b.cost - fixed,
                civil(b.finish).0,
                target,
            );
        }
        if let Some(target) = actual.as_mut() {
            let (labour, fixed) = actual_parts(project, task);
            if labour > 0.0 {
                cx.spread_done(task, labour, Accrual::Prorated, target);
            }
            if fixed > 0.0 {
                cx.spread_done(task, fixed, accrual, target);
            }
        }
        if let (Some(target), Some(b)) = (earned.as_mut(), base) {
            let pct = task.percent_complete.clamp(0.0, 100.0);
            if pct > 0.0 {
                cx.spread_done(task, b.cost * pct / 100.0, Accrual::Prorated, target);
            }
        }
    }
}

/// Turns work hours into percent of the total: the baseline's when there is one with work,
/// else the scheduled work.
fn to_percent(
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
) {
    let total_baseline: f64 = baseline.as_ref().map_or(0.0, |v| v.iter().sum());
    let total = if total_baseline > 0.0 {
        total_baseline
    } else {
        scheduled.iter().sum()
    };
    if total <= 0.0 {
        return;
    }
    let factor = 100.0 / total;
    let scale = |values: &mut [f64]| values.iter_mut().for_each(|v| *v *= factor);
    scale(scheduled);
    if let Some(v) = baseline.as_mut() {
        scale(v);
    }
    if let Some(v) = actual.as_mut() {
        scale(v);
    }
}
