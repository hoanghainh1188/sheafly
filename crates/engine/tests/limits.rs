//! Follow-ups to phases 9 and 10: more recurrence patterns, per-resource series
//! units, older files, time-phased actual cost and the S-curve.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::command::Command;
use sheafly_engine::evm::{evm_at, s_curve, CurveStep, SCurvePoint};
use sheafly_engine::model::{Accrual, Baseline, ResourceKind, Task};
use sheafly_engine::recurrence::{
    occurrence_dates, RecurrenceEnd, RecurrencePattern, RecurrenceSpec, SeriesResource, LAST_WEEK,
};
use sheafly_engine::schedule::schedule;
use sheafly_engine::store::{load, save};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Calendar, Document, Project, Resource};

/// 2026-01-05 is a Monday; `day_offset` 0 is that Monday.
fn at(day_offset: u32, hour: u32, minute: u32) -> i64 {
    let date = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap() + chrono::Duration::days(i64::from(day_offset));
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

fn day(year: i32, month: u32, date: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, date).unwrap()
}

fn midnight(year: i32, month: u32, date: u32) -> i64 {
    timestamp(year, month, date, 0, 0)
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
}

fn temp_path(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("sheafly-{name}-{}.pdproj", std::process::id()))
        .to_string_lossy()
        .to_string()
}

fn spec_of(pattern: RecurrencePattern, end: RecurrenceEnd) -> RecurrenceSpec {
    RecurrenceSpec {
        name: "Meeting".into(),
        duration: 120,
        start: at(0, 8, 0),
        pattern,
        end,
        calendar_id: None,
        resource_uids: Vec::new(),
        resources: Vec::new(),
        background: false,
    }
}

fn dates_from(start: i64, pattern: RecurrencePattern, end: RecurrenceEnd) -> Vec<NaiveDate> {
    let mut spec = spec_of(pattern, end);
    spec.start = start;
    occurrence_dates(&spec, &Calendar::standard()).unwrap()
}

fn base_project() -> Project {
    let mut project = Project::new("Limits", at(0, 8, 0));
    project.resources.push(Resource::new(1, "An", ResourceKind::Person));
    project.resources.push(Resource::new(2, "Binh", ResourceKind::Person));
    project.next_resource_uid = 3;
    project
}

fn series_doc(project: Project, spec: RecurrenceSpec) -> (Document, u32) {
    let mut doc = Document::from_project(project);
    doc.apply(Command::CreateRecurringTask { after_uid: None, spec }).unwrap();
    let summary = doc.project.tasks.iter().find(|task| task.recurrence.is_some()).unwrap().uid;
    (doc, summary)
}

fn occurrences(project: &Project, summary: u32) -> Vec<Task> {
    project.tasks.iter().filter(|task| task.series_uid == Some(summary)).cloned().collect()
}

fn nominal_dates(project: &Project, summary: u32) -> Vec<i64> {
    occurrences(project, summary).iter().map(|task| task.occurrence_date.unwrap()).collect()
}

/// (resource UID, units, work) of each assignment on a task.
fn units_of(project: &Project, task_uid: u32) -> Vec<(u32, f64, i64)> {
    project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == task_uid)
        .map(|assignment| (assignment.resource_uid, assignment.units, assignment.work))
        .collect()
}

#[test]
fn calendar_daily_series_counts_every_civil_day() {
    // Every 3 calendar days from Monday 5: 5, 8, 11 (a Sunday), 14.
    let dates = dates_from(at(0, 8, 0), RecurrencePattern::CalendarDaily { every: 3 }, RecurrenceEnd::Count { count: 4 });
    assert_eq!(dates, vec![day(2026, 1, 5), day(2026, 1, 8), day(2026, 1, 11), day(2026, 1, 14)]);
    let until = RecurrenceEnd::Until { date: midnight(2026, 1, 11) };
    let dates = dates_from(at(0, 8, 0), RecurrencePattern::CalendarDaily { every: 2 }, until);
    assert_eq!(dates, vec![day(2026, 1, 5), day(2026, 1, 7), day(2026, 1, 9), day(2026, 1, 11)]);

    // The Sunday occurrence keeps its nominal date and starts Monday 08:00.
    let spec = spec_of(RecurrencePattern::CalendarDaily { every: 3 }, RecurrenceEnd::Count { count: 4 });
    let (doc, summary) = series_doc(base_project(), spec);
    let occ = occurrences(&doc.project, summary);
    assert_eq!(occ[2].occurrence_date, Some(midnight(2026, 1, 11)));
    assert_eq!(occ[2].start, at(7, 8, 0));
    assert_eq!(occ[3].start, at(9, 8, 0));
}

