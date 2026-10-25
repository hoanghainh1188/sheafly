//! The reads behind the AI model's tools (phase 22, PR C, D4): one task in detail, a person's
//! load by week and the chain driving a task, written like the summary and masked alike.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::command::Command;
use sheafly_engine::model::{ConstraintType, DepKind, ResourceKind};
use sheafly_engine::summary_tools::{critical_chain, resource_load, task_detail, tool_date};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, Project};

const DAY: i64 = 480;

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date =
        NaiveDate::from_ymd_opt(2026, 1, 5).unwrap() + chrono::Duration::days(i64::from(day));
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

fn apply(doc: &mut Document, command: Command) {
    doc.apply(command).expect("command applies");
}

fn add_task(doc: &mut Document, name: &str, minutes: i64) -> u32 {
    let uid = doc.project.next_task_uid;
    apply(
        doc,
        Command::AddTask {
            name: name.into(),
            after_uid: None,
        },
    );
    apply(doc, Command::SetDuration { uid, minutes });
    uid
}

fn link(doc: &mut Document, predecessor_uid: u32, successor_uid: u32) {
    apply(
        doc,
        Command::AddDependency {
            predecessor_uid,
            successor_uid,
            kind: DepKind::Fs,
            lag: 0,
        },
    );
}

fn add_person(doc: &mut Document, name: &str) -> u32 {
    let uid = doc.project.next_resource_uid;
    apply(
        doc,
        Command::AddResource {
            name: name.into(),
            kind: ResourceKind::Person,
        },
    );
    uid
}

fn assign(doc: &mut Document, task_uid: u32, resource_uid: u32) {
    apply(
        doc,
        Command::AssignResource {
            task_uid,
            resource_uid,
            units: 1.0,
        },
    );
}

/// Thiết kế (2 days) → Lập trình (3 days) → Bàn giao, both on An; Viết tài liệu (1 day) apart.
fn chain() -> (Document, u32) {
    let mut doc = Document::from_project(Project::new("Cổng thanh toán", at(0, 8, 0)));
    apply(
        &mut doc,
        Command::RenameTask {
            uid: 1,
            name: "Thiết kế".into(),
        },
    );
    apply(
        &mut doc,
        Command::SetDuration {
            uid: 1,
            minutes: 2 * DAY,
        },
    );
    let code = add_task(&mut doc, "Lập trình", 3 * DAY);
    add_task(&mut doc, "Viết tài liệu", DAY);
    let done = doc.project.next_task_uid;
    apply(
        &mut doc,
        Command::AddMilestone {
            name: "Bàn giao".into(),
            after_uid: None,
        },
    );
    link(&mut doc, 1, code);
    link(&mut doc, code, done);
    let an = add_person(&mut doc, "An");
    assign(&mut doc, 1, an);
    assign(&mut doc, code, an);
    (doc, an)
}

#[test]
fn a_task_in_detail_has_its_links_people_and_dates() {
    let (doc, an) = chain();
    let detail = task_detail(&doc.project, 2, false).expect("task 2");
    assert_eq!(detail.line.id, "T2");
    assert_eq!(detail.line.name, "Lập trình");
    assert_eq!(detail.line.after, vec!["T1"]);
    assert_eq!(detail.successors, vec!["T4"]);
    assert_eq!(detail.assignments.len(), 1);
    assert_eq!(detail.assignments[0].resource, format!("R{an}"));
    assert_eq!(detail.assignments[0].name, "An");
    assert_eq!(detail.assignments[0].work_h, 24.0);
    assert_eq!(detail.early_start, "2026-01-07 08:00");
    assert_eq!(detail.late_finish, "2026-01-09 17:00");
    assert_eq!(detail.task_type, "fixed_units");
    assert_eq!(detail.baseline_start, None);
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["id"], "T2", "the summary line is flattened");
    assert!(json.get("manual").is_none() && json.get("children").is_none());
    assert_eq!(task_detail(&doc.project, 99, false), None);
}

#[test]
fn masking_hides_task_and_people_names_in_every_read() {
    let (doc, an) = chain();
    let detail = task_detail(&doc.project, 2, true).unwrap();
    assert_eq!(detail.line.name, "Task 2");
    assert_eq!(detail.assignments[0].name, format!("Person {an}"));
    let load = resource_load(&doc.project, an, day(0), day(6), true)
        .unwrap()
        .unwrap();
    assert_eq!(load.name, format!("Person {an}"));
    assert!(load.tasks.iter().all(|task| task.name.starts_with("Task ")));
    let steps = critical_chain(&doc.project, 4, true).unwrap();
    assert!(steps.iter().all(|step| step.name.starts_with("Task ")));
    let text = serde_json::to_string(&(detail, load, steps)).unwrap();
    for name in ["Lập trình", "Thiết kế", "\"An\""] {
        assert!(!text.contains(name), "{name} leaked");
    }
}

