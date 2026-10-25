//! Pulling from an external task tracker (phase 19, PR 1): `SetResourceEmail`, `SetSource`
//! and `ApplyPull`, with `pull::diff` for the preview. The rows are written by hand; the
//! engine knows no tracker.

use chrono::Datelike;

use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue};
use sheafly_engine::lang::Lang;
use sheafly_engine::model::{ConstraintType, ResourceKind, Task, TaskType};
use sheafly_engine::pull::{diff, PullColumn, PullField, PullKind, PullTask};
use sheafly_engine::source::{Source, StatusClass, StatusMapping, SyncSettings, TrackerKind};
use sheafly_engine::store::{load, save};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const HOUR: i64 = 60;
const PULLED: i64 = 1_000;

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(i64::from(day)))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

/// A project with An (an@example.com) and Bình (binh@example.com) and a text field "Loại".
fn doc() -> Document {
    let mut doc = Document::from_project(Project::new("Kéo", at(0, 8, 0)));
    for (name, email) in [("An", "an@example.com"), ("Bình", "binh@example.com")] {
        doc.apply(Command::AddResource {
            name: name.into(),
            kind: ResourceKind::Person,
        })
        .unwrap();
        let uid = doc.project.resources.last().unwrap().uid;
        doc.apply(Command::SetResourceEmail {
            uid,
            email: email.into(),
        })
        .unwrap();
    }
    doc.apply(Command::AddField {
        name: "Loại".into(),
        kind: FieldKind::Text,
    })
    .unwrap();
    doc
}

fn issue(key: &str, name: &str) -> PullTask {
    PullTask {
        key: key.into(),
        name: name.into(),
        ..PullTask::default()
    }
}

fn pull(doc: &mut Document, rows: Vec<PullTask>) -> Result<(), EngineError> {
    doc.apply(Command::ApplyPull {
        rows,
        pulled_at: PULLED,
        resolutions: None,
    })
    .map(|_| ())
}

fn by_key<'a>(project: &'a Project, key: &str) -> &'a Task {
    project
        .tasks
        .iter()
        .find(|task| task.external.as_ref().is_some_and(|link| link.key == key))
        .unwrap()
}

fn resource_of(project: &Project, name: &str) -> u32 {
    project.resources.iter().find(|r| r.name == name).unwrap().uid
}

fn assignees(project: &Project, task: &Task) -> Vec<u32> {
    project
        .assignments
        .iter()
        .filter(|a| a.task_uid == task.uid)
        .map(|a| a.resource_uid)
        .collect()
}

/// Three issues: a parent with a child, and a loose one that waits for the child.
fn sample() -> Vec<PullTask> {
    vec![
        PullTask {
            work: Some(40 * HOUR),
            assignee_email: Some("an@example.com".into()),
            ..issue("P-1", "Cha")
        },
        PullTask {
            parent_key: Some("P-1".into()),
            work: Some(8 * HOUR),
            start: Some(at(2, 8, 0)),
            due: Some(at(9, 17, 0)),
            status: Some(StatusClass::InProgress),
            assignee_email: Some("AN@example.com".into()),
            values: vec![("loại".into(), FieldValue::Text("Lỗi".into()))],
            ..issue("P-2", "Con")
        },
        PullTask {
            work: Some(4 * HOUR),
            status: Some(StatusClass::NotStarted),
            after: vec!["P-2".into()],
            ..issue("P-3", "Sau")
        },
    ]
}

