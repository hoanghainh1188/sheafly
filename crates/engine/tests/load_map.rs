//! The load map (phase 18 P3, step 3d; capacity plan PR 5, #150): work and capacity by
//! person, role or skill, by day, week or month, with unassigned demand and baselines.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::availability::AvailabilityPeriod;
use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue};
use sheafly_engine::load_map::{load_map, Bucket, LoadCell, LoadGroup, MAX_BUCKETS};
use sheafly_engine::skill_levels::{SkillLevelInput, SkillLevelOf};
use sheafly_engine::model::{ResourceKind, Task};
use sheafly_engine::skills::{AssignmentRule, Condition, RuleTarget};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const DAY: i64 = 480;

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = monday().checked_add_signed(chrono::Duration::days(i64::from(day))).unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
}

fn day(offset: i64) -> NaiveDate {
    monday() + chrono::Duration::days(offset)
}

/// Two people; task 1 (two days) on An full time, task 2 (one day) on Bình at half time.
fn doc() -> Document {
    let mut project = Project::new("Tải", at(0, 8, 0));
    for (uid, minutes) in [(1, 2 * DAY), (2, DAY)] {
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.duration = minutes;
        task.remaining_duration = minutes;
        task.work = minutes;
        task.remaining_work = minutes;
        project.tasks.push(task);
    }
    project.next_task_uid = 3;
    let mut doc = Document::from_project(project);
    for name in ["An", "Bình"] {
        doc.apply(Command::AddResource { name: name.into(), kind: ResourceKind::Person }).unwrap();
    }
    doc.apply(Command::AssignResource { task_uid: 1, resource_uid: 1, units: 1.0 }).unwrap();
    doc.apply(Command::AssignResource { task_uid: 2, resource_uid: 2, units: 0.5 }).unwrap();
    doc
}

fn cell(work: i64, capacity: i64) -> LoadCell {
    LoadCell {
        work,
        unassigned: 0,
        capacity,
        baseline: None,
    }
}

#[test]
fn gives_work_and_capacity_by_person_and_day() {
    let doc = doc();
    let map = load_map(&doc.project, day(0), day(6), Bucket::Day, LoadGroup::Person, None).unwrap();
    assert_eq!(map.buckets, ["2026-01-05", "2026-01-06", "2026-01-07", "2026-01-08", "2026-01-09", "2026-01-10", "2026-01-11"]);
    let an = &map.rows[0];
    assert_eq!(an.resource_uid, Some(1));
    assert_eq!(an.cells[..3], [cell(DAY, DAY), cell(DAY, DAY), cell(0, DAY)]);
    // The weekend has no working time.
    assert_eq!(an.cells[5], cell(0, 0));
    assert_eq!(map.rows[1].cells[0], cell(DAY / 2, DAY));
}

#[test]
fn weeks_start_on_monday_and_sum_their_days() {
    let doc = doc();
    let map = load_map(&doc.project, day(2), day(9), Bucket::Week, LoadGroup::Person, None).unwrap();
    assert_eq!(map.buckets, ["2026-01-05", "2026-01-12"]);
    assert_eq!(map.rows[0].cells, [cell(2 * DAY, 5 * DAY), cell(0, 5 * DAY)]);
}

#[test]
fn capacity_follows_availability_and_the_resource_calendar() {
    let mut doc = doc();
    let periods = vec![AvailabilityPeriod { from: Some(day(1)), to: Some(day(1)), units: 0.0 }];
    doc.apply(Command::SetResourceAvailability { uid: 1, periods }).unwrap();
    doc.apply(Command::AddHoliday { date: "2026-01-07".into() }).unwrap();
    let map = load_map(&doc.project, day(0), day(2), Bucket::Day, LoadGroup::Person, None).unwrap();
    let capacity: Vec<i64> = map.rows[0].cells.iter().map(|cell| cell.capacity).collect();
    assert_eq!(capacity, [DAY, 0, 0]);
    assert_eq!(map.rows[1].cells[2].capacity, 0);
}

