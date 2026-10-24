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
//!   - `baseline`: Planned curve from specified baseline number (defaulting to baseline 0).
//!   - `scheduled`: Current early CPM schedule.
//!   - `actual`: Actual cost/work/progress up to the project status date (None after status date).
//!
//! Each bucket provides both the period value (for histogram/bar chart) and the cumulative value
//! (for S-curves), allowing instantaneous UI switching without re-computation.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::baseline_load::{
    assignment_baseline_days, baseline_load_by_resource, project_baseline_load_by_day,
};
use crate::error::EngineError;
use crate::evm::progress_intervals;
use crate::load_map::{compute_buckets, Buckets};
use crate::model::{Accrual, Baseline, Project, Task};
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

/// Calculates S-curve and periodic values according to measure, bucket, filter, and baseline.
pub fn curves(
    project: &Project,
    measure: CurveMeasure,
    bucket: CurveBucket,
    filter: CurveFilter,
    baseline: Option<u8>,
) -> Result<CurvesReport, EngineError> {
    let target_resources = resolve_filter_resources(project, &filter);
    let baseline_num = baseline.unwrap_or(0);

    // 1. Determine timeline boundaries (min_start and max_finish).
    let mut min_start: Option<i64> = None;
    let mut max_finish: Option<i64> = None;
    let mut baseline_finish_ts: Option<i64> = None;
    let mut scheduled_finish_ts: Option<i64> = None;

    let has_any_baseline = project
        .tasks
        .iter()
        .filter(|t| !t.summary)
        .any(|t| task_baseline(t, baseline_num).is_some());

    for task in project.tasks.iter().filter(|t| !t.summary) {
        let task_matches = if let Some(ref res_set) = target_resources {
            project
                .assignments
                .iter()
                .any(|a| a.task_uid == task.uid && res_set.contains(&a.resource_uid))
        } else {
            true
        };

        if !task_matches {
            continue;
        }

        min_start = Some(min_start.map_or(task.start, |s| s.min(task.start)));
        max_finish = Some(max_finish.map_or(task.finish, |f| f.max(task.finish)));
        scheduled_finish_ts =
            Some(scheduled_finish_ts.map_or(task.finish, |f| f.max(task.finish)));

        if let Some(b) = task_baseline(task, baseline_num) {
            min_start = Some(min_start.map_or(b.start, |s| s.min(b.start)));
            max_finish = Some(max_finish.map_or(b.finish, |f| f.max(b.finish)));
            baseline_finish_ts =
                Some(baseline_finish_ts.map_or(b.finish, |f| f.max(b.finish)));
        }

        if let Some(start) = task.actual_start {
            min_start = Some(min_start.map_or(start, |s| s.min(start)));
        }
        if let Some(finish) = task.actual_finish {
            max_finish = Some(max_finish.map_or(finish, |f| f.max(finish)));
        }
    }

    if let Some(status) = project.status_date {
        if min_start.is_some() {
            min_start = Some(min_start.unwrap().min(status));
            max_finish = Some(max_finish.unwrap().max(status));
        }
    }

    let (Some(start_ts), Some(finish_ts)) = (min_start, max_finish) else {
        return Ok(CurvesReport {
            measure,
            bucket,
            filter,
            baseline: if has_any_baseline { Some(baseline_num) } else { None },
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
    let to_date = civil(finish_ts).0;

    let buckets = compute_buckets(from_date, to_date, bucket)?;
    let count = buckets.count;

    // Daily distribution buffers before aggregating into buckets.
    let mut scheduled_periods = vec![0.0f64; count];
    let mut baseline_periods = if has_any_baseline {
        Some(vec![0.0f64; count])
    } else {
        None
    };
    let mut actual_periods = if project.status_date.is_some() {
        Some(vec![0.0f64; count])
    } else {
        None
    };
    let mut earned_periods = if has_any_baseline && project.status_date.is_some() && measure == CurveMeasure::Cost {
        Some(vec![0.0f64; count])
    } else {
        None
    };

    let status_day = project.status_date.map(|s| civil(s).0);

    // Compute period distributions based on measure.
    match measure {
        CurveMeasure::Work => {
            compute_work_periods(
                project,
                &buckets,
                target_resources.as_ref(),
                baseline_num,
                has_any_baseline,
                status_day,
                &mut scheduled_periods,
                &mut baseline_periods,
                &mut actual_periods,
            );
        }
        CurveMeasure::Cost => {
            compute_cost_periods(
                project,
                &buckets,
                target_resources.as_ref(),
                baseline_num,
                has_any_baseline,
                status_day,
                &mut scheduled_periods,
                &mut baseline_periods,
                &mut actual_periods,
                &mut earned_periods,
            );
        }
        CurveMeasure::Progress => {
            compute_progress_periods(
                project,
                &buckets,
                target_resources.as_ref(),
                baseline_num,
                has_any_baseline,
                status_day,
                &mut scheduled_periods,
                &mut baseline_periods,
                &mut actual_periods,
            );
        }
    }

    // Build cumulative series and final points.
    let mut scheduled_cum = 0.0f64;
    let mut baseline_cum = 0.0f64;
    let mut actual_cum = 0.0f64;
    let mut earned_cum = 0.0f64;

    let mut points = Vec::with_capacity(count);

    for i in 0..count {
        let date_key_str = &buckets.keys[i];
        let bucket_date = parse_date_key(date_key_str).unwrap_or(from_date);
        let bucket_ts = compose(bucket_date, 0);

        let sched_p = scheduled_periods[i];
        scheduled_cum += sched_p;

        let base_p = baseline_periods.as_ref().map(|v| v[i]);
        if let Some(bp) = base_p {
            baseline_cum += bp;
        }

        // Actual values only exist up to the status date's bucket.
        let is_past_status = status_day.is_some_and(|sd| bucket_date > sd);
        let act_p = if is_past_status {
            None
        } else {
            actual_periods.as_ref().map(|v| v[i])
        };
        if let Some(ap) = act_p {
            actual_cum += ap;
        }

        let earn_p = if is_past_status {
            None
        } else {
            earned_periods.as_ref().map(|v| v[i])
        };
        if let Some(ep) = earn_p {
            earned_cum += ep;
        }

        points.push(CurvePoint {
            date: bucket_ts,
            label: date_key_str.clone(),
            baseline_period: base_p,
            scheduled_period: sched_p,
            actual_period: act_p,
            earned_period: earn_p,
            baseline_cumulative: base_p.map(|_| baseline_cum),
            scheduled_cumulative: scheduled_cum,
            actual_cumulative: if is_past_status { None } else { actual_periods.as_ref().map(|_| actual_cum) },
            earned_cumulative: if is_past_status { None } else { earned_periods.as_ref().map(|_| earned_cum) },
        });
    }

    let total_baseline = baseline_periods.as_ref().map(|_| baseline_cum);
    let total_actual = actual_periods.as_ref().map(|_| actual_cum);
    let total_earned = earned_periods.as_ref().map(|_| earned_cum);

    Ok(CurvesReport {
        measure,
        bucket,
        filter,
        baseline: if has_any_baseline { Some(baseline_num) } else { None },
        status_date: project.status_date,
        baseline_finish: baseline_finish_ts,
        scheduled_finish: scheduled_finish_ts,
        total_baseline,
        total_scheduled: scheduled_cum,
        total_actual,
        total_earned,
        points,
    })
}

/// Distributes work hours across buckets for Scheduled, Baseline, and Actual series.
#[allow(clippy::too_many_arguments)]
fn compute_work_periods(
    project: &Project,
    buckets: &Buckets,
    target_resources: Option<&HashSet<u32>>,
    baseline_num: u8,
    has_any_baseline: bool,
    status_day: Option<NaiveDate>,
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
) {
    // 1. Scheduled Work from project.usage (in hours)
    for row in &project.usage {
        let is_target = match target_resources {
            None => row.assignment_uid.is_none(),
            Some(res_set) => row.resource_uid.is_some_and(|uid| res_set.contains(&uid)),
        };
        if !is_target {
            continue;
        }
        for day in &row.days {
            if let Some(d) = parse_date_key(&day.date) {
                if let Some(idx) = (buckets.index_of)(d) {
                    scheduled[idx] += day.hours;
                }
            }
        }
    }

    // 2. Baseline Work (converted from minutes to hours)
    if has_any_baseline {
        if let Some(ref mut base_vec) = baseline {
            if let Some(res_set) = target_resources {
                let load_map = baseline_load_by_resource(project, baseline_num);
                for (&uid, days) in &load_map {
                    if res_set.contains(&uid) {
                        for (&d, &minutes) in days {
                            if let Some(idx) = (buckets.index_of)(d) {
                                base_vec[idx] += minutes as f64 / 60.0;
                            }
                        }
                    }
                }
            } else {
                let assigned_task_uids: HashSet<u32> =
                    project.assignments.iter().map(|a| a.task_uid).collect();
                let load_map = project_baseline_load_by_day(project, baseline_num);
                for (d, minutes) in load_map {
                    if let Some(idx) = (buckets.index_of)(d) {
                        base_vec[idx] += minutes as f64 / 60.0;
                    }
                }
                for task in project.tasks.iter().filter(|t| !t.summary) {
                    if assigned_task_uids.contains(&task.uid) {
                        continue;
                    }
                    let Some(b) = task_baseline(task, baseline_num) else {
                        continue;
                    };
                    if b.work <= 0 {
                        continue;
                    }
                    let calendar = project.task_calendar(task);
                    let intervals: Vec<(i64, i64)> = if !b.segments.is_empty() {
                        b.segments.iter().map(|s| (s.start, s.finish)).collect()
                    } else if b.finish > b.start {
                        vec![(b.start, b.finish)]
                    } else {
                        continue;
                    };
                    let total_task_mins =
                        split::working_within(calendar, &intervals, i64::MIN, i64::MAX);
                    if total_task_mins <= 0 {
                        continue;
                    }
                    let min_start = intervals.iter().map(|(s, _)| *s).min().unwrap_or(b.start);
                    let max_finish = intervals.iter().map(|(_, f)| *f).max().unwrap_or(b.finish);
                    let (mut d, _) = civil(min_start);
                    let (end_d, _) = civil(max_finish);
                    let mut accumulated = 0i64;
                    let mut prev_work = 0i64;
                    while let Some(next) = d.succ_opt() {
                        let day_mins = split::working_within(
                            calendar,
                            &intervals,
                            compose(d, 0),
                            compose(next, 0),
                        );
                        accumulated += day_mins;
                        let cur_work =
                            ((b.work as f64 * accumulated as f64) / total_task_mins as f64).round() as i64;
                        let day_work = cur_work - prev_work;
                        prev_work = cur_work;
                        if day_work > 0 {
                            if let Some(idx) = (buckets.index_of)(d) {
                                base_vec[idx] += day_work as f64 / 60.0;
                            }
                        }
                        if d >= end_d {
                            break;
                        }
                        d = next;
                    }
                }
            }
        }
    }

    // 3. Actual Work (converted from minutes to hours)
    if let (Some(status_date_ts), Some(status_d)) = (project.status_date, status_day) {
        if let Some(ref mut act_vec) = actual {
            let tasks: HashMap<u32, &Task> = project.tasks.iter().map(|t| (t.uid, t)).collect();

            if let Some(res_set) = target_resources {
                for assignment in &project.assignments {
                    if !res_set.contains(&assignment.resource_uid) || assignment.actual_work <= 0 {
                        continue;
                    }
                    let Some(task) = tasks.get(&assignment.task_uid) else {
                        continue;
                    };
                    distribute_actual_work(
                        project,
                        task,
                        assignment.actual_work,
                        status_date_ts,
                        status_d,
                        buckets,
                        act_vec,
                    );
                }
            } else {
                for task in project.tasks.iter().filter(|t| !t.summary) {
                    if task.actual_work <= 0 {
                        continue;
                    }
                    distribute_actual_work(
                        project,
                        task,
                        task.actual_work,
                        status_date_ts,
                        status_d,
                        buckets,
                        act_vec,
                    );
                }
            }
        }
    }
}

/// Distributes actual work proportionally over progress intervals up to status date.
fn distribute_actual_work(
    project: &Project,
    task: &Task,
    work_minutes: i64,
    status_date: i64,
    status_day: NaiveDate,
    buckets: &Buckets,
    target: &mut [f64],
) {
    let Some(intervals) = progress_intervals(project, task) else {
        return;
    };
    let calendar = project.task_calendar(task);
    let cut: Vec<(i64, i64)> = intervals
        .iter()
        .map(|(low, high)| ((*low).min(status_date), (*high).min(status_date)))
        .filter(|(low, high)| high > low)
        .collect();

    let total_working_mins = split::working_within(calendar, &cut, i64::MIN, i64::MAX);
    if total_working_mins <= 0 {
        // Book at actual start or task start if no working time elapsed
        let d = civil(task.actual_start.unwrap_or(task.start)).0;
        if d <= status_day {
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += work_minutes as f64 / 60.0;
            }
        }
        return;
    }

    let min_start = cut.iter().map(|(s, _)| *s).min().unwrap_or(task.start);
    let max_finish = cut.iter().map(|(_, f)| *f).max().unwrap_or(task.finish);

    let (mut d, _) = civil(min_start);
    let (end_d, _) = civil(max_finish);

    let mut accumulated = 0i64;
    let mut prev_work = 0i64;

    while let Some(next) = d.succ_opt() {
        if d > status_day {
            break;
        }
        let day_working = split::working_within(calendar, &cut, compose(d, 0), compose(next, 0));
        accumulated += day_working;

        let cur_work = ((work_minutes as f64 * accumulated as f64) / total_working_mins as f64).round() as i64;
        let day_work = cur_work - prev_work;
        prev_work = cur_work;

        if day_work > 0 {
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += day_work as f64 / 60.0;
            }
        }

        if d >= end_d {
            break;
        }
        d = next;
    }
}

/// Distributes costs across buckets for Scheduled, Baseline, Actual, and Earned series.
#[allow(clippy::too_many_arguments)]
fn compute_cost_periods(
    project: &Project,
    buckets: &Buckets,
    target_resources: Option<&HashSet<u32>>,
    baseline_num: u8,
    has_any_baseline: bool,
    status_day: Option<NaiveDate>,
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
    earned: &mut Option<Vec<f64>>,
) {
    let tasks: HashMap<u32, &Task> = project.tasks.iter().map(|t| (t.uid, t)).collect();
    let rates: HashMap<u32, f64> = project.resources.iter().map(|r| (r.uid, r.standard_rate)).collect();

    // 1. Scheduled Cost
    if let Some(res_set) = target_resources {
        for row in &project.usage {
            if let Some(uid) = row.resource_uid {
                if res_set.contains(&uid) {
                    let rate = rates.get(&uid).copied().unwrap_or(0.0);
                    for day in &row.days {
                        if let Some(d) = parse_date_key(&day.date) {
                            if let Some(idx) = (buckets.index_of)(d) {
                                scheduled[idx] += day.hours * rate;
                            }
                        }
                    }
                }
            }
        }
    } else {
        for task in project.tasks.iter().filter(|t| !t.summary) {
            let total_cost = task.cost;
            if total_cost <= 0.0 {
                continue;
            }
            distribute_task_cost(project, task, total_cost, task.fixed_cost_accrual, buckets, scheduled);
        }
    }

    // 2. Baseline Cost
    if has_any_baseline {
        if let Some(ref mut base_vec) = baseline {
            if let Some(res_set) = target_resources {
                for assignment in &project.assignments {
                    if !res_set.contains(&assignment.resource_uid) {
                        continue;
                    }
                    let Some(task) = tasks.get(&assignment.task_uid) else {
                        continue;
                    };
                    let rate = rates.get(&assignment.resource_uid).copied().unwrap_or(0.0);
                    let days = assignment_baseline_days(project, task, assignment, baseline_num);
                    for (d, minutes) in days {
                        if let Some(idx) = (buckets.index_of)(d) {
                            base_vec[idx] += (minutes as f64 / 60.0) * rate;
                        }
                    }
                }
            } else {
                for task in project.tasks.iter().filter(|t| !t.summary) {
                    let Some(b) = task_baseline(task, baseline_num) else {
                        continue;
                    };
                    let base_cost = b.cost;
                    if base_cost <= 0.0 {
                        continue;
                    }
                    distribute_baseline_cost(project, task, b, buckets, base_vec);
                }
            }
        }
    }

    // 3. Actual Cost and Earned Value
    if let (Some(status_ts), Some(status_d)) = (project.status_date, status_day) {
        if let Some(ref mut act_vec) = actual {
            if let Some(res_set) = target_resources {
                for assignment in &project.assignments {
                    if !res_set.contains(&assignment.resource_uid) {
                        continue;
                    }
                    let Some(task) = tasks.get(&assignment.task_uid) else {
                        continue;
                    };
                    let rate = rates.get(&assignment.resource_uid).copied().unwrap_or(0.0);
                    let actual_cost = if assignment.actual_cost > 0.0 {
                        assignment.actual_cost
                    } else {
                        (assignment.actual_work as f64 / 60.0) * rate
                    };
                    if actual_cost <= 0.0 {
                        continue;
                    }
                    distribute_actual_cost(
                        project,
                        task,
                        actual_cost,
                        status_ts,
                        status_d,
                        buckets,
                        act_vec,
                    );
                }
            } else {
                for task in project.tasks.iter().filter(|t| !t.summary) {
                    let actual_cost = task.actual_cost;
                    if actual_cost <= 0.0 {
                        continue;
                    }
                    distribute_actual_cost(
                        project,
                        task,
                        actual_cost,
                        status_ts,
                        status_d,
                        buckets,
                        act_vec,
                    );
                }
            }
        }

        // Earned Value (EV)
        if has_any_baseline {
            if let Some(ref mut earn_vec) = earned {
                for task in project.tasks.iter().filter(|t| !t.summary) {
                    let Some(b) = task_baseline(task, baseline_num) else {
                        continue;
                    };
                    let pct = task.percent_complete.clamp(0.0, 100.0);
                    if pct <= 0.0 || b.cost <= 0.0 {
                        continue;
                    }
                    let task_ev = b.cost * pct / 100.0;
                    distribute_actual_cost(
                        project,
                        task,
                        task_ev,
                        status_ts,
                        status_d,
                        buckets,
                        earn_vec,
                    );
                }
            }
        }
    }
}

/// Distributes task cost across buckets according to fixed_cost_accrual.
fn distribute_task_cost(
    project: &Project,
    task: &Task,
    cost: f64,
    accrual: Accrual,
    buckets: &Buckets,
    target: &mut [f64],
) {
    match accrual {
        Accrual::Start => {
            let d = civil(task.start).0;
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += cost;
            }
        }
        Accrual::End => {
            let d = civil(task.finish).0;
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += cost;
            }
        }
        Accrual::Prorated => {
            let calendar = project.task_calendar(task);
            let intervals = split::work_intervals(task);
            let total_mins = split::working_within(calendar, &intervals, i64::MIN, i64::MAX);
            if total_mins <= 0 {
                let d = civil(task.finish).0;
                if let Some(idx) = (buckets.index_of)(d) {
                    target[idx] += cost;
                }
                return;
            }

            let min_start = intervals.iter().map(|(s, _)| *s).min().unwrap_or(task.start);
            let max_finish = intervals.iter().map(|(_, f)| *f).max().unwrap_or(task.finish);

            let (mut d, _) = civil(min_start);
            let (end_d, _) = civil(max_finish);

            let mut accumulated = 0i64;
            let mut prev_cost = 0.0f64;

            while let Some(next) = d.succ_opt() {
                let day_mins = split::working_within(calendar, &intervals, compose(d, 0), compose(next, 0));
                accumulated += day_mins;

                let cur_cost = (cost * accumulated as f64) / total_mins as f64;
                let day_cost = cur_cost - prev_cost;
                prev_cost = cur_cost;

                if day_cost > 0.0 {
                    if let Some(idx) = (buckets.index_of)(d) {
                        target[idx] += day_cost;
                    }
                }

                if d >= end_d {
                    break;
                }
                d = next;
            }
        }
    }
}

