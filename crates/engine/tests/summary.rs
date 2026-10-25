//! The project summary for AI recommendations (phase 22, PR A, #100): tasks and people by
//! UID, dates and hours, slack, critical path, overloaded weeks, skills and rules, masking
//! of names (D3) and cutting the summary to a size limit (D7).

use chrono::{Datelike, NaiveDate};

use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue};
use sheafly_engine::model::{ConstraintType, DepKind, ResourceKind};
use sheafly_engine::skills::{AssignmentRule, Condition, RuleTarget};
use sheafly_engine::summary::{project_summary, ProjectSummary, SummaryOptions};
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

/// Thiết kế (2 days) → Lập trình (3 days) on An, Viết tài liệu (1 day) alongside on Bình,
/// and a milestone after Lập trình, which the engine places at the next working start.
fn chain() -> Document {
    // A new document starts with one task: it becomes Thiết kế, so the UIDs are 1 to 4.
    let mut doc = Document::from_project(Project::new("Cổng thanh toán", at(0, 8, 0)));
    let design = 1;
    apply(
        &mut doc,
        Command::RenameTask {
            uid: design,
            name: "Thiết kế".into(),
        },
    );
    apply(
        &mut doc,
        Command::SetDuration {
            uid: design,
            minutes: 2 * DAY,
        },
    );
    let code = add_task(&mut doc, "Lập trình", 3 * DAY);
    let docs = add_task(&mut doc, "Viết tài liệu", DAY);
    let done = doc.project.next_task_uid;
    apply(
        &mut doc,
        Command::AddMilestone {
            name: "Bàn giao".into(),
            after_uid: None,
        },
    );
    link(&mut doc, design, code);
    link(&mut doc, code, done);
    let an = add_person(&mut doc, "An");
    let binh = add_person(&mut doc, "Bình");
    assign(&mut doc, design, an);
    assign(&mut doc, code, an);
    assign(&mut doc, docs, binh);
    doc
}

fn summary(project: &Project) -> ProjectSummary {
    project_summary(project, &SummaryOptions::default())
}

fn json(summary: &ProjectSummary) -> String {
    serde_json::to_string(summary).expect("summary json")
}

#[test]
fn refers_to_tasks_and_people_by_uid_with_dates_and_hours() {
    let doc = chain();
    let summary = summary(&doc.project);

    assert_eq!(summary.project.name, "Cổng thanh toán");
    assert_eq!(summary.project.start, "2026-01-05 08:00");
    assert_eq!(summary.project.finish, "2026-01-12 08:00");
    assert_eq!(summary.project.leaves, 4);
    assert_eq!(summary.project.milestones, 1);
    assert!(!summary.truncated);
    assert_eq!(summary.omitted_tasks, 0);

    let code = summary
        .tasks
        .iter()
        .find(|task| task.id == "T2")
        .expect("T2");
    assert_eq!(code.name, "Lập trình");
    assert_eq!(code.start, "2026-01-07 08:00");
    assert_eq!(code.finish, "2026-01-09 17:00");
    assert_eq!(code.duration_h, 24.0);
    assert_eq!(code.work_h, 24.0);
    assert_eq!(code.slack_h, 0.0);
    assert!(code.critical);
    assert_eq!(code.after, vec!["T1"]);
    assert_eq!(code.people, vec!["R1"]);

    let docs = summary
        .tasks
        .iter()
        .find(|task| task.id == "T3")
        .expect("T3");
    assert!(!docs.critical);
    assert_eq!(docs.slack_h, 32.0);
    assert_eq!(docs.people, vec!["R2"]);

    assert_eq!(summary.critical_path, vec!["T1", "T2", "T4"]);
    let an = summary
        .resources
        .iter()
        .find(|resource| resource.id == "R1")
        .expect("R1");
    assert_eq!(an.name, "An");
    assert_eq!(an.work_h, 40.0);
    assert!(!an.overallocated);
}

#[test]
fn links_other_than_finish_to_start_show_their_kind_and_lag() {
    let mut doc = chain();
    apply(
        &mut doc,
        Command::AddDependency {
            predecessor_uid: 1,
            successor_uid: 3,
            kind: DepKind::Ss,
            lag: 120,
        },
    );
    let summary = summary(&doc.project);
    let docs = summary
        .tasks
        .iter()
        .find(|task| task.id == "T3")
        .expect("T3");
    assert_eq!(docs.after, vec!["T1 SS+2h"]);
}

