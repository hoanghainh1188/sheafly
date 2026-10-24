//! Skills and assignment rules (phase 18 P2, step 4a): resources hold skills, the project
//! keeps an ordered list of rules from custom field values to a person or a skill, and
//! `AssignTasks` gives many tasks their person in one undo step.

use chrono::Datelike;

use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue, OptionRename};
use sheafly_engine::lang::Lang;
use sheafly_engine::model::{ResourceKind, Task, TaskType};
use sheafly_engine::mspdi::{read_mspdi_with_report, write_mspdi};
use sheafly_engine::skills::{
    rule_for, AssignmentRule, Condition, RuleTarget, SkillDef, TaskAssignment, MAX_CONDITIONS,
    MAX_RULES, MAX_SKILLS,
};
use sheafly_engine::store::{load, save};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const DAY: i64 = 480;
const FIXTURE: &str = include_str!("fixtures/skills.xml");

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(i64::from(day)))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

/// Tasks 1..=`tasks` of one day each and people 1..=`people`.
fn doc(tasks: u32, people: u32) -> Document {
    let mut project = Project::new("Kỹ năng", at(0, 8, 0));
    for uid in 1..=tasks {
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.duration = DAY;
        task.work = DAY;
        task.remaining_duration = DAY;
        task.remaining_work = DAY;
        project.tasks.push(task);
    }
    project.next_task_uid = tasks + 1;
    let mut doc = Document::from_project(project);
    for person in 1..=people {
        let name = format!("Người {person}");
        doc.apply(Command::AddResource {
            name,
            kind: ResourceKind::Person,
        })
        .unwrap();
    }
    doc
}

