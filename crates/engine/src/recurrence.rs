//! Recurring tasks: a summary task owns a series definition and its children are
//! real occurrence tasks pinned (SNET) to the dates the pattern produces.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::calendar::Calendar;
use crate::error::{EngineError, Invalid};
use crate::model::{AssignmentBaseline, ConstraintType, Project, Task};
use crate::schedule::normalize_outline;
use crate::timeutil::{civil, compose};
use crate::work;

/// Same ceiling as Microsoft Project.
pub const MAX_OCCURRENCES: usize = 999;
/// Search horizon in civil days, so a calendar without working days cannot loop forever.
const HORIZON_DAYS: i64 = 366 * 100;

/// Ordinal value meaning "the last such weekday of the month".
pub const LAST_WEEK: u8 = 5;

/// Weekdays are 0 = Monday … 6 = Sunday. Ordinals are 1–4, or `LAST_WEEK`.
/// The serialized `kind` tags are stored in `.pdproj` files and must not change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecurrencePattern {
    /// Every N working days on the series calendar.
    Daily { every: u32 },
    /// Every N calendar days; one falling on a non-working day moves to the next working time.
    CalendarDaily { every: u32 },
    /// Every N weeks on the listed weekdays.
    Weekly { every: u32, weekdays: Vec<u8> },
    /// Day X of every N months; X past the month end falls on the last day.
    Monthly { every: u32, day: u32 },
    /// The Nth weekday of every N months, e.g. the second Tuesday or the last Friday.
    MonthlyNth { every: u32, ordinal: u8, weekday: u8 },
    /// Day X of a month every N years; X past the month end falls on the last day.
    Yearly { every: u32, month: u32, day: u32 },
    /// The Nth weekday of a month every N years.
    YearlyNth { every: u32, month: u32, ordinal: u8, weekday: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecurrenceEnd {
    Count { count: u32 },
    /// Inclusive civil date of the instant.
    Until { date: i64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecurrenceSpec {
    pub name: String,
    /// Working minutes per occurrence.
    pub duration: i64,
    /// Only the civil date is used; occurrences start at the first working time of their day.
    pub start: i64,
    pub pattern: RecurrencePattern,
    pub end: RecurrenceEnd,
    #[serde(default)]
    pub calendar_id: Option<u32>,
    /// Older files list resources without units; `clean_spec` moves them into
    /// `resources` at 1.0, so this stays empty for new specs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_uids: Vec<u32>,
    #[serde(default)]
    pub resources: Vec<SeriesResource>,
    /// A background series (a weekly meeting, say) never sets the project finish and
    /// is never on the critical path.
    #[serde(default)]
    pub background: bool,
}

/// A resource assigned to every attached occurrence. Units are 1.0 for 100 %.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeriesResource {
    pub uid: u32,
    pub units: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Recurrence {
    pub spec: RecurrenceSpec,
    /// Nominal dates (local midnight) of attached occurrences the user deleted.
    #[serde(default)]
    pub skipped: Vec<i64>,
}

fn is_working_day(calendar: &Calendar, date: NaiveDate) -> bool {
    calendar
        .ranges_on(date)
        .iter()
        .any(|range| range.start_minute < range.end_minute)
}

fn next_working_day(calendar: &Calendar, from: NaiveDate, limit: NaiveDate) -> Option<NaiveDate> {
    let mut date = from;
    while date <= limit {
        if is_working_day(calendar, date) {
            return Some(date);
        }
        date = date.succ_opt()?;
    }
    None
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|first| first.pred_opt())
        .map(|last| last.day())
        .unwrap_or(28)
}

fn month_index(year: i32, month: u32) -> i64 {
    i64::from(year) * 12 + i64::from(month) - 1
}

fn day_of_month(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day.min(last_day_of_month(year, month)))
}

/// The `ordinal`-th `weekday` of the month, or the last one for `LAST_WEEK`.
fn nth_weekday(year: i32, month: u32, ordinal: u8, weekday: u8) -> Option<NaiveDate> {
    let weekday = u32::from(weekday);
    if ordinal == LAST_WEEK {
        let last = NaiveDate::from_ymd_opt(year, month, last_day_of_month(year, month))?;
        let back = (last.weekday().num_days_from_monday() + 7 - weekday) % 7;
        return last.checked_sub_signed(Duration::days(i64::from(back)));
    }
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let ahead = (weekday + 7 - first.weekday().num_days_from_monday()) % 7;
    // The fourth weekday is at most day 28, so it always exists.
    NaiveDate::from_ymd_opt(year, month, 1 + ahead + 7 * (u32::from(ordinal) - 1))
}

fn check_day(day: u32) -> Result<(), EngineError> {
    if (1..=31).contains(&day) {
        Ok(())
    } else {
        Err(Invalid::DayOfMonth.into())
    }
}

fn check_month(month: u32) -> Result<(), EngineError> {
    if (1..=12).contains(&month) {
        Ok(())
    } else {
        Err(Invalid::Month.into())
    }
}

fn check_nth(ordinal: u8, weekday: u8) -> Result<(), EngineError> {
    if !(1..=LAST_WEEK).contains(&ordinal) {
        return Err(Invalid::WeekOfMonth.into());
    }
    if weekday >= 7 {
        return Err(Invalid::Weekday.into());
    }
    Ok(())
}

/// Visit every `every`-th month from `first` (a `month_index`), feeding the date
/// `pick` returns for that month to `accept` until it stops or passes `limit`.
fn step_months<A, P>(
    first: i64,
    every: u32,
    start: NaiveDate,
    limit: NaiveDate,
    dates: &mut Vec<NaiveDate>,
    accept: &A,
    pick: P,
) -> Result<(), EngineError>
where
    A: Fn(NaiveDate, &mut Vec<NaiveDate>) -> Result<bool, EngineError>,
    P: Fn(i32, u32) -> Option<NaiveDate>,
{
    let every = i64::from(every.max(1));
    let mut index = first;
    loop {
        let year = index.div_euclid(12) as i32;
        let month = index.rem_euclid(12) as u32 + 1;
        index += every;
        let Some(date) = pick(year, month) else {
            break;
        };
        if date > limit {
            break;
        }
        if date < start {
            continue;
        }
        if !accept(date, dates)? {
            break;
        }
    }
    Ok(())
}

fn too_many() -> EngineError {
    Invalid::TooManyOccurrences.into()
}

/// Civil dates of every occurrence, in order, before skipping any slot.
pub fn occurrence_dates(spec: &RecurrenceSpec, calendar: &Calendar) -> Result<Vec<NaiveDate>, EngineError> {
    let (start, _) = civil(spec.start);
    let (count, until) = match spec.end {
        RecurrenceEnd::Count { count } => {
            if count == 0 || count as usize > MAX_OCCURRENCES {
                return Err(Invalid::OccurrenceCount.into());
            }
            (Some(count as usize), None)
        }
        RecurrenceEnd::Until { date } => {
            let (until, _) = civil(date);
            if until < start {
                return Err(Invalid::SeriesEndsBeforeStart.into());
            }
            (None, Some(until))
        }
    };
    let horizon = start + Duration::days(HORIZON_DAYS);
    let limit = until.unwrap_or(horizon).min(horizon);
    let mut dates = Vec::new();
    // Returns false once the end condition stops the series.
    let accept = |date: NaiveDate, dates: &mut Vec<NaiveDate>| -> Result<bool, EngineError> {
        if until.is_some_and(|until| date > until) {
            return Ok(false);
        }
        if dates.len() >= MAX_OCCURRENCES {
            return Err(too_many());
        }
        dates.push(date);
        Ok(count.is_none_or(|count| dates.len() < count))
    };

    match &spec.pattern {
        RecurrencePattern::Daily { every } => {
            let every = (*every).max(1);
            let mut current = next_working_day(calendar, start, limit);
            while let Some(date) = current {
                if !accept(date, &mut dates)? {
                    break;
                }
                let mut next = Some(date);
                for _ in 0..every {
                    next = next
                        .and_then(|day| day.succ_opt())
                        .and_then(|day| next_working_day(calendar, day, limit));
                }
                current = next;
            }
        }
        RecurrencePattern::Weekly { every, weekdays } => {
            let every = i64::from((*every).max(1));
            let days: HashSet<u32> = weekdays.iter().filter(|day| **day < 7).map(|day| u32::from(*day)).collect();
            if days.is_empty() {
                return Err(Invalid::NoWeekday.into());
            }
            let week_start = start - Duration::days(i64::from(start.weekday().num_days_from_monday()));
            let mut date = start;
            while date <= limit {
                let week = (date - week_start).num_days() / 7;
                if week % every == 0
                    && days.contains(&date.weekday().num_days_from_monday())
                    && !accept(date, &mut dates)?
                {
                    break;
                }
                date = match date.succ_opt() {
                    Some(next) => next,
                    None => break,
                };
            }
        }
        RecurrencePattern::CalendarDaily { every } => {
            let every = i64::from((*every).max(1));
            let mut date = start;
            while date <= limit {
                if !accept(date, &mut dates)? {
                    break;
                }
                date = match date.checked_add_signed(Duration::days(every)) {
                    Some(next) => next,
                    None => break,
                };
            }
        }
        RecurrencePattern::Monthly { every, day } => {
            check_day(*day)?;
            let first = month_index(start.year(), start.month());
            step_months(first, *every, start, limit, &mut dates, &accept, |year, month| {
                day_of_month(year, month, *day)
            })?;
        }
        RecurrencePattern::MonthlyNth { every, ordinal, weekday } => {
            check_nth(*ordinal, *weekday)?;
            let first = month_index(start.year(), start.month());
            step_months(first, *every, start, limit, &mut dates, &accept, |year, month| {
                nth_weekday(year, month, *ordinal, *weekday)
            })?;
        }
        RecurrencePattern::Yearly { every, month, day } => {
            check_month(*month)?;
            check_day(*day)?;
            let first = month_index(start.year(), *month);
            let every = (*every).max(1).saturating_mul(12);
            step_months(first, every, start, limit, &mut dates, &accept, |year, month| {
                day_of_month(year, month, *day)
            })?;
        }
        RecurrencePattern::YearlyNth { every, month, ordinal, weekday } => {
            check_month(*month)?;
            check_nth(*ordinal, *weekday)?;
            let first = month_index(start.year(), *month);
            let every = (*every).max(1).saturating_mul(12);
            step_months(first, every, start, limit, &mut dates, &accept, |year, month| {
                nth_weekday(year, month, *ordinal, *weekday)
            })?;
        }
    }
    if dates.is_empty() {
        return Err(Invalid::NoOccurrences.into());
    }
    Ok(dates)
}

pub fn validate_spec(project: &Project, spec: &RecurrenceSpec) -> Result<(), EngineError> {
    if let Some(id) = spec.calendar_id {
        if project.calendar_by_id(id).is_none() {
            return Err(Invalid::NoSuchCalendar.into());
        }
    }
    let uids = spec.resource_uids.iter().chain(spec.resources.iter().map(|resource| &resource.uid));
    for uid in uids {
        if project.resources.iter().all(|resource| resource.uid != *uid) {
            return Err(EngineError::ResourceNotFound);
        }
    }
    if spec
        .resources
        .iter()
        .any(|resource| !resource.units.is_finite() || resource.units <= 0.0)
    {
        return Err(Invalid::UnitsNotPositive.into());
    }
    if spec.duration < 0 {
        return Err(Invalid::BadDuration.into());
    }
    Ok(())
}

pub fn clean_spec(spec: &RecurrenceSpec) -> RecurrenceSpec {
    let mut spec = spec.clone();
    let name = spec.name.trim();
    spec.name = if name.is_empty() {
        "Công việc lặp".into()
    } else {
        name.to_string()
    };
    spec.duration = spec.duration.max(0);
    let legacy = std::mem::take(&mut spec.resource_uids);
    spec.resources.extend(legacy.into_iter().map(|uid| SeriesResource { uid, units: 1.0 }));
    let mut seen = HashSet::new();
    spec.resources.retain(|resource| seen.insert(resource.uid));
    if let RecurrencePattern::Weekly { weekdays, .. } = &mut spec.pattern {
        weekdays.retain(|day| *day < 7);
        weekdays.sort_unstable();
        weekdays.dedup();
    }
    spec
}

fn subtree_end(tasks: &[Task], index: usize) -> usize {
    let level = tasks[index].outline_level;
    let mut end = index + 1;
    while end < tasks.len() && tasks[end].outline_level > level {
        end += 1;
    }
    end
}

/// Rebuild the attached occurrences of a series from its definition.
///
/// Detached occurrences and other children are kept as they are. Attached
/// occurrences reuse their UIDs in order, so links and baselines on them survive.
pub fn regenerate(project: &mut Project, summary_uid: u32) -> Result<(), EngineError> {
    normalize_outline(project);
    let index = project.index_of(summary_uid).ok_or(EngineError::TaskNotFound)?;
    let recurrence = project.tasks[index]
        .recurrence
        .clone()
        .ok_or_else(|| EngineError::from(Invalid::NotRecurring))?;
    let spec = clean_spec(&recurrence.spec);
    validate_spec(project, &spec)?;
    let calendar = spec
        .calendar_id
        .and_then(|id| project.calendar_by_id(id))
        .unwrap_or_else(|| project.calendar())
        .clone();
    if !calendar.has_working_time() {
        return Err(EngineError::NoWorkingTime);
    }
    let dates = occurrence_dates(&spec, &calendar)?;

    let level = project.tasks[index].outline_level;
    let end = subtree_end(&project.tasks, index);
    let entered = with_assignment_entries(project);
    let region: Vec<Task> = project.tasks.drain(index + 1..end).collect();
    let mut detached = Vec::new();
    let mut attached = Vec::new();
    let mut others = Vec::new();
    for task in region {
        if task.series_uid == Some(summary_uid) {
            // An occurrence with recorded progress happened; regenerating keeps it as is.
            if task.detached || has_progress(&task) || entered.contains(&task.uid) {
                detached.push(task);
            } else {
                attached.push(task);
            }
        } else {
            others.push(task);
        }
    }
    let taken: HashSet<i64> = detached
        .iter()
        .filter_map(|task| task.occurrence_date)
        .chain(recurrence.skipped.iter().copied())
        .collect();
    let previous: HashMap<u32, Task> = attached.iter().map(|task| (task.uid, task.clone())).collect();
    let mut reuse = attached.iter().map(|task| task.uid);

    let mut fresh = Vec::new();
    for (ordinal, date) in dates.iter().enumerate() {
        let nominal = compose(*date, 0);
        if taken.contains(&nominal) {
            continue;
        }
        let uid = match reuse.next() {
            Some(uid) => uid,
            None => project.alloc_task_uid(),
        };
        let pinned = calendar.next_work_start(nominal);
        let mut task = Task::new(uid, format!("{} {}", spec.name, ordinal + 1));
        task.outline_level = level + 1;
        task.duration = spec.duration;
        task.remaining_duration = spec.duration;
        task.work = spec.duration;
        task.remaining_work = spec.duration;
        task.milestone = spec.duration == 0;
        // Resources must not shorten a fixed occurrence length.
        task.effort_driven = false;
        task.constraint_type = ConstraintType::Snet;
        task.constraint_date = Some(pinned);
        task.calendar_id = spec.calendar_id;
        task.series_uid = Some(summary_uid);
        task.occurrence_date = Some(nominal);
        task.start = pinned;
        task.finish = pinned;
        if let Some(old) = previous.get(&uid) {
            task.baselines = old.baselines.clone();
        }
        fresh.push(task);
    }
    let removed: Vec<u32> = reuse.collect();
    let rebuilt: HashSet<u32> = fresh.iter().map(|task| task.uid).collect();
    project.dependencies.retain(|dep| {
        !removed.contains(&dep.predecessor_uid) && !removed.contains(&dep.successor_uid)
    });
    // Occurrences that survive keep each person's baseline work, as they keep the
    // task's own baselines.
    let kept_baselines: HashMap<(u32, u32), Vec<AssignmentBaseline>> = project
        .assignments
        .iter()
        .filter(|assignment| rebuilt.contains(&assignment.task_uid))
        .filter(|assignment| !assignment.baselines.is_empty())
        .map(|assignment| {
            (
                (assignment.task_uid, assignment.resource_uid),
                assignment.baselines.clone(),
            )
        })
        .collect();
    project.assignments.retain(|assignment| {
        !removed.contains(&assignment.task_uid) && !rebuilt.contains(&assignment.task_uid)
    });

    let mut occurrences: Vec<Task> = fresh.into_iter().chain(detached).collect();
    for task in &mut occurrences {
        task.outline_level = level + 1;
    }
    occurrences.sort_by_key(|task| (task.occurrence_date.unwrap_or(i64::MAX), task.uid));
    let children: Vec<Task> = occurrences.into_iter().chain(others).collect();
    project.tasks.splice(index + 1..index + 1, children);
    let summary = &mut project.tasks[index];
    summary.name = spec.name.clone();
    summary.recurrence = Some(Recurrence {
        spec: spec.clone(),
        skipped: recurrence.skipped,
    });
    normalize_outline(project);

    let mut ordered: Vec<u32> = rebuilt.into_iter().collect();
    ordered.sort_unstable();
    for uid in ordered {
        for resource in &spec.resources {
            let assignment_uid = work::add_assignment(project, uid, resource.uid, resource.units);
            if let Some(baselines) = kept_baselines.get(&(uid, resource.uid)) {
                if let Some(assignment) = project
                    .assignments
                    .iter_mut()
                    .find(|assignment| assignment.uid == assignment_uid)
                {
                    assignment.baselines = baselines.clone();
                }
            }
        }
    }
    Ok(())
}

/// An occurrence with recorded progress took place: regenerating and deleting the
/// series keep it, like an occurrence edited on its own.
/// Tasks with an actual cost entered on one of their assignments: like progress, it
/// records something that happened.
pub fn with_assignment_entries(project: &Project) -> std::collections::HashSet<u32> {
    project
        .assignments
        .iter()
        .filter(|assignment| assignment.manual_actual_cost.is_some())
        .map(|assignment| assignment.task_uid)
        .collect()
}

pub fn has_progress(task: &Task) -> bool {
    task.actual_start.is_some()
        || task.percent_complete > 0.0
        || task.manual_actual_cost.is_some()
}

/// Delete a series: the summary and its attached occurrences go, everything
/// else under it (detached occurrences, occurrences with progress, hand-added tasks)
/// moves up one level.
pub fn delete_series(project: &mut Project, summary_uid: u32) -> Result<(), EngineError> {
    normalize_outline(project);
    let index = project.index_of(summary_uid).ok_or(EngineError::TaskNotFound)?;
    let end = subtree_end(&project.tasks, index);
    let entered = with_assignment_entries(project);
    let mut removed = vec![summary_uid];
    let mut kept = Vec::new();
    for mut task in project.tasks.drain(index..end).skip(1) {
        if task.series_uid == Some(summary_uid)
            && !task.detached
            && !has_progress(&task)
            && !entered.contains(&task.uid)
        {
            removed.push(task.uid);
            continue;
        }
        if task.series_uid == Some(summary_uid) {
            task.series_uid = None;
            task.detached = false;
            task.occurrence_date = None;
        }
        task.outline_level = task.outline_level.saturating_sub(1).max(1);
        kept.push(task);
    }
    project.tasks.splice(index..index, kept);
    project.dependencies.retain(|dep| {
        !removed.contains(&dep.predecessor_uid) && !removed.contains(&dep.successor_uid)
    });
    project
        .assignments
        .retain(|assignment| !removed.contains(&assignment.task_uid));
    normalize_outline(project);
    Ok(())
}

/// Remember a deleted occurrence so regenerating does not bring it back.
pub fn skip_occurrence(project: &mut Project, uid: u32) {
    let Some(task) = project.task(uid) else {
        return;
    };
    let (Some(series), Some(nominal)) = (task.series_uid, task.occurrence_date) else {
        return;
    };
    if let Some(recurrence) = project.task_mut(series).and_then(|summary| summary.recurrence.as_mut()) {
        if !recurrence.skipped.contains(&nominal) {
            recurrence.skipped.push(nominal);
        }
    }
}
