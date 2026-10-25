//! The calendar months a project spans, shared by the time-phased and loading sheets so
//! their columns line up.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use sheafly_engine::load_map::MAX_BUCKETS;
use sheafly_engine::timeutil::civil;
use sheafly_engine::{EngineError, Invalid};

pub(crate) struct Months {
    /// The first day of the first month.
    pub(crate) first: NaiveDate,
    /// The last day of the last month.
    pub(crate) last: NaiveDate,
    /// `YYYY-MM` of each month, in order.
    pub(crate) keys: Vec<String>,
    index: HashMap<String, usize>,
}

impl Months {
    /// From the month of the project start (or an earlier task start) to the month of the
    /// last finish. As many months as the load map allows.
    pub(crate) fn of(project: &sheafly_engine::model::Project) -> Result<Self, EngineError> {
        let start = project.tasks.iter().map(|t| t.start).chain([project.start]).min().unwrap_or(project.start);
        let finish = project.tasks.iter().map(|t| t.finish).chain([project.start]).max().unwrap_or(project.start);
        let (from, _) = civil(start);
        let (to, _) = civil(finish);
        let first = from.with_day(1).ok_or(Invalid::BadDate)?;
        let mut keys = Vec::new();
        let mut month = first;
        loop {
            keys.push(key(month));
            if keys.len() > MAX_BUCKETS {
                return Err(Invalid::DateRangeTooLong.into());
            }
            let next = next_month(month).ok_or(Invalid::BadDate)?;
            if next > to {
                let last = next.pred_opt().ok_or(Invalid::BadDate)?;
                let index = keys.iter().enumerate().map(|(i, k)| (k.clone(), i)).collect();
                return Ok(Self { first, last, keys, index });
            }
            month = next;
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    /// The month of a `YYYY-MM-DD` key (usage days, load map buckets, overallocations).
    pub(crate) fn of_day(&self, day: &str) -> Option<usize> {
        self.index.get(day.get(..7)?).copied()
    }
}

fn key(date: NaiveDate) -> String {
    format!("{:04}-{:02}", date.year(), date.month())
}

fn next_month(date: NaiveDate) -> Option<NaiveDate> {
    if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    }
}