/// Distributes task baseline cost across buckets according to fixed_cost_accrual.
fn distribute_baseline_cost(
    project: &Project,
    task: &Task,
    baseline: &Baseline,
    buckets: &Buckets,
    target: &mut [f64],
) {
    let cost = baseline.cost;
    match task.fixed_cost_accrual {
        Accrual::Start => {
            let d = civil(baseline.start).0;
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += cost;
            }
        }
        Accrual::End => {
            let d = civil(baseline.finish).0;
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += cost;
            }
        }
        Accrual::Prorated => {
            let calendar = project.task_calendar(task);
            let intervals: Vec<(i64, i64)> = if !baseline.segments.is_empty() {
                baseline.segments.iter().map(|s| (s.start, s.finish)).collect()
            } else {
                vec![(baseline.start, baseline.finish)]
            };

            let total_mins = split::working_within(calendar, &intervals, i64::MIN, i64::MAX);
            if total_mins <= 0 {
                let d = civil(baseline.finish).0;
                if let Some(idx) = (buckets.index_of)(d) {
                    target[idx] += cost;
                }
                return;
            }

            let min_start = intervals.iter().map(|(s, _)| *s).min().unwrap_or(baseline.start);
            let max_finish = intervals.iter().map(|(_, f)| *f).max().unwrap_or(baseline.finish);

            let (mut d, _) = civil(min_start);
            let (end_d, _) = civil(max_finish);

            let mut accumulated = 0i64;
            let mut prev_cost = 0.0f64;

            while let Some(next) = d.succ_opt() {
                let day_mins = split::working_within(calendar, &intervals, compose(d, 0), compose(next, 0));
                accumulated += day_mins;

                let cur_cost = (cost * accumulated as f64) / total_mins as f64;
                let day_cost = cur_cost - prev_cost;
                prev_cost = cur_cost;

                if day_cost > 0.0 {
                    if let Some(idx) = (buckets.index_of)(d) {
                        target[idx] += day_cost;
                    }
                }

                if d >= end_d {
                    break;
                }
                d = next;
            }
        }
    }
}

