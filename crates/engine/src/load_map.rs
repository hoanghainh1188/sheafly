//! The load map (phase 18 P3, step 3d; capacity plan PR 5, #150): for each person, role,
//! or skill, the assigned work, unassigned demand, baseline, and capacity across periods
//! (days, weeks, or months) of a span, for the heatmap and histogram loading charts.
//! A read, never logged.
//!
//! Work comes from the usage the scheduler already publishes (`Project::usage`, the hours of
//! each assignment by day), so it follows split gaps and task calendars as the Usage view
//! does. Capacity is the working time of the person's calendar on each day times the units
//! they have that day (`availability::units_on`: max units, or a period's) and focus factor,
//! the same capacity overallocation and leveling read. Both are in working minutes.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::availability::units_on;
use crate::baseline_load::baseline_load_by_resource;
use crate::error::{EngineError, Invalid};
use crate::model::{Project, Task};
use crate::skill_levels::{qualifies, rank};
use crate::skills::{rule_for, RuleTarget};
use crate::split;
use crate::timeutil::{civil, compose, date_key, parse_date_key};

/// Days, weeks, or months a map can have, so a span typed by mistake cannot ask for decades.
pub const MAX_BUCKETS: usize = 731;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Day,
    /// Monday to Sunday.
    Week,
    /// Whole calendar months.
    Month,
    /// Calendar quarters (January to March, …).
    Quarter,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoadGroup {
    #[default]
    Person,
    #[serde(alias = "role")]
    Group,
    Skill {
        id: u32,
        #[serde(default)]
        min_level: Option<u32>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadCell {
    /// Working minutes of assigned work.
    pub work: i64,
    /// Working minutes of unassigned work (for a skill, leaves matching its rules).
    #[serde(default)]
    pub unassigned: i64,
    /// Working minutes capacity.
    pub capacity: i64,
    /// Baseline working minutes in this bucket (if a baseline was requested).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadRow {
    /// The resource UID (present in Person mode, or for individual skill members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_uid: Option<u32>,
    /// Row label: resource name, group/role name, or skill name.
    pub label: String,
    pub cells: Vec<LoadCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadMap {
    /// The first day of each bucket, `YYYY-MM-DD`.
    pub buckets: Vec<String>,
    /// The rows of the map.
    pub rows: Vec<LoadRow>,
}

/// The baseline load of every person by day, from [`baseline_load_by_resource`].
type BaselineLoad = BTreeMap<u32, BTreeMap<NaiveDate, i64>>;

pub(crate) struct Buckets {
    pub(crate) first: NaiveDate,
    pub(crate) last: NaiveDate,
    pub(crate) count: usize,
    pub(crate) keys: Vec<String>,
    pub(crate) index_of: Box<dyn Fn(NaiveDate) -> Option<usize>>,
}

/// Work and capacity by resource, role or skill for the days `from` to `to` (both included),
/// by day, week, or whole calendar month.
pub fn load_map(
    project: &Project,
    from: NaiveDate,
    to: NaiveDate,
    bucket: Bucket,
    group: LoadGroup,
    baseline: Option<u8>,
) -> Result<LoadMap, EngineError> {
    if to < from {
        return Err(Invalid::BadDate.into());
    }

    let buckets = compute_buckets(from, to, bucket)?;
    // One pass over the assignments for every person, not one per person.
    let baseline = baseline.map(|number| baseline_load_by_resource(project, number));
    let baseline = baseline.as_ref();

    match group {
        LoadGroup::Person => build_person_rows(project, &buckets, baseline),
        LoadGroup::Group => build_role_rows(project, &buckets, baseline),
        LoadGroup::Skill { id, min_level } => {
            build_skill_rows(project, &buckets, id, min_level, baseline)
        }
    }
}

pub(crate) fn compute_buckets(from: NaiveDate, to: NaiveDate, bucket: Bucket) -> Result<Buckets, EngineError> {
    match bucket {
        Bucket::Day => {
            let first = from;
            let last = to;
            let count = (last - first).num_days() as usize + 1;
            if count > MAX_BUCKETS {
                return Err(Invalid::DateRangeTooLong.into());
            }
            let keys = (0..count)
                .map(|i| date_key(first + chrono::Duration::days(i as i64)))
                .collect();
            let index_of = Box::new(move |date: NaiveDate| {
                if date >= first && date <= last {
                    Some((date - first).num_days() as usize)
                } else {
                    None
                }
            });
            Ok(Buckets { first, last, count, keys, index_of })
        }
        Bucket::Week => {
            let first = monday_of(from).ok_or(Invalid::BadDate)?;
            let last = monday_of(to)
                .and_then(|m| m.checked_add_signed(chrono::Duration::days(6)))
                .ok_or(Invalid::BadDate)?;
            let days = (last - first).num_days() as usize + 1;
            let count = days / 7;
            if count > MAX_BUCKETS {
                return Err(Invalid::DateRangeTooLong.into());
            }
            let keys = (0..count)
                .map(|i| date_key(first + chrono::Duration::days((i * 7) as i64)))
                .collect();
            let index_of = Box::new(move |date: NaiveDate| {
                if date >= first && date <= last {
                    Some((date - first).num_days() as usize / 7)
                } else {
                    None
                }
            });
            Ok(Buckets { first, last, count, keys, index_of })
        }
        Bucket::Month => months(from, to, 1),
        Bucket::Quarter => months(from, to, 3),
    }
}

/// Buckets of `span` whole calendar months, the first starting on a multiple of `span`
/// (quarters start in January, April, July and October).
fn months(from: NaiveDate, to: NaiveDate, span: u32) -> Result<Buckets, EngineError> {
    let month_of = |date: NaiveDate| date.year() * 12 + date.month0() as i32;
    let span_i = span as i32;
    let start_month = month_of(from) - (from.month0() % span) as i32;
    let end_month = month_of(to) - (to.month0() % span) as i32 + span_i;
    let day_one = |index: i32| NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1);
    let first = day_one(start_month).ok_or(Invalid::BadDate)?;
    let last = day_one(end_month).and_then(|d| d.pred_opt()).ok_or(Invalid::BadDate)?;
    let count = ((end_month - start_month) / span_i) as usize;
    if count > MAX_BUCKETS {
        return Err(Invalid::DateRangeTooLong.into());
    }
    let keys = (0..count)
        .map(|i| date_key(day_one(start_month + i as i32 * span_i).unwrap_or(first)))
        .collect();
    let index_of = Box::new(move |date: NaiveDate| {
        if date >= first && date <= last {
            Some(((month_of(date) - start_month) / span_i) as usize)
        } else {
            None
        }
    });
    Ok(Buckets { first, last, count, keys, index_of })
}

fn build_person_rows(
    project: &Project,
    buckets: &Buckets,
    baseline: Option<&BaselineLoad>,
) -> Result<LoadMap, EngineError> {
    let positions: HashMap<u32, usize> = project
        .resources
        .iter()
        .enumerate()
        .map(|(pos, r)| (r.uid, pos))
        .collect();

    let mut rows: Vec<LoadRow> = project
        .resources
        .iter()
        .map(|r| LoadRow {
            resource_uid: Some(r.uid),
            label: r.name.clone(),
            cells: vec![LoadCell::default(); buckets.count],
        })
        .collect();

    populate_usage(project, &positions, &buckets.index_of, &mut rows);

    for (resource, row) in project.resources.iter().zip(&mut rows) {
        add_resource_capacity(project, resource, buckets.first, buckets.last, &buckets.index_of, &mut row.cells);
        if let Some(b) = baseline {
            add_resource_baseline(b, resource.uid, buckets, &mut row.cells);
        }
    }

    Ok(LoadMap { buckets: buckets.keys.clone(), rows })
}

fn build_role_rows(
    project: &Project,
    buckets: &Buckets,
    baseline: Option<&BaselineLoad>,
) -> Result<LoadMap, EngineError> {
    let mut roles: Vec<String> = Vec::new();
    for r in &project.resources {
        if !roles.contains(&r.group) {
            roles.push(r.group.clone());
        }
    }

    let mut rows: Vec<LoadRow> = roles
        .iter()
        .map(|role| LoadRow {
            resource_uid: None,
            label: role.clone(),
            cells: vec![LoadCell::default(); buckets.count],
        })
        .collect();

    let mut role_to_row: HashMap<&str, usize> = HashMap::new();
    for (i, role) in roles.iter().enumerate() {
        role_to_row.insert(role.as_str(), i);
    }

    let mut resource_to_row: HashMap<u32, usize> = HashMap::new();
    for r in &project.resources {
        if let Some(&row_idx) = role_to_row.get(r.group.as_str()) {
            resource_to_row.insert(r.uid, row_idx);
        }
    }

    populate_usage(project, &resource_to_row, &buckets.index_of, &mut rows);

    for resource in &project.resources {
        if let Some(&row_idx) = resource_to_row.get(&resource.uid) {
            add_resource_capacity(project, resource, buckets.first, buckets.last, &buckets.index_of, &mut rows[row_idx].cells);
            if let Some(b) = baseline {
                add_resource_baseline(b, resource.uid, buckets, &mut rows[row_idx].cells);
            }
        }
    }

    Ok(LoadMap { buckets: buckets.keys.clone(), rows })
}

fn build_skill_rows(
    project: &Project,
    buckets: &Buckets,
    skill_id: u32,
    min_level: Option<u32>,
    baseline: Option<&BaselineLoad>,
) -> Result<LoadMap, EngineError> {
    let skill = match project.skills.iter().find(|s| s.id == skill_id) {
        Some(s) => s,
        None => return Ok(LoadMap { buckets: buckets.keys.clone(), rows: Vec::new() }),
    };

    let members: Vec<&crate::model::Resource> = project
        .resources
        .iter()
        .filter(|r| qualifies(project, r, skill_id, min_level))
        .collect();

    // Summary row for the skill itself
    let mut skill_cells = vec![LoadCell::default(); buckets.count];

    // Unassigned demand for this skill: the leaves suggestions would offer, so work already
    // started or finished is not counted as still waiting for someone.
    let staffed: HashSet<u32> = project.assignments.iter().map(|a| a.task_uid).collect();
    for task in &project.tasks {
        if task.summary
            || staffed.contains(&task.uid)
            || task.work <= 0
            || task.actual_start.is_some()
            || task.percent_complete > 0.0
        {
            continue;
        }
        if let Some(rule_idx) = rule_for(project, task) {
            let rule = &project.assignment_rules[rule_idx];
            let matches_skill = match rule.target {
                RuleTarget::Skill { id: tid, min_level: t_min } => {
                    if tid != skill_id {
                        false
                    } else {
                        match (min_level, t_min) {
                            (Some(q_min), Some(t_min)) => {
                                let q_rank = rank(project, q_min).unwrap_or(0);
                                let t_rank = rank(project, t_min).unwrap_or(0);
                                t_rank >= q_rank
                            }
                            (Some(_), None) => false,
                            (None, _) => true,
                        }
                    }
                }
                _ => false,
            };
            if matches_skill {
                spread_unassigned_task_work(project, task, &buckets.index_of, &mut skill_cells);
            }
        }
    }

    // Individual member rows
    let mut member_rows: Vec<LoadRow> = members
        .iter()
        .map(|m| LoadRow {
            resource_uid: Some(m.uid),
            label: m.name.clone(),
            cells: vec![LoadCell::default(); buckets.count],
        })
        .collect();

    // Map resource uid to member row index
    let mut member_positions: HashMap<u32, usize> = HashMap::new();
    for (i, m) in members.iter().enumerate() {
        member_positions.insert(m.uid, i);
    }

    populate_usage(project, &member_positions, &buckets.index_of, &mut member_rows);

    for (m, row) in members.iter().zip(&mut member_rows) {
        add_resource_capacity(project, m, buckets.first, buckets.last, &buckets.index_of, &mut row.cells);
        if let Some(b) = baseline {
            add_resource_baseline(b, m.uid, buckets, &mut row.cells);
        }
    }

    // Accumulate members into the skill summary row
    for row in &member_rows {
        for (i, cell) in row.cells.iter().enumerate() {
            skill_cells[i].work += cell.work;
            skill_cells[i].capacity += cell.capacity;
            if let Some(b) = cell.baseline {
                skill_cells[i].baseline = Some(skill_cells[i].baseline.unwrap_or(0) + b);
            }
        }
    }

    let mut rows = vec![LoadRow {
        resource_uid: None,
        label: skill.name.clone(),
        cells: skill_cells,
    }];
    rows.extend(member_rows);

    Ok(LoadMap { buckets: buckets.keys.clone(), rows })
}

fn populate_usage(
    project: &Project,
    positions: &HashMap<u32, usize>,
    index_of: &impl Fn(NaiveDate) -> Option<usize>,
    rows: &mut [LoadRow],
) {
    for usage in &project.usage {
        let Some(&row) = usage.resource_uid.and_then(|uid| positions.get(&uid)) else {
            continue;
        };
        for day in &usage.days {
            let Some(index) = parse_date_key(&day.date).and_then(index_of) else {
                continue;
            };
            if index < rows[row].cells.len() {
                rows[row].cells[index].work += (day.hours * 60.0).round() as i64;
            }
        }
    }
}

fn add_resource_capacity(
    project: &Project,
    resource: &crate::model::Resource,
    first: NaiveDate,
    last: NaiveDate,
    index_of: &impl Fn(NaiveDate) -> Option<usize>,
    cells: &mut [LoadCell],
) {
    let calendar = project.resource_calendar(resource);
    let focus = crate::focus::focus_of(project, resource);
    let days = (last - first).num_days() as usize + 1;
    for offset in 0..days {
        let date = first + chrono::Duration::days(offset as i64);
        let working: i64 = calendar
            .ranges_on(date)
            .iter()
            .map(|range| i64::from(range.end_minute) - i64::from(range.start_minute))
            .sum();
        if working > 0 {
            let capacity = (working as f64 * units_on(resource, date) * focus).round() as i64;
            if let Some(index) = index_of(date) {
                if index < cells.len() {
                    cells[index].capacity += capacity;
                }
            }
        }
    }
}

fn add_resource_baseline(
    baseline: &BaselineLoad,
    resource_uid: u32,
    buckets: &Buckets,
    cells: &mut [LoadCell],
) {
    let Some(days) = baseline.get(&resource_uid) else {
        return;
    };
    for (&date, &mins) in days.range(buckets.first..=buckets.last) {
        if let Some(cell) = (buckets.index_of)(date).and_then(|index| cells.get_mut(index)) {
            *cell.baseline.get_or_insert(0) += mins;
        }
    }
}

fn spread_unassigned_task_work(
    project: &Project,
    task: &Task,
    index_of: &impl Fn(NaiveDate) -> Option<usize>,
    cells: &mut [LoadCell],
) {
    if task.work <= 0 || task.finish <= task.start {
        return;
    }
    let intervals: Vec<(i64, i64)> = if !task.segments.is_empty() {
        task.segments.iter().map(|s| (s.start, s.finish)).collect()
    } else {
        vec![(task.start, task.finish)]
    };
    let Some(min_start) = intervals.iter().map(|(s, _)| *s).min() else {
        return;
    };
    let Some(max_finish) = intervals.iter().map(|(_, f)| *f).max() else {
        return;
    };

    let calendar = project.task_calendar(task);
    let (mut date, _) = civil(min_start);
    let (end_date, _) = civil(max_finish);

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
        return;
    }

    let mut accumulated_minutes: i64 = 0;
    let mut accumulated_work: i64 = 0;
    for (d, minutes) in day_minutes {
        accumulated_minutes += minutes;
        let cumulative_work = ((task.work as f64 * accumulated_minutes as f64) / total_task_minutes as f64).round() as i64;
        let day_work = cumulative_work - accumulated_work;
        accumulated_work = cumulative_work;
        if let Some(idx) = index_of(d) {
            if idx < cells.len() {
                cells[idx].unassigned += day_work;
            }
        }
    }
}

fn monday_of(date: NaiveDate) -> Option<NaiveDate> {
    date.checked_sub_signed(chrono::Duration::days(i64::from(date.weekday().num_days_from_monday())))
}
