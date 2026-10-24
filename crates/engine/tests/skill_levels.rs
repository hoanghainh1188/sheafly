//! Skill levels and resource groups (capacity planning, PR 1; issue #150): the project's
//! ordered scale of levels, a level per skill a resource has, rules that ask for a minimum
//! level, a resource's role, and how they cross files, MSPDI and table import.

use std::collections::BTreeMap;

use chrono::Datelike;

use sheafly_engine::command::Command;
use sheafly_engine::import::{check_import, ImportColumn, ImportResource, ImportTable};
use sheafly_engine::model::{ResourceKind, Task};
use sheafly_engine::mspdi::{read_mspdi_with_report, write_mspdi};
use sheafly_engine::skill_levels::{qualifies, SkillLevelInput, SkillLevelOf, MAX_SKILL_LEVELS};
use sheafly_engine::skills::{AssignmentRule, RuleTarget};
use sheafly_engine::store::{load, save};
use sheafly_engine::suggest::{suggest, NoPick};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Project};

const DAY: i64 = 480;

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(i64::from(day)))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

/// One task of a day and people 1..=`people`.
fn doc(people: u32) -> Document {
    let mut project = Project::new("Cấp độ", at(0, 8, 0));
    let mut task = Task::new(1, "Việc 1");
    task.duration = DAY;
    task.work = DAY;
    task.remaining_duration = DAY;
    task.remaining_work = DAY;
    project.tasks.push(task);
    project.next_task_uid = 2;
    let mut doc = Document::from_project(project);
    for person in 1..=people {
        apply(
            &mut doc,
            Command::AddResource {
                name: format!("Người {person}"),
                kind: ResourceKind::Person,
            },
        )
        .unwrap();
    }
    doc
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

fn new_level(name: &str) -> SkillLevelInput {
    SkillLevelInput {
        id: None,
        name: name.into(),
    }
}

fn keep(id: u32, name: &str) -> SkillLevelInput {
    SkillLevelInput {
        id: Some(id),
        name: name.into(),
    }
}

fn set_levels(doc: &mut Document, levels: Vec<SkillLevelInput>) -> Result<(), EngineError> {
    apply(doc, Command::SetSkillLevels { levels })
}

/// The ids of the scale, lowest first.
fn level_ids(doc: &Document) -> Vec<u32> {
    doc.project
        .skill_levels
        .iter()
        .map(|level| level.id)
        .collect()
}

fn add_skill(doc: &mut Document, name: &str) -> u32 {
    apply(doc, Command::AddSkill { name: name.into() }).unwrap();
    doc.project.skills.last().unwrap().id
}

fn set_skills(
    doc: &mut Document,
    uid: u32,
    skills: Vec<u32>,
    levels: &[(u32, u32)],
) -> Result<(), EngineError> {
    apply(
        doc,
        Command::SetResourceSkills {
            uid,
            skills,
            levels: levels
                .iter()
                .map(|&(skill, level)| SkillLevelOf { skill, level })
                .collect(),
        },
    )
}

fn levels_of(doc: &Document, uid: u32) -> BTreeMap<u32, u32> {
    resource(doc, uid).skill_levels.clone()
}

fn resource(doc: &Document, uid: u32) -> &sheafly_engine::model::Resource {
    doc.project
        .resources
        .iter()
        .find(|resource| resource.uid == uid)
        .unwrap()
}

fn skill_rule(id: u32, min_level: Option<u32>) -> AssignmentRule {
    AssignmentRule {
        conditions: Vec::new(),
        target: RuleTarget::Skill { id, min_level },
        units: 1.0,
    }
}

/// Junior < Mid < Senior, with their ids.
fn three_levels(doc: &mut Document) -> (u32, u32, u32) {
    set_levels(
        doc,
        vec![new_level("Junior"), new_level(" Mid "), new_level("Senior")],
    )
    .unwrap();
    let ids = level_ids(doc);
    (ids[0], ids[1], ids[2])
}

#[test]
fn the_scale_is_trimmed_checked_and_ids_are_never_reused() {
    let mut doc = doc(0);
    let (junior, mid, senior) = three_levels(&mut doc);
    assert_eq!(doc.project.skill_levels[1].name, "Mid");
    assert_eq!((junior, mid, senior), (1, 2, 3));

    assert_eq!(
        invalid(set_levels(&mut doc, vec![new_level("A"), new_level("a")])),
        Invalid::SkillLevelNameTaken
    );
    for bad in ["", "  ", "A,B", "A:B", &"x".repeat(31)] {
        assert_eq!(
            invalid(set_levels(&mut doc, vec![new_level(bad)])),
            Invalid::SkillLevelName,
            "{bad:?}"
        );
    }
    assert_eq!(
        invalid(set_levels(&mut doc, vec![keep(99, "Lạ")])),
        Invalid::NoSuchSkillLevel
    );
    assert_eq!(
        invalid(set_levels(
            &mut doc,
            vec![keep(junior, "A"), keep(junior, "B")]
        )),
        Invalid::NoSuchSkillLevel
    );
    let many = (0..=MAX_SKILL_LEVELS)
        .map(|n| new_level(&format!("L{n}")))
        .collect();
    assert_eq!(
        invalid(set_levels(&mut doc, many)),
        Invalid::TooManySkillLevels
    );

    // Renaming and moving keep ids; a removed id is not handed out again.
    set_levels(
        &mut doc,
        vec![keep(senior, "Chuyên gia"), keep(junior, "Mới")],
    )
    .unwrap();
    assert_eq!(level_ids(&doc), vec![senior, junior]);
    assert_eq!(doc.project.skill_levels[0].name, "Chuyên gia");
    set_levels(&mut doc, vec![keep(junior, "Mới"), new_level("Lead")]).unwrap();
    assert_eq!(level_ids(&doc), vec![junior, 4]);

    // Undo goes back to the scale before.
    doc.undo().unwrap();
    assert_eq!(level_ids(&doc), vec![senior, junior]);
}

#[test]
fn resources_hold_a_level_per_skill_and_it_is_checked() {
    let mut doc = doc(1);
    let (_, mid, senior) = three_levels(&mut doc);
    let backend = add_skill(&mut doc, "Backend");
    let qa = add_skill(&mut doc, "QA");
    set_skills(&mut doc, 1, vec![qa, backend], &[(backend, senior)]).unwrap();
    assert_eq!(resource(&doc, 1).skills, vec![backend, qa]);
    assert_eq!(levels_of(&doc, 1), BTreeMap::from([(backend, senior)]));

    assert_eq!(
        invalid(set_skills(&mut doc, 1, vec![backend], &[(qa, mid)])),
        Invalid::LevelWithoutSkill
    );
    assert_eq!(
        invalid(set_skills(&mut doc, 1, vec![backend], &[(backend, 42)])),
        Invalid::NoSuchSkillLevel
    );

    // Deleting a skill takes its level with it.
    apply(&mut doc, Command::DeleteSkill { id: backend }).unwrap();
    assert!(levels_of(&doc, 1).is_empty());
}

#[test]
fn a_removed_level_moves_people_and_rules_to_the_nearest_kept_level_below() {
    let mut doc = doc(3);
    let (junior, mid, senior) = three_levels(&mut doc);
    let skill = add_skill(&mut doc, "Backend");
    set_skills(&mut doc, 1, vec![skill], &[(skill, senior)]).unwrap();
    set_skills(&mut doc, 2, vec![skill], &[(skill, mid)]).unwrap();
    set_skills(&mut doc, 3, vec![skill], &[(skill, junior)]).unwrap();
    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![
                skill_rule(skill, Some(senior)),
                skill_rule(skill, Some(junior)),
            ],
        },
    )
    .unwrap();

    // Senior goes: its people and rules fall to Mid.
    set_levels(&mut doc, vec![keep(junior, "Junior"), keep(mid, "Mid")]).unwrap();
    assert_eq!(levels_of(&doc, 1), BTreeMap::from([(skill, mid)]));
    assert_eq!(
        doc.project.assignment_rules[0].target,
        RuleTarget::Skill {
            id: skill,
            min_level: Some(mid)
        }
    );

    // Junior goes, with nothing below it: no level, and the rule asks for none.
    set_levels(&mut doc, vec![keep(mid, "Mid")]).unwrap();
    assert!(levels_of(&doc, 3).is_empty());
    assert_eq!(levels_of(&doc, 2), BTreeMap::from([(skill, mid)]));
    assert_eq!(
        doc.project.assignment_rules[1].target,
        RuleTarget::Skill {
            id: skill,
            min_level: None
        }
    );
}