#[test]
fn counts_slack_spread_late_tasks_and_missed_deadlines() {
    let mut doc = chain();
    apply(
        &mut doc,
        Command::SetDeadline {
            uid: 2,
            date: Some(at(3, 17, 0)),
        },
    );
    apply(
        &mut doc,
        Command::SetConstraint {
            uid: 3,
            constraint_type: ConstraintType::Snet,
            date: Some(at(1, 8, 0)),
        },
    );
    let summary = summary(&doc.project);

    assert_eq!(summary.project.deadlines_missed, 1);
    assert!(summary.project.late >= 1);
    let code = summary
        .tasks
        .iter()
        .find(|task| task.id == "T2")
        .expect("T2");
    assert_eq!(code.deadline.as_deref(), Some("2026-01-08 17:00"));
    assert!(code.late);
    let docs = summary
        .tasks
        .iter()
        .find(|task| task.id == "T3")
        .expect("T3");
    assert_eq!(docs.constraint.as_deref(), Some("SNET 2026-01-06 08:00"));

    // Open leaves that are not milestones: T1 and T2 critical, T3 with 24 h of slack.
    assert_eq!(summary.slack.zero, 2);
    assert_eq!(summary.slack.to_week, 1);
    assert_eq!(
        summary.slack.under_day + summary.slack.to_month + summary.slack.over_month,
        0
    );
}

#[test]
fn lists_the_weeks_a_person_is_overloaded() {
    let mut doc = chain();
    // Bình also tests on day one while writing the docs: two full-time tasks on Monday.
    let test = add_task(&mut doc, "Kiểm thử", DAY);
    assign(&mut doc, test, 2);
    let summary = summary(&doc.project);

    assert_eq!(summary.project.overallocated_people, 1);
    let binh = summary
        .resources
        .iter()
        .find(|resource| resource.id == "R2")
        .expect("R2");
    assert!(binh.overallocated);
    assert_eq!(binh.overloaded_weeks.len(), 1);
    let week = &binh.overloaded_weeks[0];
    assert_eq!(week.week, "2026-01-05");
    assert_eq!(week.work_h, 16.0);
    assert_eq!(week.capacity_h, 40.0);
    // Busy days, not the week's total, decide overload; the week shows the peak day.
    assert_eq!(week.peak_percent, 200);
    let an = summary
        .resources
        .iter()
        .find(|resource| resource.id == "R1")
        .expect("R1");
    assert!(an.overloaded_weeks.is_empty());
}

/// A list field Mảng (Thanh toán, Báo cáo), a skill Backend with levels Junior < Senior, a
/// rule sending Thanh toán to Backend from Senior, and an unassigned task in Thanh toán.
fn with_rules() -> Document {
    let mut doc = chain();
    let options = vec!["Thanh toán".to_string(), "Báo cáo".to_string()];
    apply(
        &mut doc,
        Command::AddField {
            name: "Mảng".into(),
            kind: FieldKind::Choice { options },
        },
    );
    apply(
        &mut doc,
        Command::AddField {
            name: "Khách hàng".into(),
            kind: FieldKind::Text,
        },
    );
    apply(
        &mut doc,
        Command::AddSkill {
            name: "Backend".into(),
        },
    );
    apply(
        &mut doc,
        Command::SetSkillLevels {
            levels: vec![
                sheafly_engine::skill_levels::SkillLevelInput {
                    id: None,
                    name: "Junior".into(),
                },
                sheafly_engine::skill_levels::SkillLevelInput {
                    id: None,
                    name: "Senior".into(),
                },
            ],
        },
    );
    apply(
        &mut doc,
        Command::SetResourceSkills {
            uid: 1,
            skills: vec![1],
            levels: vec![sheafly_engine::skill_levels::SkillLevelOf { skill: 1, level: 2 }],
        },
    );
    let refund = add_task(&mut doc, "Hoàn tiền", 2 * DAY);
    apply(
        &mut doc,
        Command::SetFieldValue {
            uid: refund,
            field_id: 1,
            value: Some(FieldValue::Text("Thanh toán".into())),
        },
    );
    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![
                AssignmentRule {
                    conditions: vec![Condition {
                        field: 1,
                        values: vec![FieldValue::Text("Thanh toán".into())],
                    }],
                    target: RuleTarget::Skill {
                        id: 1,
                        min_level: Some(2),
                    },
                    units: 1.0,
                },
                AssignmentRule {
                    conditions: vec![Condition {
                        field: 2,
                        values: vec![FieldValue::Text("Ngân hàng Á Châu".into())],
                    }],
                    target: RuleTarget::Resource { uid: 2 },
                    units: 0.5,
                },
            ],
        },
    );
    doc
}

