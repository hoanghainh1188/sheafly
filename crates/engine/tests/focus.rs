//! The productivity factor (capacity plan PR 2; issue #150): people give only part of
//! their time at work to project work, which slows the rate at which their work is done.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::command::Command;
use sheafly_engine::load_map::{load_map, Bucket, LoadGroup};
use sheafly_engine::model::{ResourceKind, Task, TaskType};
use sheafly_engine::mspdi::{read_mspdi, write_mspdi};
use sheafly_engine::skills::{AssignmentRule, RuleTarget};
use sheafly_engine::store::{load, save};
use sheafly_engine::suggest::suggest;
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const DAY: i64 = 480;

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = monday() + chrono::Duration::days(i64::from(day));
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

fn monday() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
}

fn apply(doc: &mut Document, command: Command) -> Result<(), EngineError> {
    doc.apply(command).map(|_| ())
}

fn invalid(result: Result<(), EngineError>) -> Invalid {
    match result {
        Err(EngineError::Invalid(reason)) => reason,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

/// Tasks 1..=`tasks` of `days` working days each, of `task_type`, and people An, Bình.
fn doc(tasks: u32, days: i64, task_type: TaskType) -> Document {
    let mut project = Project::new("Năng suất", at(0, 8, 0));
    for uid in 1..=tasks {
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.duration = days * DAY;
        task.remaining_duration = days * DAY;
        task.work = days * DAY;
        task.remaining_work = days * DAY;
        task.task_type = task_type;
        project.tasks.push(task);
    }
    project.next_task_uid = tasks + 1;
    let mut doc = Document::from_project(project);
    for name in ["An", "Bình"] {
        apply(
            &mut doc,
            Command::AddResource {
                name: name.into(),
                kind: ResourceKind::Person,
            },
        )
        .unwrap();
    }
    doc
}

fn assign(doc: &mut Document, task_uid: u32, resource_uid: u32, units: f64) {
    apply(
        doc,
        Command::AssignResource {
            task_uid,
            resource_uid,
            units,
        },
    )
    .unwrap();
}

fn factor(doc: &mut Document, factor: f64) -> Result<(), EngineError> {
    apply(doc, Command::SetFocusFactor { factor })
}

fn own(doc: &mut Document, uid: u32, focus: Option<f64>) -> Result<(), EngineError> {
    apply(doc, Command::SetResourceFocus { uid, focus })
}

fn task(doc: &Document, uid: u32) -> &Task {
    doc.project.task(uid).unwrap()
}

fn units_of(doc: &Document, task_uid: u32) -> Vec<f64> {
    doc.project
        .assignments
        .iter()
        .filter(|assignment| assignment.task_uid == task_uid)
        .map(|assignment| assignment.units)
        .collect()
}

#[test]
fn fixed_work_takes_longer_at_a_lower_factor_and_units_stay_as_typed() {
    let mut doc = doc(1, 6, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    assert_eq!(task(&doc, 1).duration, 6 * DAY);
    factor(&mut doc, 0.8).unwrap();
    // 48 hours of work at 80 % take 60 working hours.
    assert_eq!(task(&doc, 1).duration, 60 * 60);
    assert_eq!(task(&doc, 1).work, 6 * DAY);
    assert_eq!(units_of(&doc, 1), vec![1.0]);
    assert_eq!(task(&doc, 1).units, 1.0);
    // Five days, two more, then the morning of the third: Wednesday at noon.
    assert_eq!(task(&doc, 1).finish, at(9, 12, 0));

    // Back to 100 %: the dates come back and units have not drifted.
    factor(&mut doc, 1.0).unwrap();
    assert_eq!(task(&doc, 1).duration, 6 * DAY);
    assert_eq!(units_of(&doc, 1), vec![1.0]);
}

#[test]
fn the_factor_set_before_or_after_assigning_gives_the_same_task() {
    let mut before = doc(1, 6, TaskType::FixedWork);
    factor(&mut before, 0.75).unwrap();
    assign(&mut before, 1, 1, 0.5);
    let mut after = doc(1, 6, TaskType::FixedWork);
    assign(&mut after, 1, 1, 0.5);
    factor(&mut after, 0.75).unwrap();
    assert_eq!(task(&before, 1).duration, task(&after, 1).duration);
    assert_eq!(task(&before, 1).duration, 2 * 6 * DAY * 4 / 3);
    assert_eq!(units_of(&before, 1), units_of(&after, 1));
}

#[test]
fn a_resource_of_its_own_and_machines_do_not_follow_the_project() {
    let mut doc = doc(3, 4, TaskType::FixedWork);
    apply(
        &mut doc,
        Command::AddResource {
            name: "Cẩu".into(),
            kind: ResourceKind::Machine,
        },
    )
    .unwrap();
    assign(&mut doc, 1, 1, 1.0);
    assign(&mut doc, 2, 2, 1.0);
    assign(&mut doc, 3, 3, 1.0);
    own(&mut doc, 2, Some(1.0)).unwrap();
    factor(&mut doc, 0.5).unwrap();
    assert_eq!(task(&doc, 1).duration, 8 * DAY);
    assert_eq!(task(&doc, 2).duration, 4 * DAY);
    assert_eq!(task(&doc, 3).duration, 4 * DAY);

    // Clearing Bình's own factor brings the project's back to Bình's tasks.
    own(&mut doc, 2, None).unwrap();
    assert_eq!(task(&doc, 2).duration, 8 * DAY);
    // A machine turned into a person starts to follow the project.
    let crane = doc.project.resources[2].clone();
    apply(
        &mut doc,
        Command::UpdateResource {
            uid: 3,
            name: crane.name,
            kind: ResourceKind::Person,
            max_units: crane.max_units,
            calendar_id: crane.calendar_id,
            standard_rate: crane.standard_rate,
            overtime_rate: crane.overtime_rate,
            cost_per_use: crane.cost_per_use,
        },
    )
    .unwrap();
    assert_eq!(task(&doc, 3).duration, 8 * DAY);
}

#[test]
fn each_task_type_keeps_what_it_fixes() {
    // Fixed Units, not effort driven: the duration stays and less work gets done.
    let mut units_doc = doc(1, 5, TaskType::FixedUnits);
    let doc_units = &mut units_doc;
    apply(
        doc_units,
        Command::SetTaskType {
            uid: 1,
            task_type: TaskType::FixedUnits,
            effort_driven: false,
        },
    )
    .unwrap();
    assign(doc_units, 1, 1, 1.0);
    factor(doc_units, 0.8).unwrap();
    assert_eq!(task(&units_doc, 1).duration, 5 * DAY);
    assert_eq!(task(&units_doc, 1).work, 4 * DAY);

    // Fixed Duration: new work sets the units, read back as typed.
    let mut doc = doc(1, 5, TaskType::FixedDuration);
    assign(&mut doc, 1, 1, 1.0);
    own(&mut doc, 1, Some(0.5)).unwrap();
    apply(
        &mut doc,
        Command::SetWork {
            uid: 1,
            minutes: 2 * DAY,
        },
    )
    .unwrap();
    assert_eq!(task(&doc, 1).duration, 5 * DAY);
    let units = units_of(&doc, 1)[0];
    assert!((units - 0.8).abs() < 1e-9, "{units}");
}

#[test]
fn a_task_with_nobody_works_at_the_project_factor() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    factor(&mut doc, 0.8).unwrap();
    assert_eq!(task(&doc, 1).duration, 5 * DAY);
    assert_eq!(task(&doc, 1).work, 4 * DAY);
    assert_eq!(task(&doc, 1).units, 1.0);
}

#[test]
fn started_work_keeps_its_dates_when_the_factor_changes() {
    let mut doc = doc(2, 4, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    assign(&mut doc, 2, 1, 1.0);
    apply(
        &mut doc,
        Command::SetPercentComplete {
            uid: 1,
            percent: 25.0,
        },
    )
    .unwrap();
    let started = task(&doc, 1).clone();
    factor(&mut doc, 0.5).unwrap();
    assert_eq!(task(&doc, 1).duration, started.duration);
    assert_eq!(task(&doc, 1).finish, started.finish);
    assert_eq!(task(&doc, 2).duration, 8 * DAY);
}

#[test]
fn usage_and_the_load_map_count_useful_hours() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    factor(&mut doc, 0.8).unwrap();
    let row = doc
        .project
        .usage
        .iter()
        .find(|row| row.assignment_uid.is_some())
        .unwrap();
    assert!((row.days[0].hours - 6.4).abs() < 1e-9, "{:?}", row.days[0]);
    let total: f64 = row.days.iter().map(|day| day.hours).sum();
    assert!((total - 32.0).abs() < 1e-9);

    // A full-time person at 80 % is fully loaded by 6.4 hours a day.
    let map = load_map(&doc.project, monday(), monday(), Bucket::Day, LoadGroup::Person, None).unwrap();
    assert_eq!(map.rows[0].cells[0].work, 384);
    assert_eq!(map.rows[0].cells[0].capacity, 384);
}

#[test]
fn suggestions_date_each_person_at_their_own_rate() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    apply(&mut doc, Command::AddSkill { name: "Web".into() }).unwrap();
    for uid in [1, 2] {
        apply(
            &mut doc,
            Command::SetResourceSkills {
                uid,
                skills: vec![1],
                levels: Vec::new(),
            },
        )
        .unwrap();
    }
    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![AssignmentRule {
                conditions: Vec::new(),
                target: RuleTarget::Skill {
                    id: 1,
                    min_level: None,
                },
                units: 1.0,
            }],
        },
    )
    .unwrap();
    own(&mut doc, 1, Some(0.5)).unwrap();
    let picks = &suggest(&doc.project)[0].picks;
    // Bình at 100 % finishes first; An at 50 % takes twice as long.
    assert_eq!(picks[0].resource_uid, 2);
    assert_eq!(picks[1].resource_uid, 1);
    assert_eq!(picks[0].finish, at(3, 17, 0));
    assert_eq!(picks[1].finish, at(9, 17, 0));
}

