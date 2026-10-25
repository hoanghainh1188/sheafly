//! Costs over time (phase 25 PR A, sheafly-pro#5): `cost_timephased`, which the S-curves of
//! cost and the cash flow's money out share. Each series adds up to its cost and follows
//! the accrual, the segments and the calendar.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::command::Command;
use sheafly_engine::cost_timephased::{cost_timephased, CostSeries};
use sheafly_engine::curves::{curves, CurveFilter, CurveMeasure};
use sheafly_engine::load_map::Bucket;
use sheafly_engine::model::{Accrual, ResourceKind, Task};
use sheafly_engine::timeutil::{civil, timestamp};
use sheafly_engine::{Document, Project};

const DAY: i64 = 480;
const EPS: f64 = 1e-6;

fn at(day: u32, hour: u32) -> i64 {
    let date = day0()
        .checked_add_signed(chrono::Duration::days(i64::from(day)))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, 0)
}

/// Monday 2026-01-05.
fn day0() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
}

fn date(day: u32) -> NaiveDate {
    civil(at(day, 8)).0
}

/// One task of `days` working days with one person at 10 an hour on it, and a fixed cost of
/// 100 with `accrual`.
fn doc(days: i64, accrual: Accrual) -> Document {
    let mut project = Project::new("Chi phí", at(0, 8));
    project.tasks.push(Task::new(1, "Việc"));
    project.next_task_uid = 2;
    let mut doc = Document::from_project(project);
    doc.apply(Command::AddResource {
        name: "An".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();
    doc.apply(Command::UpdateResource {
        uid: 1,
        name: "An".into(),
        kind: ResourceKind::Person,
        max_units: 1.0,
        calendar_id: None,
        standard_rate: 10.0,
        overtime_rate: 15.0,
        cost_per_use: 0.0,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: days * DAY,
    })
    .unwrap();
    doc.apply(Command::SetFixedCost {
        uid: 1,
        cost: 100.0,
        accrual,
    })
    .unwrap();
    doc
}

fn series(doc: &Document, series: CostSeries) -> Vec<(NaiveDate, f64)> {
    cost_timephased(
        &doc.project,
        series,
        date(0) - chrono::Days::new(30),
        date(400),
    )
}

fn total(days: &[(NaiveDate, f64)]) -> f64 {
    days.iter().map(|(_, amount)| amount).sum()
}

fn on(days: &[(NaiveDate, f64)], day: NaiveDate) -> f64 {
    days.iter()
        .filter(|(d, _)| *d == day)
        .map(|(_, amount)| amount)
        .sum()
}

fn leaves_cost(doc: &Document) -> f64 {
    doc.project
        .tasks
        .iter()
        .filter(|t| !t.summary)
        .map(|t| t.cost)
        .sum()
}

#[test]
fn the_scheduled_series_follows_the_accrual_of_the_fixed_cost() {
    // Two days of labour at 10 an hour: 160, plus the fixed 100.
    for (accrual, first, second) in [
        (Accrual::Start, 180.0, 80.0),
        (Accrual::End, 80.0, 180.0),
        (Accrual::Prorated, 130.0, 130.0),
    ] {
        let doc = doc(2, accrual);
        assert!((doc.project.tasks[0].cost - 260.0).abs() < EPS);
        let days = series(&doc, CostSeries::Scheduled);
        assert_eq!(days.len(), 2, "{accrual:?}");
        assert!(
            (on(&days, date(0)) - first).abs() < EPS,
            "{accrual:?} {days:?}"
        );
        assert!(
            (on(&days, date(1)) - second).abs() < EPS,
            "{accrual:?} {days:?}"
        );
        assert!((total(&days) - leaves_cost(&doc)).abs() < EPS);
    }
}

#[test]
fn a_baseline_keeps_its_days_and_its_total() {
    let mut doc = doc(2, Accrual::Prorated);
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 4 * DAY,
    })
    .unwrap();
    let base = series(&doc, CostSeries::Baseline { number: 0 });
    assert_eq!(
        base.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
        [date(0), date(1)]
    );
    assert!((total(&base) - 260.0).abs() < EPS);
    assert!(series(&doc, CostSeries::Baseline { number: 1 }).is_empty());
    // The schedule now spends over four days, on the new cost.
    let now = series(&doc, CostSeries::Scheduled);
    assert_eq!(now.len(), 4);
    assert!((total(&now) - leaves_cost(&doc)).abs() < EPS);
}