#[test]
fn email_is_checked_unique_and_clearable() {
    let mut doc = doc();
    let an = resource_of(&doc.project, "An");
    let binh = resource_of(&doc.project, "Bình");
    for bad in ["an", "a@b", "@x.com", "a b@x.com", "a@@x.com"] {
        let err = doc
            .apply(Command::SetResourceEmail { uid: an, email: bad.into() })
            .unwrap_err();
        assert_eq!(err, EngineError::Invalid(Invalid::EmailInvalid), "{bad}");
    }
    let err = doc
        .apply(Command::SetResourceEmail { uid: binh, email: " AN@Example.com ".into() })
        .unwrap_err();
    assert_eq!(err, EngineError::Invalid(Invalid::EmailTaken));
    // The same address on the same person is fine, and so is clearing it.
    doc.apply(Command::SetResourceEmail { uid: an, email: "AN@example.com".into() }).unwrap();
    assert_eq!(doc.project.resources[0].email, "AN@example.com");
    doc.apply(Command::SetResourceEmail { uid: an, email: " ".into() }).unwrap();
    assert!(doc.project.resources[0].email.is_empty());
    let err = doc
        .apply(Command::SetResourceEmail { uid: 999, email: "x@y.com".into() })
        .unwrap_err();
    assert_eq!(err, EngineError::ResourceNotFound);
}

fn source() -> Source {
    Source {
        kind: TrackerKind::Backlog,
        site: " Team.Backlog.com ".into(),
        project_key: " PROJ ".into(),
        filter: String::new(),
        hours_per_point: 4.0,
        status_map: vec![StatusMapping {
            status: " Đang xử lý ".into(),
            class: StatusClass::InProgress,
        }],
        sync: SyncSettings::default(),
    }
}

#[test]
fn the_source_is_tidied_and_refused_when_wrong() {
    let mut doc = doc();
    doc.apply(Command::SetSource { source: Some(source()) }).unwrap();
    let kept = doc.project.source.clone().unwrap();
    assert_eq!(kept.site, "team.backlog.com");
    assert_eq!(kept.project_key, "PROJ");
    assert_eq!(kept.status_map[0].status, "Đang xử lý");

    let cases: Vec<(Source, Invalid)> = vec![
        (Source { site: "https://x.backlog.com".into(), ..source() }, Invalid::SourceSite),
        (Source { site: "".into(), ..source() }, Invalid::SourceSite),
        (Source { project_key: "  ".into(), ..source() }, Invalid::SourceProjectKey),
        (Source { filter: "x".repeat(501), ..source() }, Invalid::SourceFilter),
        (Source { hours_per_point: 0.0, ..source() }, Invalid::SourceHoursPerPoint),
        (Source { hours_per_point: f64::NAN, ..source() }, Invalid::SourceHoursPerPoint),
        (
            Source {
                status_map: vec![
                    StatusMapping { status: "Xong".into(), class: StatusClass::Done },
                    StatusMapping { status: "xong".into(), class: StatusClass::Done },
                ],
                ..source()
            },
            Invalid::SourceStatusRepeated,
        ),
        (
            Source {
                status_map: vec![StatusMapping { status: " ".into(), class: StatusClass::Done }],
                ..source()
            },
            Invalid::SourceStatus,
        ),
    ];
    for (bad, reason) in cases {
        let err = doc.apply(Command::SetSource { source: Some(bad) }).unwrap_err();
        assert_eq!(err, EngineError::Invalid(reason));
    }
    // A refused edit leaves the earlier source, and clearing works.
    assert_eq!(doc.project.source.as_ref().unwrap().site, "team.backlog.com");
    doc.apply(Command::SetSource { source: None }).unwrap();
    assert!(doc.project.source.is_none());
}

#[test]
fn the_first_pull_adds_tasks_with_their_data() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let project = &doc.project;
    let an = resource_of(project, "An");

    let parent = by_key(project, "P-1");
    assert!(parent.summary);
    assert_eq!(parent.outline_level, 1);
    // A group takes nothing of its own from the source: its owner and effort roll up.
    assert!(assignees(project, parent).is_empty());
    assert_eq!(parent.external.as_ref().unwrap().synced_at, PULLED);

    let child = by_key(project, "P-2");
    assert_eq!(child.parent_uid, Some(parent.uid));
    assert_eq!(child.outline_level, 2);
    assert_eq!(child.task_type, TaskType::FixedWork);
    assert_eq!(child.work, 8 * HOUR);
    assert_eq!(child.constraint_type, ConstraintType::Snet);
    assert_eq!(child.constraint_date, Some(at(2, 8, 0)));
    assert_eq!(child.deadline, Some(at(9, 17, 0)));
    assert_eq!(assignees(project, child), vec![an]);
    assert_eq!(child.percent_complete, 50.0);
    let field = project.fields[0].id;
    assert_eq!(child.values.get(&field), Some(&FieldValue::Text("Lỗi".into())));
    assert!(child.start >= at(2, 8, 0));

    let after = by_key(project, "P-3");
    assert_eq!(after.percent_complete, 0.0);
    assert!(project
        .dependencies
        .iter()
        .any(|d| d.predecessor_uid == child.uid && d.successor_uid == after.uid));
    assert!(after.start >= child.finish);
}