#[test]
fn months_align_to_calendar_month_boundaries() {
    let doc = doc();
    // 2026-01-15 to 2026-03-10 covers whole months Jan, Feb, Mar (3 buckets)
    let jan15 = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let mar10 = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
    let map = load_map(&doc.project, jan15, mar10, Bucket::Month, LoadGroup::Person, None).unwrap();
    assert_eq!(map.buckets, ["2026-01-01", "2026-02-01", "2026-03-01"]);
    // Task 1 was in Jan (2 * 480 min work)
    assert_eq!(map.rows[0].cells[0].work, 2 * DAY);
    assert_eq!(map.rows[0].cells[1].work, 0);
    // Jan 2026 has 22 working days (Mon-Fri) = 22 * 480 = 10,560 minutes
    assert_eq!(map.rows[0].cells[0].capacity, 22 * DAY);
    // Feb 2026 has 20 working days = 20 * 480 = 9,600 minutes
    assert_eq!(map.rows[0].cells[1].capacity, 20 * DAY);
}

#[test]
fn groups_aggregate_by_role() {
    let mut doc = doc();
    // Assign An and Bình to roles
    doc.apply(Command::SetResourceGroup { uid: 1, group: "Backend".into() }).unwrap();
    doc.apply(Command::SetResourceGroup { uid: 2, group: "Backend".into() }).unwrap();

    let map = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Group, None).unwrap();
    assert_eq!(map.rows.len(), 1);
    assert_eq!(map.rows[0].label, "Backend");
    assert_eq!(map.rows[0].resource_uid, None);
    // An has 2 days, Bình has 1 day work: total 3 days work
    assert_eq!(map.rows[0].cells[0].work, 3 * DAY);
    // Combined capacity: 2 people * 5 working days * 480 = 10 days = 4800 min
    assert_eq!(map.rows[0].cells[0].capacity, 10 * DAY);
}

/// Skill "Rust" (id 1) for An, and task 3 (one day, unassigned) whose rule targets it.
fn add_rust_rule(doc: &mut Document) {
    // Add skill "Rust" (id 1)
    doc.apply(Command::AddSkill { name: "Rust".into() }).unwrap();
    doc.apply(Command::SetResourceSkills { uid: 1, skills: vec![1], levels: vec![] }).unwrap();

    // Add custom field and an unassigned task matching a rule for Rust
    doc.apply(Command::AddField { name: "Module".into(), kind: FieldKind::Text }).unwrap();
    doc.apply(Command::AddTask { name: "New Feature".into(), after_uid: None }).unwrap();
    // Task 3: 8 hours work
    doc.apply(Command::SetDuration { uid: 3, minutes: DAY }).unwrap();
    doc.apply(Command::SetFieldValue { uid: 3, field_id: 1, value: Some(FieldValue::Text("Core".into())) }).unwrap();

    // Rule: if Module == Core -> assign to Rust skill
    let rules = vec![AssignmentRule {
        conditions: vec![Condition { field: 1, values: vec![FieldValue::Text("Core".into())] }],
        target: RuleTarget::Skill { id: 1, min_level: None },
        units: 1.0,
    }];
    doc.apply(Command::SetAssignmentRules { rules }).unwrap();
}

#[test]
fn skill_group_includes_members_and_unassigned_rule_demand() {
    let mut doc = doc();
    add_rust_rule(&mut doc);

    let map = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Skill { id: 1, min_level: None }, None).unwrap();
    // Row 0 is the skill summary row, Row 1 is member An
    assert_eq!(map.rows.len(), 2);
    assert_eq!(map.rows[0].label, "Rust");
    assert_eq!(map.rows[0].resource_uid, None);
    // Member An has 2 days assigned work
    assert_eq!(map.rows[0].cells[0].work, 2 * DAY);
    // Unassigned task 3 has 1 day work targeting Rust
    assert_eq!(map.rows[0].cells[0].unassigned, DAY);
    // Member An's row
    assert_eq!(map.rows[1].label, "An");
    assert_eq!(map.rows[1].resource_uid, Some(1));
    assert_eq!(map.rows[1].cells[0].work, 2 * DAY);
    assert_eq!(map.rows[1].cells[0].unassigned, 0);
}