#[test]
fn monthly_nth_weekday_series() {
    // 2026-01-01 is a Thursday. Second Tuesday: Jan 13, Feb 10, Mar 10.
    let second_tuesday = RecurrencePattern::MonthlyNth { every: 1, ordinal: 2, weekday: 1 };
    let dates = dates_from(midnight(2026, 1, 1), second_tuesday, RecurrenceEnd::Count { count: 3 });
    assert_eq!(dates, vec![day(2026, 1, 13), day(2026, 2, 10), day(2026, 3, 10)]);

    // Last Friday every 2 months: Jan 30, Mar 27, May 29.
    let last_friday = RecurrencePattern::MonthlyNth { every: 2, ordinal: LAST_WEEK, weekday: 4 };
    let until = RecurrenceEnd::Until { date: midnight(2026, 6, 30) };
    let dates = dates_from(midnight(2026, 1, 1), last_friday, until);
    assert_eq!(dates, vec![day(2026, 1, 30), day(2026, 3, 27), day(2026, 5, 29)]);

    // A start after this month's first Monday begins next month.
    let first_monday = RecurrencePattern::MonthlyNth { every: 1, ordinal: 1, weekday: 0 };
    let dates = dates_from(midnight(2026, 1, 20), first_monday, RecurrenceEnd::Count { count: 2 });
    assert_eq!(dates, vec![day(2026, 2, 2), day(2026, 3, 2)]);

    // Fourth Sunday of February 2026.
    let fourth_sunday = RecurrencePattern::MonthlyNth { every: 1, ordinal: 4, weekday: 6 };
    let dates = dates_from(midnight(2026, 2, 1), fourth_sunday, RecurrenceEnd::Count { count: 1 });
    assert_eq!(dates, vec![day(2026, 2, 22)]);
}

#[test]
fn yearly_series_on_a_date_or_nth_weekday() {
    // 29 February falls back to the 28th outside leap years.
    let leap_day = RecurrencePattern::Yearly { every: 1, month: 2, day: 29 };
    let dates = dates_from(midnight(2026, 1, 1), leap_day, RecurrenceEnd::Count { count: 3 });
    assert_eq!(dates, vec![day(2026, 2, 28), day(2027, 2, 28), day(2028, 2, 29)]);

    // Like monthly, the cycle is anchored on the start year: a start after this
    // year's date skips that slot, so every 2 years from 2026 gives 2028, 2030.
    let biennial = RecurrencePattern::Yearly { every: 2, month: 2, day: 10 };
    let until = RecurrenceEnd::Until { date: midnight(2031, 12, 31) };
    let dates = dates_from(midnight(2026, 3, 1), biennial, until);
    assert_eq!(dates, vec![day(2028, 2, 10), day(2030, 2, 10)]);
    let annual = RecurrencePattern::Yearly { every: 1, month: 2, day: 10 };
    let dates = dates_from(midnight(2026, 3, 1), annual, RecurrenceEnd::Count { count: 1 });
    assert_eq!(dates, vec![day(2027, 2, 10)]);

    // Fourth Thursday of November: 2026-11-26, 2027-11-25, 2028-11-23.
    let thanksgiving = RecurrencePattern::YearlyNth { every: 1, month: 11, ordinal: 4, weekday: 3 };
    let dates = dates_from(midnight(2026, 1, 1), thanksgiving, RecurrenceEnd::Count { count: 3 });
    assert_eq!(dates, vec![day(2026, 11, 26), day(2027, 11, 25), day(2028, 11, 23)]);

    // Last Monday of May: 2026-05-25, 2027-05-31.
    let last_monday = RecurrencePattern::YearlyNth { every: 1, month: 5, ordinal: LAST_WEEK, weekday: 0 };
    let dates = dates_from(midnight(2026, 1, 1), last_monday, RecurrenceEnd::Count { count: 2 });
    assert_eq!(dates, vec![day(2026, 5, 25), day(2027, 5, 31)]);
}