#[test]
fn pulling_unchanged_data_changes_nothing_and_adds_no_undo_step() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let project = doc.project.clone();
    let steps = doc.commands.len();

    let preview = diff(&doc.project, &sample(), PULLED + 500);
    assert!(preview.errors.is_empty());
    assert!(preview.is_empty());
    assert!(preview.rows.iter().all(|row| row.kind == PullKind::Same));

    let err = doc
        .apply(Command::ApplyPull { rows: sample(), pulled_at: PULLED + 500, resolutions: None })
        .unwrap_err();
    assert_eq!(err, EngineError::Invalid(Invalid::NothingToPull));
    assert_eq!(doc.commands.len(), steps);
    assert_eq!(doc.project, project);
}

#[test]
fn a_changed_issue_updates_only_its_task_and_its_sync_time() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let mut rows = sample();
    rows[1].name = "Con (đổi tên)".into();
    rows[1].work = Some(12 * HOUR);
    rows[1].due = None;
    rows[1].assignee_email = Some("binh@example.com".into());
    rows[1].status = Some(StatusClass::Done);
    rows[1].values = vec![("Loại".into(), FieldValue::Text("Tính năng".into()))];
    rows[2].start = Some(at(12, 8, 0));

    let preview = diff(&doc.project, &rows, PULLED + 7);
    let kinds: Vec<PullKind> = preview.rows.iter().map(|row| row.kind).collect();
    assert_eq!(kinds, vec![PullKind::Same, PullKind::Change, PullKind::Change]);
    assert_eq!(
        preview.rows[1].fields,
        vec![
            PullField::Name,
            PullField::Assignee,
            PullField::Work,
            PullField::Status,
            PullField::Due,
            PullField::Fields,
        ]
    );
    assert_eq!(preview.rows[2].fields, vec![PullField::Start]);

    doc.apply(Command::ApplyPull { rows: rows.clone(), pulled_at: PULLED + 7, resolutions: None }).unwrap();
    let project = &doc.project;
    let child = by_key(project, "P-2");
    assert_eq!(child.name, "Con (đổi tên)");
    assert_eq!(child.work, 12 * HOUR);
    assert_eq!(child.deadline, None);
    assert_eq!(assignees(project, child), vec![resource_of(project, "Bình")]);
    assert_eq!(child.percent_complete, 100.0);
    assert_eq!(child.external.as_ref().unwrap().synced_at, PULLED + 7);
    assert_eq!(by_key(project, "P-1").external.as_ref().unwrap().synced_at, PULLED);
    assert_eq!(by_key(project, "P-3").constraint_date, Some(at(12, 8, 0)));

    // The same rows again are now nothing to pull.
    let err = doc.apply(Command::ApplyPull { rows, pulled_at: PULLED + 9, resolutions: None }).unwrap_err();
    assert_eq!(err, EngineError::Invalid(Invalid::NothingToPull));
}

#[test]
fn a_start_the_source_no_longer_has_is_cleared_but_a_missing_estimate_is_kept() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let mut rows = sample();
    rows[1].start = None;
    rows[1].work = None;
    rows[1].assignee_email = None;
    pull(&mut doc, rows).unwrap();
    let child = by_key(&doc.project, "P-2");
    assert_eq!(child.constraint_type, ConstraintType::Asap);
    assert_eq!(child.constraint_date, None);
    assert_eq!(child.work, 8 * HOUR);
    assert_eq!(assignees(&doc.project, child).len(), 1);
}