#[test]
fn factors_are_checked_and_undone() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    for bad in [0.49, 1.01, f64::NAN, 0.0] {
        assert_eq!(
            invalid(factor(&mut doc, bad)),
            Invalid::FocusFactor,
            "{bad}"
        );
        assert_eq!(
            invalid(own(&mut doc, 1, Some(bad))),
            Invalid::FocusFactor,
            "{bad}"
        );
    }
    assert!(matches!(
        own(&mut doc, 9, Some(0.8)),
        Err(EngineError::ResourceNotFound)
    ));
    factor(&mut doc, 0.8).unwrap();
    assert_eq!(task(&doc, 1).duration, 5 * DAY);
    doc.undo().unwrap();
    assert_eq!(doc.project.focus_factor, 1.0);
    assert_eq!(task(&doc, 1).duration, 4 * DAY);
}

#[test]
fn mspdi_writes_rates_as_units_so_project_gets_the_same_dates() {
    let mut doc = doc(2, 4, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    factor(&mut doc, 0.8).unwrap();
    let xml = write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<Units>0.8</Units>"), "{xml}");
    let back = read_mspdi(&xml).unwrap();
    assert_eq!(back.focus_factor, 1.0);
    for uid in [1, 2] {
        let original = task(&doc, uid);
        let read = back.task(uid).unwrap();
        assert_eq!(
            (read.start, read.finish),
            (original.start, original.finish),
            "task {uid}"
        );
    }
}

#[test]
fn factors_survive_a_file_and_old_projects_work_full_time() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    factor(&mut doc, 0.75).unwrap();
    own(&mut doc, 2, Some(0.9)).unwrap();
    let dir = std::env::temp_dir().join(format!("sheafly-focus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("focus.sheafly").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let opened = load(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(opened.project.focus_factor, 0.75);
    assert_eq!(opened.project.resources[1].focus, Some(0.9));
    assert_eq!(opened.project.tasks, doc.project.tasks);

    let mut json = serde_json::to_value(&doc.project).unwrap();
    json.as_object_mut().unwrap().remove("focus_factor");
    json["resources"][1]
        .as_object_mut()
        .unwrap()
        .remove("focus");
    let old: Project = serde_json::from_value(json).unwrap();
    assert_eq!(old.focus_factor, 1.0);
    assert_eq!(old.resources[1].focus, None);

    let sent = r#"{"type":"set_resource_focus","uid":1}"#;
    assert!(matches!(
        serde_json::from_str::<Command>(sent).unwrap(),
        Command::SetResourceFocus {
            uid: 1,
            focus: None
        }
    ));
    let sent = r#"{"type":"set_focus_factor","factor":0.8}"#;
    assert!(serde_json::from_str::<Command>(sent).is_ok());
}

#[test]
fn usage_of_started_work_matches_its_work_after_the_factor_changes() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    apply(&mut doc, Command::SetPercentComplete { uid: 1, percent: 25.0 }).unwrap();
    factor(&mut doc, 0.8).unwrap();
    let work = doc.project.assignments[0].work;
    assert_eq!(work, 4 * DAY);
    for row in &doc.project.usage {
        let total: f64 = row.days.iter().map(|day| day.hours).sum();
        assert!((total - work as f64 / 60.0).abs() < 1e-6, "{row:?}");
    }
    let days = task(&doc, 1).duration / DAY;
    let map = load_map(
        &doc.project,
        monday(),
        monday() + chrono::Duration::days(days + 2),
        Bucket::Day,
        LoadGroup::Person,
        None,
    )
    .unwrap();
    let assigned: i64 = map.rows[0].cells.iter().map(|cell| cell.work).sum();
    assert_eq!(assigned, work);
}

#[test]
fn removing_the_last_person_leaves_the_task_as_if_nobody_had_been_on_it() {
    let mut doc = doc(1, 4, TaskType::FixedWork);
    apply(
        &mut doc,
        Command::AddResource {
            name: "Máy in".into(),
            kind: ResourceKind::Machine,
        },
    )
    .unwrap();
    factor(&mut doc, 0.8).unwrap();
    let never_assigned = task(&doc, 1).clone();
    assert_eq!(never_assigned.duration, 5 * DAY);
    assign(&mut doc, 1, 3, 1.0);
    assert_eq!(task(&doc, 1).duration, 4 * DAY);
    let assignment = doc.project.assignments[0].uid;
    apply(&mut doc, Command::RemoveAssignment { uid: assignment }).unwrap();
    assert_eq!(task(&doc, 1).duration, never_assigned.duration);
    assert_eq!(task(&doc, 1).work, never_assigned.work);
}

#[test]
fn a_paste_into_a_project_with_another_factor_takes_its_rate() {
    use sheafly_engine::clipboard::copy_tasks;
    let mut doc = doc(1, 4, TaskType::FixedWork);
    assign(&mut doc, 1, 1, 1.0);
    let clipboard = copy_tasks(&doc.project, &[1]);
    factor(&mut doc, 0.8).unwrap();
    assert_eq!(task(&doc, 1).duration, 5 * DAY);
    apply(&mut doc, Command::PasteTasks { after_uid: Some(1), clipboard }).unwrap();
    let pasted = doc.project.tasks.last().unwrap();
    assert_eq!(pasted.work, 4 * DAY);
    assert_eq!(pasted.duration, 5 * DAY);
}
