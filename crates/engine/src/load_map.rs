//! The load map (phase 18 P3, step 3d): for each person, the work assigned and the
//! capacity they have in each day or week of a span, for the heatmap. A read, never logged.
//!
//! Work comes from the usage the scheduler already publishes (`Project::usage`, the hours of
//! each assignment by day), so it follows split gaps and task calendars as the Usage view
//! does. Capacity is the working time of the person's calendar on each day times the units
//! they have that day (`availability::units_on`: max units, or a period's), the same
//! capacity overallocation and leveling read. Both are in working minutes.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::availability::units_on;
use crate::error::{EngineError, Invalid};
use crate::model::Project;
use crate::timeutil::{date_key, parse_date_key};

/// Days or weeks a map can have, so a span typed by mistake cannot ask for decades.
pub const MAX_BUCKETS: usize = 731;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Day,
    /// Monday to Sunday.
    Week,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct LoadCell {
    /// Working minutes of work assigned.
    pub work: i64,
    /// Working minutes the person has.
    pub capacity: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoadRow {
    pub resource_uid: u32,
    pub cells: Vec<LoadCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LoadMap {
    /// The first day of each bucket, `YYYY-MM-DD`.
    pub buckets: Vec<String>,
    /// One row per resource, in the project's order.
    pub rows: Vec<LoadRow>,
}

/// Work and capacity by resource for the days `from` to `to` (both included), by day or
/// by week (the weeks that hold those days, whole).
pub fn load_map(project: &Project, from: NaiveDate, to: NaiveDate, bucket: Bucket) -> Result<LoadMap, EngineError> {
    if to < from {
        return Err(Invalid::BadDate.into());
    }
    let (first, last) = match bucket {
        Bucket::Day => (from, to),
        Bucket::Week => (
            monday_of(from).ok_or(Invalid::BadDate)?,
            monday_of(to)
                .and_then(|monday| monday.checked_add_signed(chrono::Duration::days(6)))
                .ok_or(Invalid::BadDate)?,
        ),
    };
    let days = (last - first).num_days() as usize + 1;
    let length = if bucket == Bucket::Week { 7 } else { 1 };
    let count = days / length;
    if count > MAX_BUCKETS {
        return Err(Invalid::DateRangeTooLong.into());
    }
    let index_of = |date: NaiveDate| -> Option<usize> {
        (date >= first && date <= last).then(|| (date - first).num_days() as usize / length)
    };
    let buckets = (0..count)
        .map(|index| date_key(first + chrono::Duration::days((index * length) as i64)))
        .collect();

    let positions: HashMap<u32, usize> = project
        .resources
        .iter()
        .enumerate()
        .map(|(position, resource)| (resource.uid, position))
        .collect();
    let mut rows: Vec<LoadRow> = project
        .resources
        .iter()
        .map(|resource| LoadRow {
            resource_uid: resource.uid,
            cells: vec![LoadCell::default(); count],
        })
        .collect();
    for usage in &project.usage {
        let Some(&row) = usage.resource_uid.and_then(|uid| positions.get(&uid)) else {
            continue;
        };
        for day in &usage.days {
            let Some(index) = parse_date_key(&day.date).and_then(index_of) else {
                continue;
            };
            rows[row].cells[index].work += (day.hours * 60.0).round() as i64;
        }
    }
    for (resource, row) in project.resources.iter().zip(&mut rows) {
        let calendar = project.resource_calendar(resource);
        let focus = crate::focus::focus_of(project, resource);
        for offset in 0..days {
            let date = first + chrono::Duration::days(offset as i64);
            let working: i64 = calendar
                .ranges_on(date)
                .iter()
                .map(|range| i64::from(range.end_minute) - i64::from(range.start_minute))
                .sum();
            if working > 0 {
                // Only the useful share of the time counts (`focus.rs`).
                let capacity = (working as f64 * units_on(resource, date) * focus).round() as i64;
                row.cells[offset / length].capacity += capacity;
            }
        }
    }
    Ok(LoadMap { buckets, rows })
}

fn monday_of(date: NaiveDate) -> Option<NaiveDate> {
    date.checked_sub_signed(chrono::Duration::days(i64::from(date.weekday().num_days_from_monday())))
}