#[test]
fn in_progress_keeps_a_percent_set_by_hand() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let uid = by_key(&doc.project, "P-2").uid;
    doc.apply(Command::SetPercentComplete { uid, percent: 30.0 }).unwrap();
    let before = doc.project.clone();
    let err = doc
        .apply(Command::ApplyPull { rows: sample(), pulled_at: PULLED + 1, resolutions: None })
        .unwrap_err();
    assert_eq!(err, EngineError::Invalid(Invalid::NothingToPull));
    assert_eq!(doc.project, before);
}

#[test]
fn units_set_by_hand_stay_while_the_assignee_is_the_same() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let child = by_key(&doc.project, "P-2").clone();
    let assignment = doc.project.assignments.iter().find(|a| a.task_uid == child.uid).unwrap().uid;
    doc.apply(Command::UpdateAssignment { uid: assignment, units: Some(0.5), work: None, overtime_work: None }).unwrap();
    let mut rows = sample();
    rows[2].name = "Sau (đổi)".into();
    pull(&mut doc, rows).unwrap();
    let kept = doc.project.assignments.iter().find(|a| a.task_uid == child.uid).unwrap();
    assert_eq!(kept.units, 0.5);
}

#[test]
fn an_assignee_who_stays_keeps_their_assignment_when_another_person_goes() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let child = by_key(&doc.project, "P-2").clone();
    let an = resource_of(&doc.project, "An");
    let binh = resource_of(&doc.project, "Bình");
    doc.apply(Command::AssignResource { task_uid: child.uid, resource_uid: binh, units: 1.0 }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    let kept = doc
        .project
        .assignments
        .iter()
        .find(|a| a.task_uid == child.uid && a.resource_uid == an)
        .unwrap()
        .clone();
    assert_eq!(kept.baselines.len(), 1);

    let mut rows = sample();
    rows[2].name = "Sau (đổi)".into();
    pull(&mut doc, rows).unwrap();
    let child = by_key(&doc.project, "P-2");
    assert_eq!(assignees(&doc.project, child), vec![an]);
    let after = doc.project.assignments.iter().find(|a| a.task_uid == child.uid).unwrap();
    assert_eq!(after.uid, kept.uid);
    assert_eq!(after.baselines, kept.baselines);
}

#[test]
fn issues_missing_from_the_pull_are_marked_gone_and_come_back() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let rows: Vec<PullTask> = sample().into_iter().take(2).collect();

    let preview = diff(&doc.project, &rows, PULLED + 3);
    assert_eq!(preview.gone.len(), 1);
    assert_eq!(preview.gone[0].key, "P-3");
    assert_eq!(preview.gone[0].name, "Sau");

    pull(&mut doc, rows).unwrap();
    let task = by_key(&doc.project, "P-3");
    assert!(task.external.as_ref().unwrap().gone);
    assert_eq!(task.external.as_ref().unwrap().synced_at, PULLED);
    // Marked once: pulling the same short list again has nothing more to say.
    let rows: Vec<PullTask> = sample().into_iter().take(2).collect();
    assert_eq!(
        doc.apply(Command::ApplyPull { rows, pulled_at: PULLED + 4, resolutions: None }).unwrap_err(),
        EngineError::Invalid(Invalid::NothingToPull)
    );

    let preview = diff(&doc.project, &sample(), PULLED + 5);
    assert_eq!(preview.rows[2].kind, PullKind::Change);
    assert_eq!(preview.rows[2].fields, vec![PullField::Present]);
    pull(&mut doc, sample()).unwrap();
    assert!(!by_key(&doc.project, "P-3").external.as_ref().unwrap().gone);
    // Never deleted along the way.
    assert!(doc.project.tasks.iter().any(|task| task.name == "Sau"));
}