fn invalid(result: Result<(), EngineError>) -> Invalid {
    match result {
        Err(EngineError::Invalid(reason)) => reason,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

fn apply(doc: &mut Document, command: Command) -> Result<(), EngineError> {
    doc.apply(command).map(|_| ())
}

fn add_skill(doc: &mut Document, name: &str) -> u32 {
    apply(doc, Command::AddSkill { name: name.into() }).unwrap();
    doc.project.skills.last().unwrap().id
}

fn set_skills(doc: &mut Document, uid: u32, skills: Vec<u32>) -> Result<(), EngineError> {
    apply(doc, Command::SetResourceSkills { uid, skills, levels: Vec::new() })
}

fn add_field(doc: &mut Document, name: &str, kind: FieldKind) -> u32 {
    apply(
        doc,
        Command::AddField {
            name: name.into(),
            kind,
        },
    )
    .unwrap();
    doc.project.fields.last().unwrap().id
}

fn choice(options: &[&str]) -> FieldKind {
    FieldKind::Choice {
        options: options.iter().map(|option| option.to_string()).collect(),
    }
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(value.into())
}

fn set_value(doc: &mut Document, uid: u32, field_id: u32, value: FieldValue) {
    apply(
        doc,
        Command::SetFieldValue {
            uid,
            field_id,
            value: Some(value),
        },
    )
    .unwrap();
}

fn condition(field: u32, values: Vec<FieldValue>) -> Condition {
    Condition { field, values }
}

fn to_person(conditions: Vec<Condition>, uid: u32) -> AssignmentRule {
    AssignmentRule {
        conditions,
        target: RuleTarget::Resource { uid },
        units: 1.0,
    }
}

fn to_skill(conditions: Vec<Condition>, id: u32) -> AssignmentRule {
    AssignmentRule {
        conditions,
        target: RuleTarget::Skill { id, min_level: None },
        units: 1.0,
    }
}

fn set_rules(doc: &mut Document, rules: Vec<AssignmentRule>) -> Result<(), EngineError> {
    apply(doc, Command::SetAssignmentRules { rules })
}

fn matched(doc: &Document, uid: u32) -> Option<usize> {
    rule_for(&doc.project, doc.project.task(uid).unwrap())
}

#[test]
fn skills_are_added_renamed_and_kept_unique() {
    let mut doc = doc(0, 0);
    let backend = add_skill(&mut doc, "  Backend ");
    let design = add_skill(&mut doc, "Thiết kế");
    assert_eq!(
        doc.project.skills,
        vec![
            SkillDef {
                id: backend,
                name: "Backend".into()
            },
            SkillDef {
                id: design,
                name: "Thiết kế".into()
            },
        ]
    );
    assert_ne!(backend, design);
    let taken = apply(
        &mut doc,
        Command::AddSkill {
            name: "BACKEND".into(),
        },
    );
    assert_eq!(invalid(taken), Invalid::SkillNameTaken);
    let empty = apply(&mut doc, Command::AddSkill { name: "  ".into() });
    assert_eq!(invalid(empty), Invalid::SkillName);
    let long = apply(
        &mut doc,
        Command::AddSkill {
            name: "x".repeat(51),
        },
    );
    assert_eq!(invalid(long), Invalid::SkillName);
    // A comma would split the name in MSPDI's comma-separated list.
    let comma = apply(
        &mut doc,
        Command::AddSkill {
            name: "C, C++".into(),
        },
    );
    assert_eq!(invalid(comma), Invalid::SkillName);

    apply(
        &mut doc,
        Command::RenameSkill {
            id: backend,
            name: "backend".into(),
        },
    )
    .unwrap();
    assert_eq!(doc.project.skills[0].name, "backend");
    let clash = apply(
        &mut doc,
        Command::RenameSkill {
            id: backend,
            name: "thiết KẾ".into(),
        },
    );
    assert_eq!(invalid(clash), Invalid::SkillNameTaken);
    let missing = apply(
        &mut doc,
        Command::RenameSkill {
            id: 99,
            name: "Khác".into(),
        },
    );
    assert_eq!(invalid(missing), Invalid::NoSuchSkill);

    doc.undo().unwrap();
    assert_eq!(doc.project.skills[0].name, "Backend");
}

#[test]
fn skill_ids_are_never_reused_and_there_is_a_limit() {
    let mut doc = doc(0, 0);
    let first = add_skill(&mut doc, "A");
    apply(&mut doc, Command::DeleteSkill { id: first }).unwrap();
    let second = add_skill(&mut doc, "A");
    assert!(second > first);

    let mut project = doc.project.clone();
    for index in 1..MAX_SKILLS {
        project.skills.push(SkillDef {
            id: 100 + index as u32,
            name: format!("K{index}"),
        });
    }
    let mut full = Document::from_project(project);
    let over = apply(
        &mut full,
        Command::AddSkill {
            name: "Thêm".into(),
        },
    );
    assert_eq!(invalid(over), Invalid::TooManySkills);
}

#[test]
fn resource_skills_are_sorted_checked_and_follow_a_deleted_skill() {
    let mut doc = doc(0, 2);
    let a = add_skill(&mut doc, "A");
    let b = add_skill(&mut doc, "B");
    set_skills(&mut doc, 1, vec![b, a, b]).unwrap();
    assert_eq!(doc.project.resources[0].skills, vec![a, b]);
    assert_eq!(
        invalid(set_skills(&mut doc, 1, vec![a, 42])),
        Invalid::NoSuchSkill
    );
    assert!(matches!(
        set_skills(&mut doc, 9, vec![a]),
        Err(EngineError::ResourceNotFound)
    ));

    set_skills(&mut doc, 2, vec![b]).unwrap();
    set_rules(&mut doc, vec![to_skill(vec![], b), to_skill(vec![], a)]).unwrap();
    apply(&mut doc, Command::DeleteSkill { id: b }).unwrap();
    assert_eq!(doc.project.resources[0].skills, vec![a]);
    assert!(doc.project.resources[1].skills.is_empty());
    assert_eq!(doc.project.assignment_rules, vec![to_skill(vec![], a)]);
    let missing = apply(&mut doc, Command::DeleteSkill { id: b });
    assert_eq!(invalid(missing), Invalid::NoSuchSkill);

    doc.undo().unwrap();
    assert_eq!(doc.project.resources[0].skills, vec![a, b]);
    assert_eq!(doc.project.resources[1].skills, vec![b]);
    assert_eq!(doc.project.assignment_rules.len(), 2);
}

#[test]
fn rules_are_refused_for_each_reason() {
    let mut doc = doc(1, 1);
    let skill = add_skill(&mut doc, "A");
    let area = add_field(&mut doc, "Mảng", choice(&["Web", "App"]));
    let size = add_field(&mut doc, "Cỡ", FieldKind::Number);
    let reason = |doc: &mut Document, rules: Vec<AssignmentRule>| invalid(set_rules(doc, rules));

    assert_eq!(
        reason(
            &mut doc,
            vec![to_person(vec![condition(77, vec![text("x")])], 1)]
        ),
        Invalid::NoSuchField
    );
    assert_eq!(
        reason(
            &mut doc,
            vec![to_person(vec![condition(area, vec![text("Khác")])], 1)]
        ),
        Invalid::FieldValue
    );
    assert_eq!(
        reason(
            &mut doc,
            vec![to_person(vec![condition(size, vec![text("x")])], 1)]
        ),
        Invalid::FieldValue
    );
    assert_eq!(
        reason(
            &mut doc,
            vec![to_person(vec![condition(area, vec![text(" ")])], 1)]
        ),
        Invalid::FieldValue
    );
    assert_eq!(
        reason(&mut doc, vec![to_person(vec![condition(area, vec![])], 1)]),
        Invalid::EmptyCondition
    );
    let twice = vec![
        condition(area, vec![text("Web")]),
        condition(area, vec![text("App")]),
    ];
    assert_eq!(
        reason(&mut doc, vec![to_person(twice, 1)]),
        Invalid::DuplicateCondition
    );
    assert_eq!(
        reason(&mut doc, vec![to_skill(vec![], 9)]),
        Invalid::NoSuchSkill
    );
    for units in [0.0, -1.0, f64::NAN, f64::INFINITY, 1000.5] {
        let rule = AssignmentRule {
            conditions: vec![],
            target: RuleTarget::Skill { id: skill, min_level: None },
            units,
        };
        assert_eq!(reason(&mut doc, vec![rule]), Invalid::RuleUnits, "{units}");
    }
    let many = vec![to_skill(vec![], skill); MAX_RULES + 1];
    assert_eq!(reason(&mut doc, many), Invalid::TooManyRules);
    let mut conditions = Vec::new();
    for index in 0..=MAX_CONDITIONS {
        let id = add_field(&mut doc, &format!("Chữ {index}"), FieldKind::Text);
        conditions.push(condition(id, vec![text("x")]));
    }
    assert_eq!(
        reason(&mut doc, vec![to_skill(conditions, skill)]),
        Invalid::TooManyConditions
    );
    let nobody = set_rules(&mut doc, vec![to_person(vec![], 5)]);
    assert!(
        matches!(nobody, Err(EngineError::ResourceNotFound)),
        "{nobody:?}"
    );
    assert!(doc.project.assignment_rules.is_empty());

    let max = AssignmentRule {
        conditions: vec![],
        target: RuleTarget::Skill { id: skill, min_level: None },
        units: 1000.0,
    };
    set_rules(&mut doc, vec![max]).unwrap();
}

#[test]
fn rule_values_are_trimmed_and_their_repeats_dropped() {
    let mut doc = doc(1, 1);
    let tag = add_field(&mut doc, "Nhãn", FieldKind::Text);
    let values = vec![text(" a "), text("a"), text("b")];
    set_rules(&mut doc, vec![to_person(vec![condition(tag, values)], 1)]).unwrap();
    assert_eq!(
        doc.project.assignment_rules[0].conditions[0].values,
        vec![text("a"), text("b")]
    );
}

#[test]
fn the_first_rule_that_matches_is_used() {
    let mut doc = doc(5, 2);
    let area = add_field(&mut doc, "Mảng", choice(&["Web", "App", "Dữ liệu"]));
    let size = add_field(&mut doc, "Cỡ", FieldKind::Number);
    set_value(&mut doc, 1, area, text("Web"));
    set_value(&mut doc, 1, size, FieldValue::Number(3.0));
    set_value(&mut doc, 2, area, text("Web"));
    set_value(&mut doc, 2, size, FieldValue::Number(8.0));
    set_value(&mut doc, 3, area, text("App"));
    set_value(&mut doc, 4, area, text("Dữ liệu"));
    // Task 5 has no value at all.
    set_rules(
        &mut doc,
        vec![
            // Web and small: both conditions must hold.
            to_person(
                vec![
                    condition(area, vec![text("Web")]),
                    condition(size, vec![FieldValue::Number(1.0), FieldValue::Number(3.0)]),
                ],
                1,
            ),
            // Web or app: one of the values.
            to_person(vec![condition(area, vec![text("Web"), text("App")])], 2),
            // A size: task 4 has none, so this does not match it.
            to_person(vec![condition(size, vec![FieldValue::Number(5.0)])], 1),
        ],
    )
    .unwrap();
    assert_eq!(matched(&doc, 1), Some(0));
    assert_eq!(matched(&doc, 2), Some(1));
    assert_eq!(matched(&doc, 3), Some(1));
    assert_eq!(matched(&doc, 4), None);
    assert_eq!(matched(&doc, 5), None);

    // A rule with no conditions matches every task: the "everything else" rule.
    let mut rules = doc.project.assignment_rules.clone();
    rules.push(to_person(vec![], 2));
    set_rules(&mut doc, rules).unwrap();
    assert_eq!(matched(&doc, 4), Some(3));
    assert_eq!(matched(&doc, 5), Some(3));
    assert_eq!(matched(&doc, 1), Some(0));
}

#[test]
fn deleting_a_field_drops_its_rules_and_undo_brings_them_back() {
    let mut doc = doc(1, 1);
    let area = add_field(&mut doc, "Mảng", choice(&["Web", "App"]));
    let tag = add_field(&mut doc, "Nhãn", FieldKind::Text);
    let rules = vec![
        to_person(
            vec![
                condition(tag, vec![text("x")]),
                condition(area, vec![text("Web")]),
            ],
            1,
        ),
        to_person(vec![condition(tag, vec![text("y")])], 1),
        to_person(vec![], 1),
    ];
    set_rules(&mut doc, rules.clone()).unwrap();
    apply(&mut doc, Command::DeleteField { id: area }).unwrap();
    // The whole rule goes: keeping it without the condition would match more tasks.
    assert_eq!(doc.project.assignment_rules, rules[1..].to_vec());
    doc.undo().unwrap();
    assert_eq!(doc.project.assignment_rules, rules);
}

#[test]
fn changing_a_field_converts_and_renames_condition_values() {
    let mut doc = doc(1, 1);
    let area = add_field(&mut doc, "Mảng", choice(&["Web", "App", "Cũ"]));
    let size = add_field(&mut doc, "Cỡ", FieldKind::Text);
    let rules = vec![
        to_person(vec![condition(area, vec![text("Web"), text("Cũ")])], 1),
        to_person(vec![condition(area, vec![text("Cũ")])], 1),
        to_person(vec![condition(size, vec![text("2"), text("lớn")])], 1),
        to_person(vec![condition(size, vec![text("lớn")])], 1),
    ];
    set_rules(&mut doc, rules.clone()).unwrap();

    // "Web" is renamed "Website"; "Cũ" is no longer a choice.
    let renamed_options = vec![OptionRename {
        from: "Web".into(),
        to: "Website".into(),
    }];
    let update = Command::UpdateField {
        id: area,
        name: "Mảng".into(),
        kind: choice(&["Website", "App"]),
        renamed_options,
    };
    apply(&mut doc, update).unwrap();
    // Text to number keeps "2" as 2 and drops "lớn"; a rule left with nothing goes.
    let update = Command::UpdateField {
        id: size,
        name: "Cỡ".into(),
        kind: FieldKind::Number,
        renamed_options: vec![],
    };
    apply(&mut doc, update).unwrap();
    assert_eq!(
        doc.project.assignment_rules,
        vec![
            to_person(vec![condition(area, vec![text("Website")])], 1),
            to_person(vec![condition(size, vec![FieldValue::Number(2.0)])], 1),
        ]
    );
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.project.assignment_rules, rules);
}

#[test]
fn deleting_a_resource_drops_the_rules_that_point_at_it() {
    let mut doc = doc(1, 2);
    let skill = add_skill(&mut doc, "A");
    set_skills(&mut doc, 1, vec![skill]).unwrap();
    let rules = vec![
        to_person(vec![], 1),
        to_skill(vec![], skill),
        to_person(vec![], 2),
    ];
    set_rules(&mut doc, rules.clone()).unwrap();
    apply(&mut doc, Command::DeleteResource { uid: 1 }).unwrap();
    assert_eq!(doc.project.assignment_rules, rules[1..].to_vec());
    doc.undo().unwrap();
    assert_eq!(doc.project.assignment_rules, rules);
}

#[test]
fn assign_tasks_is_one_undo_step_and_all_or_nothing() {
    let mut doc = doc(3, 2);
    let row = |task_uid, resource_uid, units| TaskAssignment {
        task_uid,
        resource_uid,
        units,
    };
    let before = doc.project.assignments.len();
    let rows = vec![row(1, 1, 1.0), row(2, 2, 0.5), row(3, 1, 1.0)];
    apply(&mut doc, Command::AssignTasks { assignments: rows }).unwrap();
    let mine: Vec<(u32, u32, f64)> = doc
        .project
        .assignments
        .iter()
        .map(|assignment| {
            (
                assignment.task_uid,
                assignment.resource_uid,
                assignment.units,
            )
        })
        .collect();
    assert_eq!(mine, vec![(1, 1, 1.0), (2, 2, 0.5), (3, 1, 1.0)]);
    // Fixed units, effort driven: half a person takes twice as long.
    assert_eq!(doc.project.task(2).unwrap().duration, 2 * DAY);
    doc.undo().unwrap();
    assert_eq!(doc.project.assignments.len(), before);

    // Assigning someone already on the task changes their units, as AssignResource does.
    apply(
        &mut doc,
        Command::AssignTasks {
            assignments: vec![row(1, 1, 1.0)],
        },
    )
    .unwrap();
    apply(
        &mut doc,
        Command::AssignTasks {
            assignments: vec![row(1, 1, 2.0)],
        },
    )
    .unwrap();
    assert_eq!(doc.project.assignments.len(), 1);
    assert_eq!(doc.project.assignments[0].units, 2.0);
    doc.undo().unwrap();
    doc.undo().unwrap();

    for bad in [
        row(2, 9, 1.0),
        row(9, 1, 1.0),
        row(2, 1, 0.0),
        row(2, 1, f64::NAN),
    ] {
        let result = apply(
            &mut doc,
            Command::AssignTasks {
                assignments: vec![row(1, 1, 1.0), bad.clone()],
            },
        );
        assert!(result.is_err(), "{bad:?}");
        assert!(doc.project.assignments.is_empty(), "{bad:?}");
    }
    let nothing = apply(
        &mut doc,
        Command::AssignTasks {
            assignments: vec![],
        },
    );
    assert_eq!(invalid(nothing), Invalid::NoAssignments);
}

#[test]
fn assign_tasks_keeps_each_task_type_as_assign_resource_does() {
    for task_type in [
        TaskType::FixedUnits,
        TaskType::FixedDuration,
        TaskType::FixedWork,
    ] {
        let mut one = doc(1, 1);
        apply(
            &mut one,
            Command::SetTaskType {
                uid: 1,
                task_type,
                effort_driven: true,
            },
        )
        .unwrap();
        let mut many = one.clone();
        apply(
            &mut one,
            Command::AssignResource {
                task_uid: 1,
                resource_uid: 1,
                units: 0.5,
            },
        )
        .unwrap();
        let rows = vec![TaskAssignment {
            task_uid: 1,
            resource_uid: 1,
            units: 0.5,
        }];
        apply(&mut many, Command::AssignTasks { assignments: rows }).unwrap();
        assert_eq!(one.project.tasks, many.project.tasks, "{task_type:?}");
        assert_eq!(
            one.project.assignments, many.project.assignments,
            "{task_type:?}"
        );
    }
}

#[test]
fn skills_and_rules_survive_a_file_and_old_projects_have_none() {
    let mut doc = doc(1, 1);
    let skill = add_skill(&mut doc, "Kiểm thử");
    set_skills(&mut doc, 1, vec![skill]).unwrap();
    let area = add_field(&mut doc, "Mảng", choice(&["Web"]));
    let rule = AssignmentRule {
        conditions: vec![condition(area, vec![text("Web")])],
        target: RuleTarget::Skill { id: skill, min_level: None },
        units: 0.25,
    };
    set_rules(&mut doc, vec![rule, to_person(vec![], 1)]).unwrap();
    let row = TaskAssignment {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    };
    apply(
        &mut doc,
        Command::AssignTasks {
            assignments: vec![row],
        },
    )
    .unwrap();

    let dir = std::env::temp_dir().join(format!("sheafly-skills-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("skills.sheafly").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let opened = load(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(opened.project.skills, doc.project.skills);
    assert_eq!(opened.project.next_skill_id, doc.project.next_skill_id);
    assert_eq!(opened.project.resources, doc.project.resources);
    assert_eq!(
        opened.project.assignment_rules,
        doc.project.assignment_rules
    );

    let mut json = serde_json::to_value(&doc.project).unwrap();
    let object = json.as_object_mut().unwrap();
    object.remove("skills");
    object.remove("next_skill_id");
    object.remove("assignment_rules");
    json["resources"][0]
        .as_object_mut()
        .unwrap()
        .remove("skills");
    let old: Project = serde_json::from_value(json).unwrap();
    assert!(old.skills.is_empty() && old.assignment_rules.is_empty());
    assert!(old.resources[0].skills.is_empty());
    // With no `next_skill_id` the next id is past every one in use.
    let mut old = Document::from_project(old);
    old.project.skills.push(SkillDef {
        id: 7,
        name: "Có sẵn".into(),
    });
    let id = add_skill(&mut old, "Mới");
    assert_eq!(id, 8);
}

#[test]
fn the_commands_read_the_shape_the_app_sends() {
    let sent = r#"{"type":"set_assignment_rules","rules":[
        {"conditions":[{"field":1,"values":["Web",2]}],"target":{"type":"skill","id":3},"units":0.5},
        {"conditions":[],"target":{"type":"resource","uid":4}}]}"#;
    let command: Command = serde_json::from_str(sent).unwrap();
    let Command::SetAssignmentRules { rules } = command else {
        panic!("read as {command:?}");
    };
    assert_eq!(
        rules,
        vec![
            AssignmentRule {
                conditions: vec![condition(1, vec![text("Web"), FieldValue::Number(2.0)])],
                target: RuleTarget::Skill { id: 3, min_level: None },
                units: 0.5,
            },
            // Units default to one.
            to_person(vec![], 4),
        ]
    );
    let sent =
        r#"{"type":"assign_tasks","assignments":[{"task_uid":1,"resource_uid":2,"units":1}]}"#;
    let command: Command = serde_json::from_str(sent).unwrap();
    assert!(matches!(command, Command::AssignTasks { ref assignments } if assignments.len() == 1));
    let sent = r#"{"type":"set_resource_skills","uid":1,"skills":[2,3]}"#;
    assert!(serde_json::from_str::<Command>(sent).is_ok());
}

#[test]
fn mspdi_round_trip_keeps_skills_as_a_resource_text_field() {
    let mut doc = doc(1, 3);
    let web = add_skill(&mut doc, "Web");
    let data = add_skill(&mut doc, "Dữ liệu");
    let etl = add_skill(&mut doc, "ETL");
    let ops = add_skill(&mut doc, "Vận hành & <hạ tầng>");
    set_skills(&mut doc, 1, vec![web, ops]).unwrap();
    set_skills(&mut doc, 2, vec![data, etl]).unwrap();
    set_rules(&mut doc, vec![to_skill(vec![], web)]).unwrap();

    let xml = write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<FieldID>205520904</FieldID>"), "{xml}");
    assert!(xml.contains("<Alias>Kỹ năng</Alias>"), "{xml}");
    let (back, _) = read_mspdi_with_report(&xml).unwrap();
    let names = |project: &Project, uid: u32| -> Vec<String> {
        let resource = project
            .resources
            .iter()
            .find(|resource| resource.uid == uid)
            .unwrap();
        let mut names: Vec<String> = resource
            .skills
            .iter()
            .map(|id| {
                project
                    .skills
                    .iter()
                    .find(|skill| skill.id == *id)
                    .unwrap()
                    .name
                    .clone()
            })
            .collect();
        names.sort();
        names
    };
    assert_eq!(names(&back, 1), vec!["Vận hành & <hạ tầng>", "Web"]);
    assert_eq!(names(&back, 2), vec!["Dữ liệu", "ETL"]);
    assert!(names(&back, 3).is_empty());
    // Rules stay in .sheafly files only; skills nobody has are not written either.
    assert!(back.assignment_rules.is_empty());
    // Custom task fields are not confused with the resource field.
    assert!(back.fields.is_empty());
}

#[test]
fn mspdi_without_skills_writes_no_resource_field() {
    let doc = doc(1, 1);
    let xml = write_mspdi(&doc.project).unwrap();
    assert!(!xml.contains("205520904"));
}

#[test]
fn mspdi_reads_a_skills_field_from_another_slot_and_leaves_other_fields() {
    let (project, report) = read_mspdi_with_report(FIXTURE).unwrap();
    let names: Vec<&str> = project
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    // Repeats differ only in letter case; empty pieces are dropped.
    assert_eq!(names, vec!["Backend", "Kiểm thử", "Thiết kế"]);
    let skills_of = |uid: u32| {
        project
            .resources
            .iter()
            .find(|r| r.uid == uid)
            .unwrap()
            .skills
            .clone()
    };
    assert_eq!(
        skills_of(1),
        vec![project.skills[0].id, project.skills[1].id]
    );
    assert_eq!(
        skills_of(2),
        vec![project.skills[0].id, project.skills[2].id]
    );
    // Resource 3's "Phòng" field is another resource text field: not skills.
    assert!(skills_of(3).is_empty());
    assert!(project.fields.is_empty());
    assert!(report
        .message(Lang::En)
        .is_none_or(|message| !message.contains("Skills")));
}