/// Distributes actual cost or EV over progress intervals up to status date.
fn distribute_actual_cost(
    project: &Project,
    task: &Task,
    cost: f64,
    status_date: i64,
    status_day: NaiveDate,
    buckets: &Buckets,
    target: &mut [f64],
) {
    let Some(intervals) = progress_intervals(project, task) else {
        return;
    };
    let calendar = project.task_calendar(task);
    let cut: Vec<(i64, i64)> = intervals
        .iter()
        .map(|(low, high)| ((*low).min(status_date), (*high).min(status_date)))
        .filter(|(low, high)| high > low)
        .collect();

    let total_working_mins = split::working_within(calendar, &cut, i64::MIN, i64::MAX);
    if total_working_mins <= 0 {
        let d = civil(task.actual_start.unwrap_or(task.start)).0;
        if d <= status_day {
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += cost;
            }
        }
        return;
    }

    let min_start = cut.iter().map(|(s, _)| *s).min().unwrap_or(task.start);
    let max_finish = cut.iter().map(|(_, f)| *f).max().unwrap_or(task.finish);

    let (mut d, _) = civil(min_start);
    let (end_d, _) = civil(max_finish);

    let mut accumulated = 0i64;
    let mut prev_cost = 0.0f64;

    while let Some(next) = d.succ_opt() {
        if d > status_day {
            break;
        }
        let day_mins = split::working_within(calendar, &cut, compose(d, 0), compose(next, 0));
        accumulated += day_mins;

        let cur_cost = (cost * accumulated as f64) / total_working_mins as f64;
        let day_cost = cur_cost - prev_cost;
        prev_cost = cur_cost;

        if day_cost > 0.0 {
            if let Some(idx) = (buckets.index_of)(d) {
                target[idx] += day_cost;
            }
        }

        if d >= end_d {
            break;
        }
        d = next;
    }
}