#[test]
fn new_patterns_reject_out_of_range_fields() {
    let bad = [
        RecurrencePattern::MonthlyNth { every: 1, ordinal: 0, weekday: 1 },
        RecurrencePattern::MonthlyNth { every: 1, ordinal: 6, weekday: 1 },
        RecurrencePattern::MonthlyNth { every: 1, ordinal: 1, weekday: 7 },
        RecurrencePattern::Yearly { every: 1, month: 13, day: 1 },
        RecurrencePattern::Yearly { every: 1, month: 2, day: 0 },
        RecurrencePattern::YearlyNth { every: 1, month: 0, ordinal: 1, weekday: 0 },
    ];
    for pattern in bad {
        let spec = spec_of(pattern.clone(), RecurrenceEnd::Count { count: 2 });
        assert!(occurrence_dates(&spec, &Calendar::standard()).is_err(), "{pattern:?}");
    }
}

#[test]
fn series_assigns_each_resource_at_its_units() {
    let mut spec = spec_of(
        RecurrencePattern::MonthlyNth { every: 1, ordinal: 2, weekday: 1 },
        RecurrenceEnd::Count { count: 2 },
    );
    spec.resources = vec![SeriesResource { uid: 1, units: 0.5 }, SeriesResource { uid: 2, units: 1.5 }];
    let (mut doc, summary) = series_doc(base_project(), spec.clone());
    for task in occurrences(&doc.project, summary) {
        // Occurrences are fixed units, so work follows units over the 2-hour duration.
        assert_eq!(units_of(&doc.project, task.uid), vec![(1, 0.5, 60), (2, 1.5, 180)]);
        assert_eq!(task.duration, 120);
    }

    let mut changed = spec.clone();
    changed.resources = vec![SeriesResource { uid: 2, units: 0.25 }];
    doc.apply(Command::UpdateRecurringTask { uid: summary, spec: changed }).unwrap();
    let first = occurrences(&doc.project, summary)[0].uid;
    assert_eq!(units_of(&doc.project, first), vec![(2, 0.25, 30)]);
    doc.undo().unwrap();
    assert_eq!(units_of(&doc.project, first), vec![(1, 0.5, 60), (2, 1.5, 180)]);

    let mut invalid = spec;
    invalid.resources = vec![SeriesResource { uid: 1, units: 0.0 }];
    assert!(doc.apply(Command::UpdateRecurringTask { uid: summary, spec: invalid }).is_err());
}

#[test]
fn new_patterns_undo_and_survive_store_round_trip() {
    let spec = spec_of(RecurrencePattern::CalendarDaily { every: 2 }, RecurrenceEnd::Count { count: 3 });
    let (mut doc, summary) = series_doc(base_project(), spec.clone());
    let patterns = [
        RecurrencePattern::MonthlyNth { every: 1, ordinal: LAST_WEEK, weekday: 4 },
        RecurrencePattern::Yearly { every: 1, month: 3, day: 15 },
        RecurrencePattern::YearlyNth { every: 1, month: 11, ordinal: 4, weekday: 3 },
    ];
    let mut expected = vec![nominal_dates(&doc.project, summary)];
    for pattern in &patterns {
        let mut next = spec.clone();
        next.pattern = pattern.clone();
        next.resources = vec![SeriesResource { uid: 1, units: 0.75 }];
        doc.apply(Command::UpdateRecurringTask { uid: summary, spec: next }).unwrap();
        expected.push(nominal_dates(&doc.project, summary));
    }
    assert_eq!(expected[0], vec![midnight(2026, 1, 5), midnight(2026, 1, 7), midnight(2026, 1, 9)]);
    assert_eq!(expected[3], vec![midnight(2026, 11, 26), midnight(2027, 11, 25), midnight(2028, 11, 23)]);

    let path = temp_path("patterns");
    save(&doc, &path).unwrap();
    let mut loaded = load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert_eq!(loaded.project.tasks, doc.project.tasks);
    assert_eq!(loaded.project.assignments, doc.project.assignments);
    let restored = loaded.project.task(summary).unwrap().recurrence.clone().unwrap();
    assert_eq!(restored.spec.pattern, patterns[2]);
    assert_eq!(restored.spec.resources, vec![SeriesResource { uid: 1, units: 0.75 }]);

    // Undo after loading walks back through each pattern.
    for dates in expected.iter().rev().skip(1) {
        loaded.undo().unwrap();
        assert_eq!(&nominal_dates(&loaded.project, summary), dates);
    }
}

