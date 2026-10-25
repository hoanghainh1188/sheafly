//! Overloaded weeks of the project summary (`summary.rs`): the weeks with an overallocated
//! day, with the person's work and capacity from one weekly `load_map`, so a summary of
//! many people reads the load once instead of once per person.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};

use crate::load_map::{load_map, Bucket, LoadGroup, MAX_BUCKETS};
use crate::model::Project;
use crate::summary::{hours, Lookup, WeekLoad, OVERLOADED_WEEKS_LIMIT};
use crate::timeutil::{date_key, parse_date_key};

/// Work and capacity (working minutes) by resource and Monday.
#[derive(Default)]
pub(crate) struct WeekTable {
    cells: HashMap<(u32, String), (i64, i64)>,
}

impl WeekTable {
    /// The weeks from the first to the last overallocated day of anyone, at most
    /// `MAX_BUCKETS` of them; later weeks list no work or capacity.
    pub(crate) fn new(
        project: &Project,
        overloaded: &BTreeMap<u32, BTreeMap<String, f64>>,
    ) -> Self {
        let days = overloaded
            .values()
            .flat_map(|days| days.keys())
            .filter_map(|key| parse_date_key(key));
        let (Some(from), Some(to)) = days.fold(
            (None, None),
            |(low, high): (Option<NaiveDate>, Option<NaiveDate>), day| {
                (
                    Some(low.map_or(day, |low| low.min(day))),
                    Some(high.map_or(day, |high| high.max(day))),
                )
            },
        ) else {
            return Self::default();
        };
        let limit = from + chrono::Duration::days(7 * MAX_BUCKETS as i64 - 7);
        let Ok(map) = load_map(
            project,
            from,
            to.min(limit),
            Bucket::Week,
            LoadGroup::Person,
            None,
        ) else {
            return Self::default();
        };
        let mut cells = HashMap::new();
        for row in map.rows {
            let Some(uid) = row.resource_uid.filter(|uid| overloaded.contains_key(uid)) else {
                continue;
            };
            for (key, cell) in map.buckets.iter().zip(&row.cells) {
                cells.insert((uid, key.clone()), (cell.work, cell.capacity));
            }
        }
        Self { cells }
    }
}

/// The earliest weeks with an overallocated day for one person.
pub(crate) fn overloaded_weeks(lookup: &Lookup, resource_uid: u32) -> Vec<WeekLoad> {
    let Some(days) = lookup.overloaded.get(&resource_uid) else {
        return Vec::new();
    };
    let mut peaks: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for (key, share) in days {
        let Some(day) = parse_date_key(key) else {
            continue;
        };
        let monday = day - chrono::Duration::days(i64::from(day.weekday().num_days_from_monday()));
        let peak = peaks.entry(monday).or_insert(0.0);
        *peak = peak.max(*share);
    }
    peaks
        .into_iter()
        .take(OVERLOADED_WEEKS_LIMIT)
        .map(|(monday, share)| {
            let week = date_key(monday);
            let (work, capacity) = lookup
                .weeks
                .cells
                .get(&(resource_uid, week.clone()))
                .copied()
                .unwrap_or_default();
            WeekLoad {
                week,
                work_h: hours(work),
                capacity_h: hours(capacity),
                peak_percent: (share * 100.0).round().min(9_999.0) as u32,
            }
        })
        .collect()
}