#[test]
fn new_children_go_under_a_parent_that_is_already_in_the_project() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let mut rows = sample();
    rows.push(PullTask {
        parent_key: Some("P-1".into()),
        ..issue("P-4", "Con thêm")
    });
    pull(&mut doc, rows).unwrap();
    let names: Vec<&str> = doc.project.tasks.iter().map(|task| task.name.as_str()).collect();
    let parent = names.iter().position(|name| *name == "Cha").unwrap();
    assert_eq!(names[parent..parent + 3], ["Cha", "Con", "Con thêm"]);
    assert_eq!(names[parent + 3], "Sau");
    assert_eq!(by_key(&doc.project, "P-4").parent_uid, Some(by_key(&doc.project, "P-1").uid));
}

#[test]
fn a_task_is_not_moved_when_the_source_changes_its_parent() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let before: Vec<String> = doc.project.tasks.iter().map(|t| t.name.clone()).collect();
    let mut rows = sample();
    rows[2].parent_key = Some("P-1".into());
    rows[2].name = "Sau (đổi)".into();
    pull(&mut doc, rows).unwrap();
    let after: Vec<String> = doc.project.tasks.iter().map(|t| t.name.clone()).collect();
    assert_eq!(before.len(), after.len());
    assert_eq!(by_key(&doc.project, "P-3").parent_uid, None);
}