/// Write a `.pdproj` the way phase 9 did: no burn-down table, raw JSON payloads.
fn write_old_file(path: &str, base: &str, commands: &[String], cursor: usize) {
    let _ = std::fs::remove_file(path);
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE command_log (seq INTEGER PRIMARY KEY, payload TEXT NOT NULL);",
        )
        .unwrap();
    let meta = [("version", "1".to_string()), ("cursor", cursor.to_string()), ("base", base.to_string())];
    for (key, value) in meta {
        connection
            .execute("INSERT INTO meta(key, value) VALUES(?1, ?2)", rusqlite::params![key, value])
            .unwrap();
    }
    for (seq, payload) in commands.iter().enumerate() {
        connection
            .execute(
                "INSERT INTO command_log(seq, payload) VALUES(?1, ?2)",
                rusqlite::params![seq as i64, payload],
            )
            .unwrap();
    }
}

fn series_spec(project: &Project, summary: u32) -> RecurrenceSpec {
    project.task(summary).unwrap().recurrence.as_ref().unwrap().spec.clone()
}

#[test]
fn older_command_log_with_unit_less_series_still_loads() {
    let path = temp_path("old-log");
    let base = serde_json::to_string(&base_project()).unwrap();
    let old_command = format!(
        concat!(
            r#"{{"type":"create_recurring_task","after_uid":null,"spec":{{"name":"Meeting","duration":120,"#,
            r#""start":{},"pattern":{{"kind":"daily","every":1}},"end":{{"kind":"count","count":3}},"#,
            r#""calendar_id":null,"resource_uids":[1]}}}}"#
        ),
        at(0, 8, 0)
    );
    write_old_file(&path, &base, &[old_command], 1);
    let loaded = load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(loaded.burndown.is_empty());
    let summary = loaded.project.tasks.iter().find(|task| task.recurrence.is_some()).unwrap().uid;
    let spec = series_spec(&loaded.project, summary);
    assert!(spec.resource_uids.is_empty());
    assert_eq!(spec.resources, vec![SeriesResource { uid: 1, units: 1.0 }]);
    let occ = occurrences(&loaded.project, summary);
    assert_eq!(occ.len(), 3);
    for task in &occ {
        assert_eq!(units_of(&loaded.project, task.uid), vec![(1, 1.0, 120)]);
    }
}