fn day(offset: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap() + chrono::Duration::days(offset)
}

#[test]
fn a_persons_load_is_read_by_week_with_their_tasks() {
    let (doc, an) = chain();
    let load = resource_load(&doc.project, an, day(0), day(13), false)
        .unwrap()
        .unwrap();
    assert_eq!(load.id, format!("R{an}"));
    assert_eq!(load.weeks.len(), 2);
    assert_eq!(load.weeks[0].week, "2026-01-05");
    assert_eq!(
        (load.weeks[0].work_h, load.weeks[0].capacity_h),
        (40.0, 40.0)
    );
    assert_eq!(load.weeks[0].percent, Some(100));
    assert_eq!(
        (load.weeks[1].work_h, load.weeks[1].percent),
        (0.0, Some(0))
    );
    let ids: Vec<&str> = load.tasks.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, ["T1", "T2"]);
    let later = resource_load(&doc.project, an, day(7), day(13), false)
        .unwrap()
        .unwrap();
    assert!(
        later.tasks.is_empty(),
        "tasks outside the span are left out"
    );
}

#[test]
fn a_load_span_must_run_forward_and_stay_within_two_years() {
    let (doc, an) = chain();
    assert!(resource_load(&doc.project, an, day(7), day(0), false).is_err());
    assert!(resource_load(&doc.project, an, day(0), day(104 * 7), false).is_err());
    assert!(resource_load(&doc.project, an, day(0), day(104 * 7 - 1), false).is_ok());
    assert_eq!(
        resource_load(&doc.project, 99, day(0), day(6), false).unwrap(),
        None
    );
    assert_eq!(tool_date(" 2026-01-05 "), Some(day(0)));
    assert_eq!(tool_date("05/01/2026"), None);
}

#[test]
fn the_chain_steps_back_through_driving_predecessors() {
    let (doc, _) = chain();
    let steps = critical_chain(&doc.project, 4, false).unwrap();
    let ids: Vec<&str> = steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["T1", "T2", "T4"]);
    assert_eq!(steps[0].after, None);
    assert_eq!(steps[1].after.as_deref(), Some("T1"));
    assert_eq!(steps[2].after.as_deref(), Some("T2"));
    assert!(steps.iter().all(|step| step.critical));
    let alone = critical_chain(&doc.project, 3, false).unwrap();
    assert_eq!(
        alone.len(),
        1,
        "a task with no predecessor is its own chain"
    );
    assert_eq!(critical_chain(&doc.project, 99, false), None);
}

#[test]
fn a_group_link_is_followed_to_the_leaf_that_finishes_last() {
    let mut doc = Document::from_project(Project::new("Nhóm", at(0, 8, 0)));
    let short = add_task(&mut doc, "Ngắn", DAY);
    let long = add_task(&mut doc, "Dài", 3 * DAY);
    let after = add_task(&mut doc, "Sau", DAY);
    // Task 1 becomes the group of the two tasks right under it.
    apply(&mut doc, Command::Indent { uid: short });
    apply(&mut doc, Command::Indent { uid: long });
    link(&mut doc, 1, after);
    let steps = critical_chain(&doc.project, after, false).unwrap();
    let ids: Vec<String> = steps.iter().map(|step| step.id.clone()).collect();
    assert_eq!(ids, [format!("T{long}"), format!("T{after}")]);
    assert_eq!(steps[1].after.as_deref(), Some("T1"));
    // Asking about the group starts from its last leaf.
    let group = critical_chain(&doc.project, 1, false).unwrap();
    assert_eq!(group.last().unwrap().id, format!("T{long}"));
}

#[test]
fn a_started_task_ends_the_chain() {
    let (mut doc, _) = chain();
    apply(
        &mut doc,
        Command::SetActualStart {
            uid: 2,
            start: Some(at(2, 8, 0)),
        },
    );
    let steps = critical_chain(&doc.project, 4, false).unwrap();
    let ids: Vec<&str> = steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["T2", "T4"]);
}

#[test]
fn a_constraint_that_sets_the_start_ends_the_chain() {
    let (mut doc, _) = chain();
    // Lập trình may not start before Thursday: the constraint, not Thiết kế, sets its start.
    apply(
        &mut doc,
        Command::SetConstraint {
            uid: 2,
            constraint_type: ConstraintType::Snet,
            date: Some(at(3, 8, 0)),
        },
    );
    let steps = critical_chain(&doc.project, 4, false).unwrap();
    let ids: Vec<&str> = steps.iter().map(|step| step.id.as_str()).collect();
    assert_eq!(ids, ["T2", "T4"]);
    assert_eq!(
        steps[0].constraint.as_deref(),
        Some("SNET 2026-01-08 08:00")
    );
}