#[test]
fn skill_with_min_level_filters_members() {
    let mut doc = doc();
    doc.apply(Command::AddSkill { name: "Dev".into() }).unwrap();
    doc.apply(Command::SetSkillLevels {
        levels: vec![
            SkillLevelInput { id: None, name: "Junior".into() },
            SkillLevelInput { id: None, name: "Senior".into() },
        ],
    }).unwrap();
    // An is Junior (id 1), Bình is Senior (id 2)
    doc.apply(Command::SetResourceSkills { uid: 1, skills: vec![1], levels: vec![SkillLevelOf { skill: 1, level: 1 }] }).unwrap();
    doc.apply(Command::SetResourceSkills { uid: 2, skills: vec![1], levels: vec![SkillLevelOf { skill: 1, level: 2 }] }).unwrap();

    // Query skill 1 with min_level Senior (id 2) -> only Bình qualifies
    let map = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Skill { id: 1, min_level: Some(2) }, None).unwrap();
    assert_eq!(map.rows.len(), 2); // Summary + Bình
    assert_eq!(map.rows[1].label, "Bình");
    assert_eq!(map.rows[1].resource_uid, Some(2));
}

#[test]
fn baseline_load_included_when_requested() {
    let mut doc = doc();
    // Set baseline 0
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    let map = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Person, Some(0)).unwrap();
    // An had 2 days = 960 min baseline work in week 1
    assert_eq!(map.rows[0].cells[0].baseline, Some(2 * DAY));
    // Bình had 1 day = 480 min baseline work
    assert_eq!(map.rows[1].cells[0].baseline, Some(DAY));
}

#[test]
fn unassigned_demand_leaves_out_started_and_finished_work() {
    let skill = LoadGroup::Skill { id: 1, min_level: None };
    for percent in [50.0, 100.0] {
        let mut doc = doc();
        add_rust_rule(&mut doc);
        doc.apply(Command::SetPercentComplete { uid: 3, percent }).unwrap();
        let map = load_map(&doc.project, day(0), day(4), Bucket::Week, skill.clone(), None).unwrap();
        assert_eq!(map.rows[0].cells[0].unassigned, 0, "{percent}% done");
    }
}

#[test]
fn baseline_load_adds_up_by_role_and_skill() {
    let mut doc = doc();
    add_rust_rule(&mut doc);
    doc.apply(Command::SetResourceSkills { uid: 2, skills: vec![1], levels: vec![] }).unwrap();
    doc.apply(Command::SetResourceGroup { uid: 1, group: "Backend".into() }).unwrap();
    doc.apply(Command::SetResourceGroup { uid: 2, group: "Backend".into() }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    let role = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Group, Some(0)).unwrap();
    assert_eq!(role.rows[0].cells[0].baseline, Some(3 * DAY));
    let skill = LoadGroup::Skill { id: 1, min_level: None };
    let skill = load_map(&doc.project, day(0), day(4), Bucket::Day, skill, Some(0)).unwrap();
    assert_eq!(skill.rows[0].cells[0].baseline, Some(DAY + DAY / 2));
    assert_eq!(skill.rows[1].cells[1].baseline, Some(DAY));
    // A day outside every baseline stays empty.
    assert_eq!(skill.rows[2].cells[3].baseline, None);
    // Without a baseline nothing is read.
    let none = load_map(&doc.project, day(0), day(4), Bucket::Week, LoadGroup::Group, None).unwrap();
    assert_eq!(none.rows[0].cells[0].baseline, None);
}

#[test]
fn refuses_a_backwards_or_too_long_span() {
    let doc = doc();
    let invalid = |from, to, bucket| match load_map(&doc.project, from, to, bucket, LoadGroup::Person, None) {
        Err(EngineError::Invalid(reason)) => reason,
        other => panic!("expected an error, got {other:?}"),
    };
    assert_eq!(invalid(day(3), day(1), Bucket::Day), Invalid::BadDate);
    assert_eq!(invalid(day(0), day(MAX_BUCKETS as i64), Bucket::Day), Invalid::DateRangeTooLong);
    assert!(load_map(&doc.project, day(0), day(7 * (MAX_BUCKETS as i64 - 1)), Bucket::Week, LoadGroup::Person, None).is_ok());
}

#[test]
fn five_hundred_people_over_a_year_of_weeks_is_quick() {
    let mut project = Project::new("Lớn", at(0, 8, 0));
    for uid in 1..=500 {
        project.resources.push(sheafly_engine::model::Resource::new(uid, format!("Người {uid}"), ResourceKind::Person));
    }
    let started = std::time::Instant::now();
    let map = load_map(&project, day(0), day(364), Bucket::Week, LoadGroup::Person, None).unwrap();
    assert_eq!(map.rows.len(), 500);
    assert_eq!(map.buckets.len(), 53);
    assert!(started.elapsed().as_millis() < 2000);
}