#[test]
fn split_gaps_holidays_and_weekends_carry_nothing() {
    let mut doc = doc(4, Accrual::Prorated);
    // Day 1 (Tuesday) is a holiday; a split adds a gap of one working day after day 2.
    doc.apply(Command::AddHoliday {
        date: "2026-01-06".into(),
    })
    .unwrap();
    doc.apply(Command::SplitTask {
        uid: 1,
        at: at(3, 8),
        gap: DAY,
    })
    .unwrap();
    let days = series(&doc, CostSeries::Scheduled);
    let spent: Vec<NaiveDate> = days.iter().map(|(d, _)| *d).collect();
    // Mon, (Tue off), Wed, (Thu gap), Fri, (weekend), Mon, Tue.
    assert_eq!(spent, [date(0), date(2), date(4), date(7)]);
    assert!((total(&days) - leaves_cost(&doc)).abs() < EPS);
}

#[test]
fn overtime_and_a_productivity_factor_still_add_up_to_the_cost() {
    let mut doc = doc(2, Accrual::End);
    let assignment = doc.project.assignments[0].uid;
    doc.apply(Command::UpdateAssignment {
        uid: assignment,
        units: None,
        work: None,
        overtime_work: Some(120),
    })
    .unwrap();
    doc.apply(Command::SetFocusFactor { factor: 0.8 }).unwrap();
    let days = series(&doc, CostSeries::Scheduled);
    assert!(days.len() >= 2);
    assert!(
        (total(&days) - leaves_cost(&doc)).abs() < EPS,
        "{days:?} {}",
        leaves_cost(&doc)
    );
}

#[test]
fn actual_remaining_and_forecast_split_at_the_status_date() {
    // Four days, half done by the end of day 1, status date at the end of day 1.
    let mut doc = doc(4, Accrual::Prorated);
    doc.apply(Command::SetStatusDate {
        date: Some(at(1, 17)),
    })
    .unwrap();
    doc.apply(Command::SetPercentComplete {
        uid: 1,
        percent: 50.0,
    })
    .unwrap();
    let task = doc.project.tasks[0].clone();
    assert!(task.actual_cost > 0.0);

    let actual = series(&doc, CostSeries::Actual);
    let remaining = series(&doc, CostSeries::Remaining);
    let forecast = series(&doc, CostSeries::Forecast);
    assert!((total(&actual) - task.actual_cost).abs() < EPS);
    assert!((total(&remaining) - (task.cost - task.actual_cost)).abs() < EPS);
    assert!((total(&forecast) - task.cost).abs() < EPS);
    assert!(actual.iter().all(|(d, _)| *d <= date(1)), "{actual:?}");
    assert!(
        remaining.iter().all(|(d, _)| *d >= date(2)),
        "{remaining:?}"
    );
    // With no work late and none ahead, the forecast is the schedule.
    let scheduled = series(&doc, CostSeries::Scheduled);
    for (day, amount) in &scheduled {
        assert!(
            (on(&forecast, *day) - amount).abs() < EPS,
            "{day} {forecast:?} {scheduled:?}"
        );
    }
}

#[test]
fn late_work_still_to_do_is_spent_from_the_status_date() {
    // Two days on days 0 and 1, nothing done, status date on day 3.
    let mut doc = doc(2, Accrual::Start);
    doc.apply(Command::SetStatusDate {
        date: Some(at(3, 17)),
    })
    .unwrap();
    let remaining = series(&doc, CostSeries::Remaining);
    assert!(series(&doc, CostSeries::Actual).is_empty());
    assert_eq!(
        remaining.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
        [date(3)]
    );
    assert!((total(&remaining) - leaves_cost(&doc)).abs() < EPS);
}

#[test]
fn a_started_task_has_no_fixed_cost_left_with_start_accrual() {
    let mut doc = doc(4, Accrual::Start);
    doc.apply(Command::SetStatusDate {
        date: Some(at(1, 17)),
    })
    .unwrap();
    doc.apply(Command::SetPercentComplete {
        uid: 1,
        percent: 25.0,
    })
    .unwrap();
    let actual = series(&doc, CostSeries::Actual);
    // The fixed 100 and a quarter of 320 labour, all on day 0.
    assert!((on(&actual, date(0)) - 180.0).abs() < EPS, "{actual:?}");
    let remaining = series(&doc, CostSeries::Remaining);
    assert!((total(&remaining) - 240.0).abs() < EPS, "{remaining:?}");
    assert!(
        (on(&remaining, date(2)) - 120.0).abs() < EPS,
        "{remaining:?}"
    );
}