#[test]
fn older_base_project_with_unit_less_series_migrates_on_regeneration() {
    let spec = spec_of(RecurrencePattern::Daily { every: 1 }, RecurrenceEnd::Count { count: 2 });
    let (doc, summary) = series_doc(base_project(), spec);

    // Rewrite the stored series in the phase 9 shape: plain resource UIDs, no units.
    let mut value = serde_json::to_value(&doc.project).unwrap();
    let stored = value["tasks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|task| task["uid"] == summary)
        .unwrap();
    let spec_json = stored["recurrence"]["spec"].as_object_mut().unwrap();
    spec_json.remove("resources");
    spec_json.insert("resource_uids".into(), serde_json::json!([1]));

    let path = temp_path("old-base");
    write_old_file(&path, &value.to_string(), &[], 0);
    let mut old = load(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let spec = series_spec(&old.project, summary);
    assert_eq!(spec.resource_uids, vec![1]);
    assert!(spec.resources.is_empty());

    // Renaming regenerates the series, which moves the UIDs to units 1.0.
    old.apply(Command::RenameTask { uid: summary, name: "Standup".into() }).unwrap();
    let spec = series_spec(&old.project, summary);
    assert!(spec.resource_uids.is_empty());
    assert_eq!(spec.resources, vec![SeriesResource { uid: 1, units: 1.0 }]);
    for task in occurrences(&old.project, summary) {
        assert_eq!(units_of(&old.project, task.uid), vec![(1, 1.0, 120)]);
    }
    // The migrated spec serializes without the legacy field.
    let json = serde_json::to_string(&spec).unwrap();
    assert!(!json.contains("resource_uids"));
}

// ---- Time-phased actual cost and S-curve ----

/// Parallel leaf tasks starting Monday 08:00 with the given durations in minutes.
fn leaf_project(durations: &[i64]) -> Project {
    let mut project = Project::new("EVM", at(0, 8, 0));
    for (index, duration) in durations.iter().enumerate() {
        let mut task = Task::new(index as u32 + 1, format!("T{}", index + 1));
        task.duration = *duration;
        task.work = *duration;
        task.remaining_duration = *duration;
        task.remaining_work = *duration;
        project.tasks.push(task);
    }
    project.next_task_uid = durations.len() as u32 + 1;
    schedule(&mut project).unwrap();
    project
}

fn set_baseline_zero(task: &mut Task, cost: f64) {
    task.baselines.retain(|baseline| baseline.number != 0);
    task.baselines.push(Baseline {
        number: 0,
        start: task.start,
        finish: task.finish,
        work: task.work,
        cost,
        segments: Vec::new(),
        // These tests price the task by the accrual alone: all of it is fixed cost.
        fixed_cost: Some(cost),
    });
}

#[test]
fn actual_cost_is_cut_at_the_status_date() {
    // One two-day task: 960 working minutes at 10/h is 160 labour, plus 40 fixed cost.
    let mut project = leaf_project(&[960]);
    project.resources.push(Resource::new(1, "An", ResourceKind::Person));
    project.next_resource_uid = 2;
    let mut doc = Document::from_project(project);
    doc.apply(Command::AssignResource { task_uid: 1, resource_uid: 1, units: 1.0 }).unwrap();
    doc.apply(Command::UpdateResource {
        uid: 1,
        name: "An".into(),
        kind: ResourceKind::Person,
        max_units: 1.0,
        calendar_id: None,
        standard_rate: 10.0,
        overtime_rate: 0.0,
        cost_per_use: 0.0,
    })
    .unwrap();
    doc.apply(Command::SetFixedCost { uid: 1, cost: 40.0, accrual: Accrual::Prorated }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: 1, percent: 50.0 }).unwrap();
    let task = doc.project.task(1).unwrap();
    close(task.cost, 200.0);
    close(task.actual_cost, 100.0);
    assert_eq!(task.actual_start, Some(at(0, 8, 0)));

    // 50 % of two days is assumed done evenly over Monday 08:00 to 17:00.
    close(evm_at(&doc.project, at(0, 7, 0)).ac, 0.0);
    close(evm_at(&doc.project, at(0, 10, 0)).ac, 25.0); // 120 / 480 × 100
    close(evm_at(&doc.project, at(0, 12, 30)).ac, 50.0); // lunch: 240 / 480 × 100
    close(evm_at(&doc.project, at(0, 17, 0)).ac, 100.0);
    close(evm_at(&doc.project, at(3, 9, 0)).ac, 100.0);

    // The status date drives the reported AC, hence CV and CPI, and the snapshot AC.
    doc.apply(Command::SetStatusDate { date: Some(at(0, 12, 0)) }).unwrap();
    let m = doc.metrics().evm.metrics.unwrap();
    close(m.ac, 50.0);
    close(m.ev, 100.0);
    close(m.pv, 50.0); // 240 / 960 × 200
    close(m.cv, 50.0);
    close(m.cpi.unwrap(), 2.0);
    close(doc.burndown.last().unwrap().ac, 50.0);

    // Accrual at end: the fixed cost is incurred at the actual finish; labour as it works.
    doc.apply(Command::SetFixedCost { uid: 1, cost: 40.0, accrual: Accrual::End }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: 1, percent: 100.0 }).unwrap();
    assert_eq!(doc.project.task(1).unwrap().actual_finish, Some(at(1, 17, 0)));
    close(evm_at(&doc.project, at(1, 12, 0)).ac, 120.0); // 720 / 960 × 160 labour
    close(evm_at(&doc.project, at(1, 17, 0)).ac, 200.0);

    // Accrual at start: the fixed cost from the actual start on, labour as it works.
    doc.apply(Command::SetFixedCost { uid: 1, cost: 40.0, accrual: Accrual::Start }).unwrap();
    doc.apply(Command::SetActualStart { uid: 1, start: Some(at(0, 13, 0)) }).unwrap();
    close(evm_at(&doc.project, at(0, 12, 0)).ac, 0.0);
    close(evm_at(&doc.project, at(0, 13, 0)).ac, 40.0);
    // Starting at 13:00 the work runs to Wednesday noon: by Tuesday 17:00 720 of 960 minutes.
    close(evm_at(&doc.project, at(1, 17, 0)).ac, 160.0);
    let finish = doc.project.task(1).unwrap().finish;
    close(evm_at(&doc.project, finish).ac, 200.0);
}