#[test]
fn a_minimum_level_limits_who_qualifies_and_who_is_suggested() {
    let mut doc = doc(4);
    let (junior, mid, senior) = three_levels(&mut doc);
    let skill = add_skill(&mut doc, "Backend");
    set_skills(&mut doc, 1, vec![skill], &[(skill, junior)]).unwrap();
    set_skills(&mut doc, 2, vec![skill], &[(skill, senior)]).unwrap();
    set_skills(&mut doc, 3, vec![skill], &[]).unwrap();
    set_skills(&mut doc, 4, vec![skill], &[(skill, mid)]).unwrap();

    let project = &doc.project;
    let who = |min: Option<u32>| -> Vec<u32> {
        project
            .resources
            .iter()
            .filter(|resource| qualifies(project, resource, skill, min))
            .map(|resource| resource.uid)
            .collect()
    };
    assert_eq!(who(None), vec![1, 2, 3, 4]);
    // No level ranks below every level.
    assert_eq!(who(Some(junior)), vec![1, 2, 4]);
    assert_eq!(who(Some(mid)), vec![2, 4]);
    assert_eq!(who(Some(senior)), vec![2]);

    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![skill_rule(skill, Some(mid))],
        },
    )
    .unwrap();
    let suggestion = &suggest(&doc.project)[0];
    let mut picked: Vec<u32> = suggestion
        .picks
        .iter()
        .map(|pick| pick.resource_uid)
        .collect();
    picked.sort_unstable();
    assert_eq!(picked, vec![2, 4]);

    // A level nobody has leaves the task with nobody to pick.
    set_levels(
        &mut doc,
        vec![
            keep(junior, "Junior"),
            keep(mid, "Mid"),
            keep(senior, "Senior"),
            new_level("Lead"),
        ],
    )
    .unwrap();
    let lead = *level_ids(&doc).last().unwrap();
    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![skill_rule(skill, Some(lead))],
        },
    )
    .unwrap();
    assert_eq!(
        suggest(&doc.project)[0].reason,
        Some(NoPick::NoOneWithSkill)
    );

    // A rule naming a level the scale lacks is refused.
    assert_eq!(
        invalid(apply(
            &mut doc,
            Command::SetAssignmentRules {
                rules: vec![skill_rule(skill, Some(77))]
            },
        )),
        Invalid::NoSuchSkillLevel
    );
}