#[test]
fn reports_skills_rules_and_unassigned_demand() {
    let doc = with_rules();
    let summary = summary(&doc.project);

    assert_eq!(summary.project.unassigned, 1);
    let an = summary
        .resources
        .iter()
        .find(|resource| resource.id == "R1")
        .expect("R1");
    assert_eq!(an.skills, vec!["Backend:Senior"]);
    let backend = summary
        .skills
        .iter()
        .find(|skill| skill.name == "Backend")
        .expect("Backend");
    assert_eq!(backend.people, 1);
    assert_eq!(backend.unassigned_tasks, 1);
    assert_eq!(backend.unassigned_work_h, 16.0);
    assert_eq!(summary.rules.len(), 2);
    assert_eq!(summary.rules[0].when, vec!["Mảng = Thanh toán"]);
    assert_eq!(summary.rules[0].to, "skill Backend ≥ Senior");
    assert_eq!(summary.rules[0].units, 1.0);
    assert_eq!(summary.rules[1].when, vec!["Khách hàng = Ngân hàng Á Châu"]);
    assert_eq!(summary.rules[1].to, "R2");
}

#[test]
fn masking_hides_names_of_the_project_tasks_people_fields_and_text_values() {
    let doc = with_rules();
    let masked = project_summary(
        &doc.project,
        &SummaryOptions {
            mask: true,
            ..SummaryOptions::default()
        },
    );
    let text = json(&masked);

    for name in [
        "Cổng thanh toán",
        "Thiết kế",
        "Lập trình",
        "Bàn giao",
        "Hoàn tiền",
        "\"An\"",
        "Bình",
        "Mảng",
        "Khách hàng",
        "Ngân hàng",
        "Thanh toán",
        "Báo cáo",
    ] {
        assert!(!text.contains(name), "{name} leaks into {text}");
    }
    assert_eq!(masked.project.name, "Project");
    let code = masked
        .tasks
        .iter()
        .find(|task| task.id == "T2")
        .expect("T2");
    assert_eq!(code.name, "Task 2");
    let binh = masked
        .resources
        .iter()
        .find(|resource| resource.id == "R2")
        .expect("R2");
    assert_eq!(binh.name, "Person 2");
    assert_eq!(masked.rules[0].when, vec!["Field 1 = Option 1"]);
    assert_eq!(masked.rules[1].when, vec!["Field 2 = Value 1"]);
    // Skills and levels are not names of people or tasks, so they stay.
    assert_eq!(masked.rules[0].to, "skill Backend ≥ Senior");

    // Everything else is the same as unmasked.
    let plain = summary(&doc.project);
    assert_eq!(masked.critical_path, plain.critical_path);
    assert_eq!(masked.tasks.len(), plain.tasks.len());
    assert_eq!(masked.project.finish, plain.project.finish);
}

#[test]
fn masking_names_machines_apart_from_people() {
    let mut doc = chain();
    let uid = doc.project.next_resource_uid;
    apply(
        &mut doc,
        Command::AddResource {
            name: "Máy in 3D".into(),
            kind: ResourceKind::Machine,
        },
    );
    let masked = project_summary(
        &doc.project,
        &SummaryOptions {
            mask: true,
            ..SummaryOptions::default()
        },
    );
    let machine = masked
        .resources
        .iter()
        .find(|resource| resource.id == format!("R{uid}"))
        .expect("machine");
    assert_eq!(machine.name, format!("Machine {uid}"));
    assert!(machine.machine);
}

#[test]
fn groups_appear_as_summary_tasks_and_parents() {
    let mut doc = chain();
    let group = doc.project.next_task_uid;
    apply(
        &mut doc,
        Command::AddTask {
            name: "Giai đoạn 1".into(),
            after_uid: None,
        },
    );
    let first = add_task(&mut doc, "Khảo sát", DAY);
    apply(&mut doc, Command::Indent { uid: first });
    let summary = summary(&doc.project);
    let phase = summary
        .tasks
        .iter()
        .find(|task| task.id == format!("T{group}"))
        .expect("group");
    assert!(phase.summary);
    let survey = summary
        .tasks
        .iter()
        .find(|task| task.id == format!("T{first}"))
        .expect("child");
    assert_eq!(survey.parent.as_deref(), Some(format!("T{group}").as_str()));
    assert_eq!(summary.project.leaves, 5);
}

