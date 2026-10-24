use sheafly_engine::assignment_matrix::assignment_matrix;
use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue};
use sheafly_engine::model::ResourceKind;
use sheafly_engine::skills::{AssignmentRule, Condition, RuleTarget};
use sheafly_engine::Document;

#[test]
fn empty_project_gives_empty_matrix() {
    let doc = Document::new_default();
    let matrix = assignment_matrix(&doc.project, false);
    assert_eq!(matrix.columns.len(), 0);
    assert_eq!(matrix.roles.len(), 0);
    assert_eq!(matrix.rows.len(), 1); // Document::new_default() has initial Task 1
    assert_eq!(matrix.total_work, 0);
}

#[test]
fn person_mode_and_summary_rollup() {
    let mut doc = Document::new_default();

    // 1. Add resources with roles
    doc.apply(Command::AddResource {
        name: "An".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let an_uid = doc.project.resources.last().unwrap().uid;
    doc.apply(Command::SetResourceGroup {
        uid: an_uid,
        group: "Backend".into(),
    }).unwrap();

    doc.apply(Command::AddResource {
        name: "Bình".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let binh_uid = doc.project.resources.last().unwrap().uid;
    doc.apply(Command::SetResourceGroup {
        uid: binh_uid,
        group: "Backend".into(),
    }).unwrap();

    doc.apply(Command::AddResource {
        name: "Cường".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let cuong_uid = doc.project.resources.last().unwrap().uid;
    doc.apply(Command::SetResourceGroup {
        uid: cuong_uid,
        group: "Frontend".into(),
    }).unwrap();

    // 2. Add tasks:
    // Task 1: Summary (Level 0)
    // Task 2: Sub-task 1 (Level 1) -> 8h (480m) to An (1.0)
    // Task 3: Sub-task 2 (Level 1) -> 8h (480m) to Bình (0.5)
    // Task 4: Task 2 (Level 0) -> 16h (960m) to Cường (1.0)
    doc.apply(Command::AddTask {
        name: "Phase 1".into(),
        after_uid: None,
    }).unwrap();
    let sum1_uid = doc.project.tasks.last().unwrap().uid;

    doc.apply(Command::AddTask {
        name: "Subtask 1".into(),
        after_uid: Some(sum1_uid),
    }).unwrap();
    let sub1_uid = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::Indent { uid: sub1_uid }).unwrap();

    doc.apply(Command::AddTask {
        name: "Subtask 2".into(),
        after_uid: Some(sub1_uid),
    }).unwrap();
    let sub2_uid = doc.project.tasks.last().unwrap().uid;

    doc.apply(Command::AddTask {
        name: "Phase 2".into(),
        after_uid: Some(sub2_uid),
    }).unwrap();
    let p2_uid = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::Outdent { uid: p2_uid }).unwrap();

    // Assign
    doc.apply(Command::AssignResource {
        task_uid: sub1_uid,
        resource_uid: an_uid,
        units: 1.0,
    }).unwrap();
    doc.apply(Command::AssignResource {
        task_uid: sub2_uid,
        resource_uid: binh_uid,
        units: 0.5,
    }).unwrap();
    doc.apply(Command::AssignResource {
        task_uid: p2_uid,
        resource_uid: cuong_uid,
        units: 1.0,
    }).unwrap();

    // Test Person Mode
    let matrix = assignment_matrix(&doc.project, false);
    assert_eq!(matrix.roles.len(), 2);
    assert_eq!(matrix.roles[0].name, "Backend");
    assert_eq!(matrix.roles[0].resource_uids, vec![an_uid, binh_uid]);
    assert_eq!(matrix.roles[1].name, "Frontend");
    assert_eq!(matrix.roles[1].resource_uids, vec![cuong_uid]);

    assert_eq!(matrix.columns.len(), 3);
    assert_eq!(matrix.columns[0].label, "An");
    assert_eq!(matrix.columns[1].label, "Bình");
    assert_eq!(matrix.columns[2].label, "Cường");

    // Summary row for Phase 1
    let sum_row = matrix.rows.iter().find(|r| r.task_uid == sum1_uid).unwrap();
    assert!(sum_row.summary);
    // An has work in subtask 1, Bình has work in subtask 2, Cường has 0
    assert!(sum_row.cells[0].is_some());
    assert_eq!(sum_row.cells[0].as_ref().unwrap().work, 480);
    assert!(sum_row.cells[1].is_some());
    assert_eq!(sum_row.cells[1].as_ref().unwrap().work, 480);
    assert!(sum_row.cells[2].is_none());
    assert_eq!(sum_row.total_work, 960);

    // Check grand total
    assert_eq!(matrix.total_work, 480 + 480 + 480); // 3 tasks x 480m default duration
    assert_eq!(matrix.columns[0].total_work, 480);
    assert_eq!(matrix.columns[1].total_work, 480);
    assert_eq!(matrix.columns[2].total_work, 480);

    // Test Role Mode (Group = true)
    let role_matrix = assignment_matrix(&doc.project, true);
    assert_eq!(role_matrix.columns.len(), 2);
    assert_eq!(role_matrix.columns[0].label, "Backend");
    assert_eq!(role_matrix.columns[1].label, "Frontend");

    let role_sum_row = role_matrix.rows.iter().find(|r| r.task_uid == sum1_uid).unwrap();
    assert!(role_sum_row.cells[0].is_some());
    // Backend has An (1.0, 480) + Bình (0.5, 480) -> 1.5 units, 960 work
    assert_eq!(role_sum_row.cells[0].as_ref().unwrap().units, 1.5);
    assert_eq!(role_sum_row.cells[0].as_ref().unwrap().work, 960);
    assert!(role_sum_row.cells[1].is_none());
    assert_eq!(role_sum_row.total_work, 960);
    assert_eq!(role_matrix.total_work, matrix.total_work);
}

#[test]
fn unassigned_column_matches_rule() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "Lê".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let le_uid = doc.project.resources.last().unwrap().uid;

    // Add custom choice field
    doc.apply(Command::AddField {
        name: "Mảng".into(),
        kind: FieldKind::Choice {
            options: vec!["BE".into(), "FE".into()],
        },
    }).unwrap();
    let field_id = doc.project.fields.last().unwrap().id;

    // Add assignment rule: Mảng == BE -> Lê
    doc.apply(Command::SetAssignmentRules {
        rules: vec![AssignmentRule {
            conditions: vec![Condition {
                field: field_id,
                values: vec![FieldValue::Text("BE".into())],
            }],
            target: RuleTarget::Resource { uid: le_uid },
            units: 1.0,
        }],
    }).unwrap();

    // Add unassigned task with Mảng = BE
    doc.apply(Command::AddTask {
        name: "Backend Task".into(),
        after_uid: None,
    }).unwrap();
    let task_uid = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::SetFieldValue {
        uid: task_uid,
        field_id,
        value: Some(FieldValue::Text("BE".into())),
    }).unwrap();

    // Add another task without rule match
    doc.apply(Command::AddTask {
        name: "Other Task".into(),
        after_uid: Some(task_uid),
    }).unwrap();
    let other_uid = doc.project.tasks.last().unwrap().uid;

    let matrix = assignment_matrix(&doc.project, false);
    let be_row = matrix.rows.iter().find(|r| r.task_uid == task_uid).unwrap();
    assert!(be_row.unassigned.is_some());
    assert_eq!(be_row.unassigned.as_ref().unwrap().rule_index, 0);
    assert_eq!(
        be_row.unassigned.as_ref().unwrap().target,
        RuleTarget::Resource { uid: le_uid }
    );

    let other_row = matrix.rows.iter().find(|r| r.task_uid == other_uid).unwrap();
    assert!(other_row.unassigned.is_none());
}

#[test]
fn resources_without_role_are_placed_last() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "No Role Person".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let no_role_uid = doc.project.resources.last().unwrap().uid;

    doc.apply(Command::AddResource {
        name: "QA Person".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let qa_uid = doc.project.resources.last().unwrap().uid;
    doc.apply(Command::SetResourceGroup {
        uid: qa_uid,
        group: "QA".into(),
    }).unwrap();

    let matrix = assignment_matrix(&doc.project, false);
    assert_eq!(matrix.roles.len(), 2);
    assert_eq!(matrix.roles[0].name, "QA");
    assert_eq!(matrix.roles[0].resource_uids, vec![qa_uid]);
    assert_eq!(matrix.roles[1].name, "");
    assert_eq!(matrix.roles[1].resource_uids, vec![no_role_uid]);

    assert_eq!(matrix.columns.len(), 2);
    assert_eq!(matrix.columns[0].resource_uid, Some(qa_uid));
    assert_eq!(matrix.columns[1].resource_uid, Some(no_role_uid));
}

#[test]
fn multi_level_summary_rollups_are_correct() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "Worker".into(),
        kind: ResourceKind::Person,
    }).unwrap();
    let w_uid = doc.project.resources.last().unwrap().uid;

    // Grandparent (L0)
    //   Parent (L1)
    //     Child (L2) -> 480m to Worker
    doc.apply(Command::AddTask { name: "Grandparent".into(), after_uid: None }).unwrap();
    let gp_uid = doc.project.tasks.last().unwrap().uid;

    doc.apply(Command::AddTask { name: "Parent".into(), after_uid: Some(gp_uid) }).unwrap();
    let p_uid = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::Indent { uid: p_uid }).unwrap();

    doc.apply(Command::AddTask { name: "Child".into(), after_uid: Some(p_uid) }).unwrap();
    let ch_uid = doc.project.tasks.last().unwrap().uid;
    doc.apply(Command::Indent { uid: ch_uid }).unwrap();

    doc.apply(Command::AssignResource {
        task_uid: ch_uid,
        resource_uid: w_uid,
        units: 1.0,
    }).unwrap();

    let matrix = assignment_matrix(&doc.project, false);
    let gp_row = matrix.rows.iter().find(|r| r.task_uid == gp_uid).unwrap();
    let p_row = matrix.rows.iter().find(|r| r.task_uid == p_uid).unwrap();
    let ch_row = matrix.rows.iter().find(|r| r.task_uid == ch_uid).unwrap();

    assert!(gp_row.summary);
    assert!(p_row.summary);
    assert!(!ch_row.summary);

    assert_eq!(ch_row.cells[0].as_ref().unwrap().work, 480);
    assert_eq!(p_row.cells[0].as_ref().unwrap().work, 480);
    assert_eq!(gp_row.cells[0].as_ref().unwrap().work, 480);

    assert_eq!(matrix.total_work, 480);
    assert_eq!(matrix.columns[0].total_work, 480);
}