/// Normalizes Work periods into Progress percentage (0..100%).
#[allow(clippy::too_many_arguments)]
fn compute_progress_periods(
    project: &Project,
    buckets: &Buckets,
    target_resources: Option<&HashSet<u32>>,
    baseline_num: u8,
    has_any_baseline: bool,
    status_day: Option<NaiveDate>,
    scheduled: &mut [f64],
    baseline: &mut Option<Vec<f64>>,
    actual: &mut Option<Vec<f64>>,
) {
    // 1. Compute work hours first
    compute_work_periods(
        project,
        buckets,
        target_resources,
        baseline_num,
        has_any_baseline,
        status_day,
        scheduled,
        baseline,
        actual,
    );

    // 2. Determine normalizing denominator: total baselined work if baseline exists, else scheduled
    let total_baseline_work = baseline.as_ref().map(|v| v.iter().sum::<f64>()).unwrap_or(0.0);
    let total_sched_work = scheduled.iter().sum::<f64>();

    let total_work = if has_any_baseline && total_baseline_work > 0.0 {
        total_baseline_work
    } else {
        total_sched_work
    };

    if total_work <= 0.0 {
        // Zero work: leave all at 0.0
        return;
    }

    let factor = 100.0 / total_work;

    for s in scheduled.iter_mut() {
        *s *= factor;
    }

    if let Some(ref mut base_vec) = baseline {
        for b in base_vec.iter_mut() {
            *b *= factor;
        }
    }

    if let Some(ref mut act_vec) = actual {
        for a in act_vec.iter_mut() {
            *a *= factor;
        }
    }
}