#[test]
fn entered_actual_costs_replace_the_computed_ones() {
    let mut doc = doc(4, Accrual::Prorated);
    doc.apply(Command::SetStatusDate {
        date: Some(at(1, 17)),
    })
    .unwrap();
    doc.apply(Command::SetPercentComplete {
        uid: 1,
        percent: 50.0,
    })
    .unwrap();
    doc.apply(Command::SetEditableActualCosts { editable: true })
        .unwrap();
    doc.apply(Command::SetTaskActualCost {
        uid: 1,
        cost: Some(400.0),
    })
    .unwrap();
    let task = &doc.project.tasks[0];
    assert!((task.actual_cost - 400.0).abs() < EPS);
    assert!((total(&series(&doc, CostSeries::Actual)) - 400.0).abs() < EPS);
    let left = (task.cost - 400.0).max(0.0);
    assert!((total(&series(&doc, CostSeries::Remaining)) - left).abs() < EPS);
    // Spending more than planned leaves nothing to spend.
    doc.apply(Command::SetTaskActualCost {
        uid: 1,
        cost: Some(10_000.0),
    })
    .unwrap();
    assert!(series(&doc, CostSeries::Remaining).is_empty());
    assert!((total(&series(&doc, CostSeries::Forecast)) - 10_000.0).abs() < EPS);
}

#[test]
fn without_a_status_date_actual_is_everything_recorded() {
    let mut doc = doc(4, Accrual::Prorated);
    doc.apply(Command::SetPercentComplete {
        uid: 1,
        percent: 50.0,
    })
    .unwrap();
    let task = doc.project.tasks[0].clone();
    assert!((total(&series(&doc, CostSeries::Actual)) - task.actual_cost).abs() < EPS);
    let remaining = series(&doc, CostSeries::Remaining);
    assert!((total(&remaining) - (task.cost - task.actual_cost)).abs() < EPS);
    assert!(
        remaining.iter().all(|(d, _)| *d >= date(2)),
        "{remaining:?}"
    );
}

#[test]
fn the_span_cuts_what_falls_outside_it() {
    let doc = doc(4, Accrual::Prorated);
    let inside = cost_timephased(&doc.project, CostSeries::Scheduled, date(1), date(2));
    assert_eq!(
        inside.iter().map(|(d, _)| *d).collect::<Vec<_>>(),
        [date(1), date(2)]
    );
}

#[test]
fn the_s_curve_of_cost_reads_the_same_numbers() {
    let mut doc = doc(5, Accrual::End);
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 6 * DAY,
    })
    .unwrap();
    doc.apply(Command::SetStatusDate {
        date: Some(at(2, 17)),
    })
    .unwrap();
    doc.apply(Command::SetPercentComplete {
        uid: 1,
        percent: 30.0,
    })
    .unwrap();
    let report = curves(
        &doc.project,
        CurveMeasure::Cost,
        Bucket::Day,
        CurveFilter::Project,
        Some(0),
    )
    .unwrap();
    let by_day = |series_: CostSeries| series(&doc, series_);
    let (scheduled, baseline, actual) = (
        by_day(CostSeries::Scheduled),
        by_day(CostSeries::Baseline { number: 0 }),
        by_day(CostSeries::Actual),
    );
    for point in &report.points {
        let day = civil(point.date).0;
        assert_eq!(point.scheduled_period, on(&scheduled, day), "{day}");
        assert_eq!(point.baseline_period, Some(on(&baseline, day)), "{day}");
        if let Some(value) = point.actual_period {
            assert_eq!(value, on(&actual, day), "{day}");
        }
    }
    assert!((report.total_scheduled - total(&scheduled)).abs() < EPS);
}

#[test]
fn quarters_are_calendar_quarters() {
    let doc = doc(1, Accrual::Prorated);
    let base = timestamp(2026, 2, 10, 8, 0);
    let mut project = doc.project.clone();
    project.start = base;
    // A month and a quarter bucket over the same span: Feb to Aug is 7 months, 3 quarters.
    let from = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();
    let to = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    let map = |bucket| {
        sheafly_engine::load_map::load_map(&project, from, to, bucket, Default::default(), None)
            .unwrap()
            .buckets
    };
    assert_eq!(map(Bucket::Month).len(), 7);
    assert_eq!(
        map(Bucket::Quarter),
        ["2026-01-01", "2026-04-01", "2026-07-01"]
    );
    let across = sheafly_engine::load_map::load_map(
        &project,
        NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(),
        Bucket::Quarter,
        Default::default(),
        None,
    )
    .unwrap();
    assert_eq!(across.buckets, ["2026-10-01", "2027-01-01"]);
    // 731 quarters is the limit, like every period.
    let too_long = sheafly_engine::load_map::load_map(
        &project,
        NaiveDate::from_ymd_opt(1900, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(2100, 1, 1).unwrap(),
        Bucket::Quarter,
        Default::default(),
        None,
    );
    assert!(too_long.is_err());
}