fn point(date: i64, pv: f64, progress: Option<(f64, f64)>) -> SCurvePoint {
    SCurvePoint { date, pv, ev: progress.map(|(ev, _)| ev), ac: progress.map(|(_, ac)| ac) }
}

/// Three parallel two-day tasks (Monday 08:00 to Tuesday 17:00), baseline cost 1000
/// each, accruing at start, prorated and at end. Start: 50 % done, AC 600.
/// Prorated: 25 % done, AC 400. End: not started.
fn progressed_project() -> Project {
    let mut project = leaf_project(&[960, 960, 960]);
    let setup = [(Accrual::Start, 50.0, 600.0), (Accrual::Prorated, 25.0, 400.0), (Accrual::End, 0.0, 0.0)];
    for (task, (accrual, pct, cost)) in project.tasks.iter_mut().zip(setup) {
        task.fixed_cost_accrual = accrual;
        set_baseline_zero(task, 1000.0);
        task.percent_complete = pct;
        task.actual_cost = cost;
        if pct > 0.0 {
            task.actual_start = Some(at(0, 8, 0));
        }
    }
    project
}

#[test]
fn s_curve_points_match_hand_computed_values() {
    let mut project = progressed_project();
    project.status_date = Some(at(1, 12, 0));
    let curve = s_curve(&project);
    assert_eq!(curve.step, CurveStep::Day);
    assert_eq!(curve.baseline_finish, Some(at(1, 17, 0)));
    assert_eq!(
        curve.points,
        vec![
            point(at(0, 0, 0), 0.0, Some((0.0, 0.0))),
            // PV 1000 + 480/960 × 1000. EV 500 + 250 (the prorated 25 % ended Monday 12:00).
            point(at(1, 0, 0), 1500.0, Some((750.0, 1000.0))),
            // Status date: PV 1000 + 720/960 × 1000.
            point(at(1, 12, 0), 1750.0, Some((750.0, 1000.0))),
            // PV runs to the baseline finish; EV and AC stop at the status date.
            point(at(1, 17, 0), 3000.0, None),
        ]
    );
    // The status-date point equals the reported metrics.
    let m = evm_at(&project, at(1, 12, 0));
    close(curve.points[2].pv, m.pv);
    close(curve.points[2].ev.unwrap(), m.ev);
    close(curve.points[2].ac.unwrap(), m.ac);

    // Mid-window AC follows each accrual: start 600 + prorated 120/240 × 400.
    close(evm_at(&project, at(0, 10, 0)).ac, 800.0);

    // A status date past the baseline finish extends the curve with flat PV.
    project.status_date = Some(at(2, 10, 0));
    let curve = s_curve(&project);
    let dates: Vec<i64> = curve.points.iter().map(|point| point.date).collect();
    assert_eq!(dates, vec![at(0, 0, 0), at(1, 0, 0), at(1, 17, 0), at(2, 0, 0), at(2, 10, 0)]);
    assert!(curve.points.iter().all(|point| point.ev.is_some() && point.ac.is_some()));
    close(curve.points.last().unwrap().pv, 3000.0);
}

#[test]
fn s_curve_samples_weekly_for_long_projects_and_needs_a_baseline() {
    // 100 working days span far more than 92 calendar days.
    let mut project = leaf_project(&[100 * 480]);
    set_baseline_zero(&mut project.tasks[0], 10_000.0);
    let curve = s_curve(&project);
    assert_eq!(curve.step, CurveStep::Week);
    assert_eq!(curve.points[0].date, at(0, 0, 0)); // Monday midnight
    assert_eq!(curve.points[1].date - curve.points[0].date, 7 * 86_400_000);
    // Without a status date only PV is present.
    assert!(curve.points.iter().all(|point| point.ev.is_none() && point.ac.is_none()));
    let last = curve.points.last().unwrap();
    assert_eq!(last.date, project.tasks[0].finish);
    close(last.pv, 10_000.0);
    assert!(curve.points.windows(2).all(|pair| pair[0].pv <= pair[1].pv));

    assert!(s_curve(&leaf_project(&[480])).points.is_empty());
}
