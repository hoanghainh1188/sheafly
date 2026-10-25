//! Split baselines in MSPDI: a task `<Baseline>` holds only its start, finish, work and
//! cost, so the pieces of a baseline saved on a split task go as task timephased baseline
//! work (type 9 for baseline 0, 18 + 6 × (n − 1) for baseline n, as Project numbers them),
//! one period per piece and an empty one per gap. Without them a round trip would spread
//! baseline load over the gaps.

use std::collections::BTreeMap;

use crate::calendar::Calendar;
use crate::model::{Baseline, Project, Task};
use crate::split::Segment;
use crate::timeutil::{format_duration_iso, format_iso};

fn work_type(number: u8) -> u32 {
    match number {
        0 => 9,
        n => 18 + 6 * (u32::from(n) - 1),
    }
}

fn number_of(kind: u32) -> Option<u8> {
    match kind {
        9 => Some(0),
        k if k >= 18 && (k - 18) % 6 == 0 => u8::try_from((k - 18) / 6 + 1).ok(),
        _ => None,
    }
}

/// Write the pieces of every split baseline of `task`; the schema puts timephased data
/// last in a task.
pub fn write(xml: &mut String, project: &Project, task: &Task) {
    let cal = project.task_calendar(task);
    for baseline in task.baselines.iter().filter(|b| b.segments.len() > 1) {
        write_baseline(xml, cal, task.uid, baseline);
    }
}

fn write_baseline(xml: &mut String, cal: &Calendar, uid: u32, baseline: &Baseline) {
    let kind = work_type(baseline.number);
    let minutes: Vec<i64> = baseline
        .segments
        .iter()
        .map(|segment| cal.working_between(segment.start, segment.finish).max(0))
        .collect();
    let total: i64 = minutes.iter().sum::<i64>().max(1);
    let mut assigned = 0i64;
    for (index, segment) in baseline.segments.iter().enumerate() {
        let value = if index + 1 == baseline.segments.len() {
            (baseline.work - assigned).max(0)
        } else {
            (baseline.work as f64 * minutes[index] as f64 / total as f64).round() as i64
        };
        assigned += value;
        period(xml, kind, uid, segment.start, segment.finish, value);
        if let Some(next) = baseline.segments.get(index + 1) {
            period(xml, kind, uid, segment.finish, next.start, 0);
        }
    }
}

fn period(xml: &mut String, kind: u32, uid: u32, start: i64, finish: i64, minutes: i64) {
    xml.push_str("<TimephasedData>\n");
    el(xml, "Type", &kind.to_string());
    el(xml, "UID", &uid.to_string());
    el(xml, "Start", &format_iso(start));
    el(xml, "Finish", &format_iso(finish));
    el(xml, "Unit", "2");
    el(xml, "Value", &format_duration_iso(minutes));
    xml.push_str("</TimephasedData>\n");
}

fn el(xml: &mut String, name: &str, value: &str) {
    xml.push_str(&format!("<{name}>{value}</{name}>\n"));
}

/// Timephased baseline work read from tasks, by task and baseline number.
#[derive(Default)]
pub struct BaselineReader {
    periods: BTreeMap<(u32, u8), Vec<(i64, i64)>>,
}

impl BaselineReader {
    /// One task timephased record; anything but baseline work is ignored.
    pub fn add(&mut self, task_uid: u32, kind: u32, start: i64, finish: i64, minutes: i64) {
        let Some(number) = number_of(kind) else {
            return;
        };
        if task_uid == 0 || minutes <= 0 || finish <= start {
            return;
        }
        self.periods
            .entry((task_uid, number))
            .or_default()
            .push((start, finish));
    }

    /// Give each baseline its pieces: periods with no working time between them are one
    /// piece, and a baseline left in one piece stays unsplit.
    pub fn finish(self, project: &mut Project) {
        for ((uid, number), periods) in self.periods {
            let Some(index) = project.tasks.iter().position(|task| task.uid == uid) else {
                continue;
            };
            let segments = segments_of(project.task_calendar(&project.tasks[index]), periods);
            if segments.len() < 2 {
                continue;
            }
            if let Some(baseline) = project.tasks[index]
                .baselines
                .iter_mut()
                .find(|baseline| baseline.number == number)
            {
                baseline.segments = segments;
            }
        }
    }
}

fn segments_of(cal: &Calendar, mut periods: Vec<(i64, i64)>) -> Vec<Segment> {
    periods.sort_unstable();
    let mut merged: Vec<Segment> = Vec::new();
    for (start, finish) in periods {
        if let Some(last) = merged.last_mut() {
            if start <= last.finish || cal.working_between(last.finish, start) <= 0 {
                last.finish = last.finish.max(finish);
                continue;
            }
        }
        merged.push(Segment { start, finish });
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_numbers_and_types_round_trip() {
        for number in 0..=10u8 {
            assert_eq!(number_of(work_type(number)), Some(number));
        }
        assert_eq!(work_type(1), 18);
        assert_eq!(number_of(10), None);
        assert_eq!(number_of(1), None);
    }
}