fn error_cells(doc: &Document, rows: &[PullTask]) -> Vec<(Option<u32>, PullColumn, Invalid)> {
    diff(&doc.project, rows, PULLED)
        .errors
        .into_iter()
        .map(|error| match error.error {
            EngineError::Invalid(reason) => (error.row, error.column, reason),
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn each_bad_row_is_listed_with_its_cell_and_the_pull_is_refused() {
    let mut doc = doc();
    let rows = vec![
        issue("", "Không khóa"),
        issue("A-1", " "),
        PullTask { work: Some(-1), ..issue("A-2", "Âm") },
        PullTask { assignee_email: Some("lạ@example.com".into()), ..issue("A-3", "Người lạ") },
        issue("A-3", "Trùng"),
        PullTask { parent_key: Some("A-9".into()), ..issue("A-4", "Cha chưa có") },
        PullTask { parent_key: Some("A-5".into()), ..issue("A-5", "Tự làm cha") },
        PullTask { after: vec!["A-6".into()], ..issue("A-6", "Tự chờ") },
        PullTask { after: vec!["Z-1".into()], ..issue("A-7", "Chờ lạ") },
        PullTask {
            values: vec![("Không có".into(), FieldValue::Text("x".into()))],
            ..issue("A-8", "Trường lạ")
        },
        issue("OK-1", "Tốt"),
    ];
    assert_eq!(
        error_cells(&doc, &rows),
        vec![
            (Some(0), PullColumn::Key, Invalid::PullKey),
            (Some(1), PullColumn::Name, Invalid::PullName),
            (Some(2), PullColumn::Work, Invalid::PullNegative),
            (Some(3), PullColumn::Assignee, Invalid::PullUnknownAssignee),
            (Some(4), PullColumn::Key, Invalid::PullKeyRepeated),
            (Some(5), PullColumn::Parent, Invalid::PullParent),
            (Some(6), PullColumn::Parent, Invalid::PullParent),
            (Some(7), PullColumn::After, Invalid::PullLink),
            (Some(8), PullColumn::After, Invalid::PullLink),
            (Some(9), PullColumn::Field("Không có".into()), Invalid::NoSuchField),
        ]
    );
    let before = doc.project.clone();
    let err = pull(&mut doc, rows).unwrap_err();
    assert_eq!(err, EngineError::Invalid(Invalid::PullHasErrors));
    assert_eq!(doc.project, before);
}

#[test]
fn a_parent_that_comes_after_its_child_is_an_error() {
    let doc = doc();
    let rows = vec![
        PullTask { parent_key: Some("B-2".into()), ..issue("B-1", "Con") },
        issue("B-2", "Cha"),
    ];
    assert_eq!(error_cells(&doc, &rows), vec![(Some(0), PullColumn::Parent, Invalid::PullParent)]);
}

#[test]
fn issues_waiting_for_each_other_in_a_loop_are_refused() {
    let mut doc = doc();
    let rows = vec![
        PullTask { after: vec!["L-2".into()], ..issue("L-1", "Một") },
        PullTask { after: vec!["L-1".into()], ..issue("L-2", "Hai") },
    ];
    let preview = diff(&doc.project, &rows, PULLED);
    assert_eq!(preview.errors.len(), 1);
    assert_eq!(preview.errors[0].error, EngineError::DependencyCycle);
    let before = doc.project.clone();
    assert!(pull(&mut doc, rows).is_err());
    assert_eq!(doc.project, before);
}

#[test]
fn too_many_rows_are_refused_as_a_whole() {
    let doc = doc();
    let rows: Vec<PullTask> = (0..20_001).map(|i| issue(&format!("K-{i}"), "x")).collect();
    let preview = diff(&doc.project, &rows, PULLED);
    assert_eq!(preview.errors.len(), 1);
    assert_eq!(preview.errors[0].row, None);
    assert_eq!(preview.errors[0].error, EngineError::Invalid(Invalid::TooManyPullRows));
}

#[test]
fn messages_come_in_both_languages() {
    let doc = doc();
    let preview = diff(&doc.project, &[issue("", "x")], PULLED);
    let error = &preview.errors[0];
    assert!(error.message(Lang::Vi).contains("Khóa"));
    assert!(error.message(Lang::En).contains("key"));
}

#[test]
fn undo_redo_save_and_open_keep_the_links_the_source_and_the_emails() {
    let mut doc = doc();
    doc.apply(Command::SetSource { source: Some(source()) }).unwrap();
    let before = doc.project.clone();
    pull(&mut doc, sample()).unwrap();
    let after = doc.project.clone();
    doc.undo().unwrap();
    assert_eq!(doc.project, before);
    doc.redo().unwrap();
    assert_eq!(doc.project, after);

    let dir = std::env::temp_dir().join(format!("sheafly-pull-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("pull.sheafly").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let opened = load(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(opened.project, after);
    assert_eq!(by_key(&opened.project, "P-2").external.as_ref().unwrap().key, "P-2");
    assert_eq!(opened.project.resources[0].email, "an@example.com");
    assert_eq!(opened.project.source.as_ref().unwrap().site, "team.backlog.com");
}

#[test]
fn deleting_a_pulled_task_drops_its_link_and_a_later_pull_adds_it_again() {
    let mut doc = doc();
    pull(&mut doc, sample()).unwrap();
    let uid = by_key(&doc.project, "P-3").uid;
    doc.apply(Command::DeleteTask { uid }).unwrap();
    let preview = diff(&doc.project, &sample(), PULLED + 1);
    assert_eq!(preview.rows[2].kind, PullKind::Add);
    pull(&mut doc, sample()).unwrap();
    assert_eq!(doc.project.tasks.iter().filter(|t| t.name == "Sau").count(), 1);
}


#[test]
fn emails_cross_mspdi_and_a_repeated_one_stays_with_the_first_person() {
    let doc = doc();
    let xml = sheafly_engine::mspdi::write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<EmailAddress>an@example.com</EmailAddress>"));
    let back = sheafly_engine::mspdi::read_mspdi(&xml).unwrap();
    let emails: Vec<&str> = back.resources.iter().map(|r| r.email.as_str()).collect();
    assert_eq!(emails, ["an@example.com", "binh@example.com"]);

    let repeated = xml.replace("binh@example.com", "AN@example.com");
    let back = sheafly_engine::mspdi::read_mspdi(&repeated).unwrap();
    let emails: Vec<&str> = back.resources.iter().map(|r| r.email.as_str()).collect();
    assert_eq!(emails, ["an@example.com", ""]);
}