#[test]
fn a_resource_group_is_trimmed_limited_and_undone() {
    let mut doc = doc(1);
    apply(
        &mut doc,
        Command::SetResourceGroup {
            uid: 1,
            group: "  Backend ".into(),
        },
    )
    .unwrap();
    assert_eq!(resource(&doc, 1).group, "Backend");
    assert_eq!(
        invalid(apply(
            &mut doc,
            Command::SetResourceGroup {
                uid: 1,
                group: "x".repeat(101)
            },
        )),
        Invalid::ResourceGroup
    );
    assert!(matches!(
        apply(
            &mut doc,
            Command::SetResourceGroup {
                uid: 9,
                group: "QA".into()
            }
        ),
        Err(EngineError::ResourceNotFound)
    ));
    doc.undo().unwrap();
    assert_eq!(resource(&doc, 1).group, "");
}

#[test]
fn skill_names_with_a_colon_are_refused() {
    let mut doc = doc(0);
    assert_eq!(
        invalid(apply(
            &mut doc,
            Command::AddSkill {
                name: "Web:Senior".into()
            }
        )),
        Invalid::SkillName
    );
}

#[test]
fn levels_and_groups_survive_a_file_and_old_projects_have_none() {
    let mut doc = doc(1);
    let (_, mid, _) = three_levels(&mut doc);
    let skill = add_skill(&mut doc, "Backend");
    set_skills(&mut doc, 1, vec![skill], &[(skill, mid)]).unwrap();
    apply(
        &mut doc,
        Command::SetResourceGroup {
            uid: 1,
            group: "Nhóm A".into(),
        },
    )
    .unwrap();
    apply(
        &mut doc,
        Command::SetAssignmentRules {
            rules: vec![skill_rule(skill, Some(mid))],
        },
    )
    .unwrap();

    let dir = std::env::temp_dir().join(format!("sheafly-levels-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("levels.sheafly").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let opened = load(&path).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(opened.project.skill_levels, doc.project.skill_levels);
    assert_eq!(
        opened.project.next_skill_level_id,
        doc.project.next_skill_level_id
    );
    assert_eq!(opened.project.resources, doc.project.resources);
    assert_eq!(
        opened.project.assignment_rules,
        doc.project.assignment_rules
    );

    // Without the new fields a project reads as one with no levels and no groups.
    let mut json = serde_json::to_value(&doc.project).unwrap();
    let object = json.as_object_mut().unwrap();
    object.remove("skill_levels");
    object.remove("next_skill_level_id");
    let person = json["resources"][0].as_object_mut().unwrap();
    person.remove("skill_levels");
    person.remove("group");
    let old: Project = serde_json::from_value(json).unwrap();
    assert!(old.skill_levels.is_empty());
    assert!(old.resources[0].skill_levels.is_empty() && old.resources[0].group.is_empty());
    // With no `next_skill_level_id` the next id is past every one in use.
    let mut old = Document::from_project(old);
    old.project.skill_levels = doc.project.skill_levels.clone();
    old.project.next_skill_level_id = 0;
    let mut scale: Vec<SkillLevelInput> = old
        .project
        .skill_levels
        .iter()
        .map(|level| keep(level.id, &level.name))
        .collect();
    scale.push(new_level("Lead"));
    set_levels(&mut old, scale).unwrap();
    assert_eq!(*level_ids(&old).last().unwrap(), 4);
}

#[test]
fn the_commands_read_the_shape_the_app_sends() {
    let sent =
        r#"{"type":"set_resource_skills","uid":1,"skills":[2,3],"levels":[{"skill":2,"level":5}]}"#;
    let Command::SetResourceSkills { levels, .. } = serde_json::from_str(sent).unwrap() else {
        panic!("not read as set_resource_skills");
    };
    assert_eq!(levels, vec![SkillLevelOf { skill: 2, level: 5 }]);
    let sent = r#"{"type":"set_skill_levels","levels":[{"id":1,"name":"Mid"},{"name":"Senior"}]}"#;
    let Command::SetSkillLevels { levels } = serde_json::from_str(sent).unwrap() else {
        panic!("not read as set_skill_levels");
    };
    assert_eq!(levels, vec![keep(1, "Mid"), new_level("Senior")]);
    let sent = r#"{"type":"set_resource_group","uid":1,"group":"QA"}"#;
    assert!(serde_json::from_str::<Command>(sent).is_ok());
    let sent = r#"{"type":"set_assignment_rules","rules":[
        {"conditions":[],"target":{"type":"skill","id":3,"min_level":2}}]}"#;
    let Command::SetAssignmentRules { rules } = serde_json::from_str(sent).unwrap() else {
        panic!("not read as set_assignment_rules");
    };
    assert_eq!(
        rules[0].target,
        RuleTarget::Skill {
            id: 3,
            min_level: Some(2)
        }
    );
    // A rule with no level writes none, so older files read the same.
    let written = serde_json::to_string(&RuleTarget::Skill {
        id: 3,
        min_level: None,
    })
    .unwrap();
    assert_eq!(written, r#"{"type":"skill","id":3}"#);
}

#[test]
fn mspdi_round_trip_keeps_levels_and_groups() {
    let mut doc = doc(2);
    let (_, mid, senior) = three_levels(&mut doc);
    let backend = add_skill(&mut doc, "Backend");
    let qa = add_skill(&mut doc, "QA");
    set_skills(&mut doc, 1, vec![backend, qa], &[(backend, senior)]).unwrap();
    set_skills(&mut doc, 2, vec![qa], &[(qa, mid)]).unwrap();
    apply(
        &mut doc,
        Command::SetResourceGroup {
            uid: 1,
            group: "Kỹ thuật".into(),
        },
    )
    .unwrap();

    let xml = write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<Value>Backend:Senior, QA</Value>"), "{xml}");
    assert!(xml.contains("<Group>Kỹ thuật</Group>"));
    // Without a group the kind is written, as before.
    assert!(xml.contains("<Group>Người</Group>"));

    let (back, _) = read_mspdi_with_report(&xml).unwrap();
    // Levels come back in the order the file names them: MSPDI has no scale.
    let names: Vec<&str> = back
        .skill_levels
        .iter()
        .map(|level| level.name.as_str())
        .collect();
    assert_eq!(names, vec!["Senior", "Mid"]);
    let level = |name: &str| {
        back.skill_levels
            .iter()
            .find(|level| level.name == name)
            .unwrap()
            .id
    };
    let skill = |name: &str| {
        back.skills
            .iter()
            .find(|skill| skill.name == name)
            .unwrap()
            .id
    };
    assert_eq!(
        back.resources[0].skill_levels,
        BTreeMap::from([(skill("Backend"), level("Senior"))])
    );
    assert_eq!(
        back.resources[1].skill_levels,
        BTreeMap::from([(skill("QA"), level("Mid"))])
    );
    assert_eq!(back.resources[0].group, "Kỹ thuật");
    assert_eq!(back.resources[1].group, "");
    assert_eq!(back.resources[1].kind, ResourceKind::Person);
}

#[test]
fn table_import_reads_levels_and_groups() {
    let mut doc = doc(0);
    three_levels(&mut doc);
    let person = |name: &str, skills: &[&str], group: &str| ImportResource {
        name: name.into(),
        skills: skills.iter().map(|skill| skill.to_string()).collect(),
        group: group.into(),
        ..ImportResource::default()
    };
    apply(
        &mut doc,
        Command::ImportRows {
            after_uid: None,
            resources: vec![
                person("An", &["Backend:senior", "QA"], "Kỹ thuật"),
                person("Bình", &["QA:Lead"], ""),
            ],
            tasks: Vec::new(),
        },
    )
    .unwrap();
    // A known level matches letter case aside; a new one goes on top of the scale.
    let names: Vec<&str> = doc
        .project
        .skill_levels
        .iter()
        .map(|level| level.name.as_str())
        .collect();
    assert_eq!(names, vec!["Junior", "Mid", "Senior", "Lead"]);
    let an = doc
        .project
        .resources
        .iter()
        .find(|resource| resource.name == "An")
        .unwrap();
    assert_eq!(an.group, "Kỹ thuật");
    assert_eq!(an.skill_levels.len(), 1);
    assert_eq!(an.skills.len(), 2);

    let bad = [
        person("Chi", &["Web:A,B"], ""),
        person("Dũng", &[], &"x".repeat(101)),
    ];
    let errors = check_import(&doc.project, None, &bad, &[]);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .all(|error| error.table == ImportTable::Resources));
    assert_eq!(errors[1].column, ImportColumn::Group);
    assert_eq!(
        invalid(apply(
            &mut doc,
            Command::ImportRows {
                after_uid: None,
                resources: bad.to_vec(),
                tasks: Vec::new()
            },
        )),
        Invalid::ImportHasErrors
    );
}