/// `count` chained tasks of one day where every tenth is late against a deadline, plus
/// `count` free tasks with lots of slack.
fn large(count: u32) -> Document {
    let mut project = Project::new("Lớn", at(0, 8, 0));
    let mut uid = 1;
    for index in 0..count {
        let mut task = sheafly_engine::Task::new(uid, format!("Chuỗi {index}"));
        task.duration = DAY;
        task.remaining_duration = DAY;
        if index % 10 == 9 {
            task.deadline = Some(at(0, 17, 0));
        }
        project.tasks.push(task);
        if index > 0 {
            let dep = project.dependencies.len() as u32 + 1;
            project.dependencies.push(sheafly_engine::Dependency {
                uid: dep,
                predecessor_uid: uid - 1,
                successor_uid: uid,
                kind: DepKind::Fs,
                lag: 0,
            });
        }
        uid += 1;
    }
    for index in 0..count {
        let mut task = sheafly_engine::Task::new(uid, format!("Tự do {index}"));
        task.duration = DAY;
        task.remaining_duration = DAY;
        project.tasks.push(task);
        uid += 1;
    }
    project.next_task_uid = uid;
    project.next_dependency_uid = project.dependencies.len() as u32 + 1;
    Document::from_project(project)
}

#[test]
fn cuts_to_the_size_limit_keeping_late_and_critical_tasks_first() {
    let doc = large(150);
    let options = SummaryOptions {
        max_chars: 12_000,
        ..SummaryOptions::default()
    };
    let summary = project_summary(&doc.project, &options);

    assert!(summary.truncated);
    assert!(
        json(&summary).len() <= options.max_chars,
        "{} chars",
        json(&summary).len()
    );
    assert_eq!(summary.tasks.len() + summary.omitted_tasks, 300);
    // Late tasks come first, then critical ones, before any task with slack.
    for task in &summary.tasks {
        assert!(
            task.critical || task.late,
            "{} kept before a critical task",
            task.id
        );
    }
    let late = summary.tasks.iter().filter(|task| task.late).count();
    assert_eq!(late, 15);
    // Kept tasks are in project order, not in the order they were picked.
    let uids: Vec<u32> = summary
        .tasks
        .iter()
        .map(|task| task.id[1..].parse().unwrap())
        .collect();
    assert!(uids.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn a_kept_task_brings_its_groups_along() {
    let mut doc = chain();
    let group = doc.project.next_task_uid;
    apply(
        &mut doc,
        Command::AddTask {
            name: "Giai đoạn 1".into(),
            after_uid: None,
        },
    );
    let late = add_task(&mut doc, "Khảo sát", DAY);
    apply(&mut doc, Command::Indent { uid: late });
    apply(
        &mut doc,
        Command::SetDeadline {
            uid: late,
            date: Some(at(0, 12, 0)),
        },
    );
    for index in 0..40 {
        add_task(&mut doc, &format!("Tự do {index}"), DAY);
    }
    let options = SummaryOptions {
        max_chars: 3_000,
        ..SummaryOptions::default()
    };
    let summary = project_summary(&doc.project, &options);

    assert!(summary.truncated);
    let kept = summary
        .tasks
        .iter()
        .find(|task| task.id == format!("T{late}"))
        .expect("late task kept");
    assert_eq!(kept.parent.as_deref(), Some(format!("T{group}").as_str()));
    assert!(
        summary
            .tasks
            .iter()
            .any(|task| task.id == format!("T{group}")),
        "group missing"
    );
    assert!(json(&summary).len() <= options.max_chars);
}

#[test]
fn keeps_everything_under_the_default_limit_and_handles_an_empty_project() {
    let doc = large(150);
    let summary = summary(&doc.project);
    assert!(!summary.truncated);
    assert_eq!(summary.tasks.len(), 300);

    let empty = Project::new("Trống", at(0, 8, 0));
    let summary = project_summary(&empty, &SummaryOptions::default());
    assert!(summary.tasks.is_empty());
    assert!(summary.critical_path.is_empty());
    assert_eq!(summary.project.leaves, 0);
    assert_eq!(summary.project.finish, "2026-01-05 08:00");
}

#[test]
fn people_take_at_most_a_third_of_the_room_so_tasks_still_fit() {
    let mut doc = Document::from_project(Project::new("Đông người", at(0, 8, 0)));
    for index in 0..30 {
        let person = add_person(&mut doc, &format!("Người {index}"));
        for _ in 0..2 {
            let task = add_task(&mut doc, &format!("Việc của {index}"), DAY);
            assign(&mut doc, task, person);
        }
    }
    let options = SummaryOptions { max_chars: 8_000, ..SummaryOptions::default() };
    let summary = project_summary(&doc.project, &options);

    assert_eq!(summary.project.overallocated_people, 30);
    assert!(summary.truncated);
    assert!(!summary.resources.is_empty());
    assert!(summary.resources.iter().all(|resource| resource.overallocated));
    assert!(summary.tasks.len() > 10, "{} tasks kept", summary.tasks.len());
    assert!(json(&summary).len() <= options.max_chars);
}

#[test]
fn never_goes_over_the_limit_at_any_size() {
    let doc = with_rules();
    let whole = json(&summary(&doc.project)).len();
    for max_chars in (0..whole + 200).step_by(97) {
        let cut = project_summary(&doc.project, &SummaryOptions { max_chars, ..SummaryOptions::default() });
        let length = json(&cut).len();
        let empty = cut.tasks.is_empty() && cut.resources.is_empty();
        assert!(length <= max_chars || empty, "{length} chars for a limit of {max_chars}");
        assert_eq!(cut.truncated, cut.omitted_tasks + cut.omitted_resources > 0, "limit {max_chars}");
        for task in &cut.tasks {
            if let Some(parent) = &task.parent {
                assert!(cut.tasks.iter().any(|kept| &kept.id == parent), "{parent} missing at {max_chars}");
            }
        }
    }
}

#[test]
fn a_cut_masked_summary_leaks_no_names_either() {
    let doc = with_rules();
    let masked = SummaryOptions { mask: true, ..SummaryOptions::default() };
    let whole = json(&project_summary(&doc.project, &masked)).len();
    let cut = project_summary(&doc.project, &SummaryOptions { max_chars: whole / 2, ..masked });
    assert!(cut.truncated);
    let text = json(&cut);
    for name in ["Thiết kế", "Lập trình", "Hoàn tiền", "\"An\"", "Bình", "Mảng", "Ngân hàng"] {
        assert!(!text.contains(name), "{name} leaks into {text}");
    }
}

#[test]
fn finished_tasks_are_not_counted_as_late() {
    let mut doc = chain();
    apply(&mut doc, Command::SetDeadline { uid: 1, date: Some(at(0, 12, 0)) });
    assert_eq!(summary(&doc.project).project.deadlines_missed, 1);
    apply(&mut doc, Command::SetPercentComplete { uid: 1, percent: 100.0 });
    let summary = summary(&doc.project);
    assert_eq!(summary.project.deadlines_missed, 0);
    assert_eq!(summary.project.late, 0);
    assert_eq!(summary.project.done, 1);
}

#[test]
fn masking_numbers_values_outside_a_list_and_keeps_numbers() {
    let mut doc = with_rules();
    apply(&mut doc, Command::AddField { name: "Điểm".into(), kind: FieldKind::Number });
    apply(&mut doc, Command::SetAssignmentRules {
        rules: vec![
            AssignmentRule {
                conditions: vec![Condition { field: 3, values: vec![FieldValue::Number(5.0), FieldValue::Number(8.0)] }],
                target: RuleTarget::Resource { uid: 1 },
                units: 1.0,
            },
            AssignmentRule {
                conditions: vec![Condition { field: 2, values: vec![FieldValue::Text("Ngân hàng Á Châu".into()), FieldValue::Text("Bưu điện".into())] }],
                target: RuleTarget::Resource { uid: 2 },
                units: 1.0,
            },
            AssignmentRule {
                conditions: vec![Condition { field: 2, values: vec![FieldValue::Text("Bưu điện".into())] }],
                target: RuleTarget::Resource { uid: 1 },
                units: 1.0,
            },
        ],
    });
    let masked = project_summary(&doc.project, &SummaryOptions { mask: true, ..SummaryOptions::default() });
    assert_eq!(masked.rules[0].when, vec!["Field 3 = 5 | 8"]);
    assert_eq!(masked.rules[1].when, vec!["Field 2 = Value 1 | Value 2"]);
    // The same text is the same masked value in every rule.
    assert_eq!(masked.rules[2].when, vec!["Field 2 = Value 2"]);
}
