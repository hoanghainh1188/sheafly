//! Suggestions from the assignment rules (phase 18 P2, step 4a): for each unassigned task,
//! the people the first matching rule points at, ranked by who would finish it first next
//! to the work they already have. A read: nothing goes into the command log.

use chrono::{Datelike, NaiveDate};

use sheafly_engine::availability::AvailabilityPeriod;
use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldKind, FieldValue};
use sheafly_engine::level_order::{Criterion, SortKey};
use sheafly_engine::model::{ResourceKind, Task, TaskType};
use sheafly_engine::skills::{AssignmentRule, Condition, RuleTarget};
use sheafly_engine::suggest::{suggest, NoPick, Pick, Suggestion, MAX_PICKS};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::work::with_first_assignment;
use sheafly_engine::{Document, Project};

const DAY: i64 = 480;

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 1, 5).unwrap() + chrono::Duration::days(i64::from(day))
}

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    let date = date(day);
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

/// Unassigned tasks 1..=`tasks` of one day each and people 1..=`people`.
fn doc(tasks: u32, people: u32) -> Document {
    let mut project = Project::new("Gợi ý", at(0, 8, 0));
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

/// A skill every one of `people` has.
fn skill_for(doc: &mut Document, people: &[u32]) -> u32 {
    doc.apply(Command::AddSkill {
        name: format!("Kỹ năng {}", doc.project.skills.len() + 1),
    })
    .unwrap();
    let id = doc.project.skills.last().unwrap().id;
    for &uid in people {
        let mut skills = doc
            .project
            .resources
            .iter()
            .find(|r| r.uid == uid)
            .unwrap()
            .skills
            .clone();
        skills.push(id);
        doc.apply(Command::SetResourceSkills { uid, skills, levels: Vec::new() })
            .unwrap();
    }
    id
}

fn rules(doc: &mut Document, rules: Vec<AssignmentRule>) {
    doc.apply(Command::SetAssignmentRules { rules }).unwrap();
}

fn everything_to(target: RuleTarget, units: f64) -> AssignmentRule {
    AssignmentRule {
        conditions: vec![],
        target,
        units,
    }
}

fn assign(doc: &mut Document, task_uid: u32, resource_uid: u32) {
    doc.apply(Command::AssignResource {
        task_uid,
        resource_uid,
        units: 1.0,
    })
    .unwrap();
}

fn of(suggestions: &[Suggestion], uid: u32) -> &Suggestion {
    suggestions
        .iter()
        .find(|s| s.task_uid == uid)
        .unwrap_or_else(|| panic!("no suggestion for {uid}"))
}

/// The people suggested for a task, best first.
fn people(suggestions: &[Suggestion], uid: u32) -> Vec<u32> {
    of(suggestions, uid)
        .picks
        .iter()
        .map(|pick| pick.resource_uid)
        .collect()
}

#[test]
fn a_rule_to_one_person_suggests_that_person_with_dates() {
    let mut doc = doc(1, 2);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 2 }, 1.0)],
    );
    let suggestions = suggest(&doc.project);
    assert_eq!(
        suggestions,
        vec![Suggestion {
            task_uid: 1,
            rule: Some(0),
            units: 1.0,
            picks: vec![Pick {
                resource_uid: 2,
                start: at(0, 8, 0),
                finish: at(0, 17, 0)
            }],
            reason: None,
        }]
    );
}

#[test]
fn people_with_the_skill_are_ranked_by_who_finishes_first() {
    let mut doc = doc(3, 3);
    let skill = skill_for(&mut doc, &[1, 2]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    // Person 1 already has two days of work from the start.
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 2 * DAY,
    })
    .unwrap();
    assign(&mut doc, 1, 1);
    doc.apply(Command::DeleteTask { uid: 3 }).unwrap();
    let suggestions = suggest(&doc.project);
    assert_eq!(suggestions.len(), 1, "assigned tasks get no suggestion");
    let picks = &of(&suggestions, 2).picks;
    assert_eq!(
        picks,
        &vec![
            Pick {
                resource_uid: 2,
                start: at(0, 8, 0),
                finish: at(0, 17, 0)
            },
            Pick {
                resource_uid: 1,
                start: at(2, 8, 0),
                finish: at(2, 17, 0)
            },
        ]
    );
}

#[test]
fn a_tie_goes_to_the_person_with_less_work_then_the_lower_uid() {
    let mut doc = doc(3, 3);
    let skill = skill_for(&mut doc, &[1, 2, 3]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    // Person 1 has work far ahead: free now, but more work than the others.
    doc.apply(Command::SetConstraint {
        uid: 1,
        constraint_type: sheafly_engine::model::ConstraintType::Snet,
        date: Some(at(20, 8, 0)),
    })
    .unwrap();
    assign(&mut doc, 1, 1);
    doc.apply(Command::DeleteTask { uid: 3 }).unwrap();
    assert_eq!(people(&suggest(&doc.project), 2), vec![2, 3, 1]);
}

#[test]
fn the_top_pick_takes_the_task_before_the_next_one_is_ranked() {
    let mut doc = doc(4, 2);
    let skill = skill_for(&mut doc, &[1, 2]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    let suggestions = suggest(&doc.project);
    let tops: Vec<(u32, u32, i64)> = suggestions
        .iter()
        .map(|s| (s.task_uid, s.picks[0].resource_uid, s.picks[0].finish))
        .collect();
    assert_eq!(
        tops,
        vec![
            (1, 1, at(0, 17, 0)),
            (2, 2, at(0, 17, 0)),
            (3, 1, at(1, 17, 0)),
            (4, 2, at(1, 17, 0)),
        ]
    );
    // The others are still offered, with their own dates.
    assert_eq!(
        of(&suggestions, 3).picks[1],
        Pick {
            resource_uid: 2,
            start: at(1, 8, 0),
            finish: at(1, 17, 0)
        }
    );
    // Nothing was written: the project is as it was.
    assert!(doc.project.assignments.is_empty());
}

#[test]
fn a_person_on_leave_comes_after_one_who_is_there() {
    let mut doc = doc(1, 2);
    let skill = skill_for(&mut doc, &[1, 2]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    let leave = AvailabilityPeriod {
        from: Some(date(0)),
        to: Some(date(4)),
        units: 0.0,
    };
    doc.apply(Command::SetResourceAvailability {
        uid: 1,
        periods: vec![leave],
    })
    .unwrap();
    let picks = of(&suggest(&doc.project), 1).picks.clone();
    assert_eq!(
        picks,
        vec![
            Pick {
                resource_uid: 2,
                start: at(0, 8, 0),
                finish: at(0, 17, 0)
            },
            Pick {
                resource_uid: 1,
                start: at(7, 8, 0),
                finish: at(7, 17, 0)
            },
        ]
    );
}

#[test]
fn the_rule_units_set_the_duration_as_assigning_would() {
    let mut doc = doc(1, 1);
    doc.apply(Command::SetTaskType {
        uid: 1,
        task_type: TaskType::FixedWork,
        effort_driven: true,
    })
    .unwrap();
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 1 }, 0.5)],
    );
    let suggestion = of(&suggest(&doc.project), 1).clone();
    assert_eq!(suggestion.units, 0.5);
    // A day of work at half time takes two days.
    assert_eq!(suggestion.picks[0].finish, at(1, 17, 0));
}

#[test]
fn the_duration_with_a_first_assignment_matches_assign_resource() {
    let types = [
        TaskType::FixedUnits,
        TaskType::FixedDuration,
        TaskType::FixedWork,
    ];
    for task_type in types {
        for effort_driven in [true, false] {
            for units in [0.5, 1.0, 2.0, 0.3] {
                let mut doc = doc(1, 1);
                doc.apply(Command::SetDuration {
                    uid: 1,
                    minutes: 3 * DAY + 7,
                })
                .unwrap();
                doc.apply(Command::SetTaskType {
                    uid: 1,
                    task_type,
                    effort_driven,
                })
                .unwrap();
                let task = doc.project.task(1).unwrap().clone();
                let expected = with_first_assignment(&task, units);
                doc.apply(Command::AssignResource {
                    task_uid: 1,
                    resource_uid: 1,
                    units,
                })
                .unwrap();
                let after = doc.project.task(1).unwrap();
                let case = format!("{task_type:?} effort {effort_driven} units {units}");
                assert_eq!(expected, (after.duration, after.work), "{case}");
            }
        }
    }
}

#[test]
fn the_leveling_order_decides_who_gets_a_free_person_first() {
    let mut doc = doc(2, 1);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 1 }, 1.0)],
    );
    let firsts = |doc: &Document| -> Vec<(u32, i64)> {
        suggest(&doc.project)
            .iter()
            .map(|s| (s.task_uid, s.picks[0].start))
            .collect()
    };
    assert_eq!(firsts(&doc), vec![(1, at(0, 8, 0)), (2, at(1, 8, 0))]);
    let order = vec![SortKey {
        criterion: Criterion::Id,
        descending: true,
    }];
    doc.apply(Command::SetLevelingOrder { order }).unwrap();
    assert_eq!(firsts(&doc), vec![(2, at(0, 8, 0)), (1, at(1, 8, 0))]);
    // The default order puts the higher priority first.
    doc.apply(Command::SetLevelingOrder { order: vec![] })
        .unwrap();
    doc.apply(Command::SetPriority {
        uid: 2,
        priority: 900,
    })
    .unwrap();
    assert_eq!(firsts(&doc), vec![(2, at(0, 8, 0)), (1, at(1, 8, 0))]);
}

#[test]
fn tasks_without_a_suggestion_say_why() {
    let mut doc = doc(3, 2);
    doc.apply(Command::AddField {
        name: "Mảng".into(),
        kind: FieldKind::Text,
    })
    .unwrap();
    let field = doc.project.fields[0].id;
    for (uid, value) in [(1, "kho"), (2, "web")] {
        let value = Some(FieldValue::Text(value.into()));
        doc.apply(Command::SetFieldValue {
            uid,
            field_id: field,
            value,
        })
        .unwrap();
    }
    let nobody = skill_for(&mut doc, &[]);
    let away = AvailabilityPeriod {
        from: None,
        to: None,
        units: 0.0,
    };
    doc.apply(Command::SetResourceAvailability {
        uid: 2,
        periods: vec![away],
    })
    .unwrap();
    let when = |value: &str| Condition {
        field,
        values: vec![FieldValue::Text(value.into())],
    };
    rules(
        &mut doc,
        vec![
            AssignmentRule {
                conditions: vec![when("kho")],
                target: RuleTarget::Skill { id: nobody, min_level: None },
                units: 1.0,
            },
            AssignmentRule {
                conditions: vec![when("web")],
                target: RuleTarget::Resource { uid: 2 },
                units: 1.0,
            },
        ],
    );
    let suggestions = suggest(&doc.project);
    let why = |uid| {
        let suggestion = of(&suggestions, uid);
        assert!(suggestion.picks.is_empty());
        (suggestion.rule, suggestion.reason)
    };
    assert_eq!(why(1), (Some(0), Some(NoPick::NoOneWithSkill)));
    assert_eq!(why(2), (Some(1), Some(NoPick::NoRoom)));
    assert_eq!(why(3), (None, Some(NoPick::NoRule)));
}

#[test]
fn only_unassigned_leaf_tasks_not_started_are_considered() {
    let mut doc = doc(6, 1);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 1 }, 1.0)],
    );
    doc.apply(Command::Indent { uid: 2 }).unwrap(); // task 1 becomes a summary
    doc.apply(Command::SetDuration { uid: 3, minutes: 0 })
        .unwrap(); // a milestone
    doc.apply(Command::SetPercentComplete {
        uid: 4,
        percent: 100.0,
    })
    .unwrap();
    // Someone else did part of it: a new person would not take over the done part.
    doc.apply(Command::SetPercentComplete {
        uid: 6,
        percent: 40.0,
    })
    .unwrap();
    let uids: Vec<u32> = suggest(&doc.project).iter().map(|s| s.task_uid).collect();
    assert_eq!(uids, vec![2, 5]);
}

#[test]
fn at_most_five_people_are_offered() {
    let mut doc = doc(1, 7);
    let skill = skill_for(&mut doc, &[1, 2, 3, 4, 5, 6, 7]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    // People 1 and 2 are busy, so the five offered are 3 to 7.
    for (task, person) in [(2, 1), (3, 2)] {
        doc.apply(Command::AddTask {
            name: "Bận".into(),
            after_uid: None,
        })
        .unwrap();
        let uid = doc.project.tasks.last().unwrap().uid;
        assert_eq!(uid, task);
        assign(&mut doc, uid, person);
    }
    assert_eq!(MAX_PICKS, 5);
    assert_eq!(people(&suggest(&doc.project), 1), vec![3, 4, 5, 6, 7]);
}

#[test]
fn suggestions_serialize_for_the_app() {
    let suggestion = Suggestion {
        task_uid: 1,
        rule: Some(2),
        units: 1.0,
        picks: vec![],
        reason: Some(NoPick::NoOneWithSkill),
    };
    let json = serde_json::to_value(&suggestion).unwrap();
    assert_eq!(json["reason"], "no_one_with_skill");
    assert_eq!(json["rule"], 2);
}

#[test]
fn a_task_that_cannot_move_goes_only_to_people_free_at_its_dates() {
    let mut doc = doc(3, 3);
    let skill = skill_for(&mut doc, &[1, 2, 3]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    // Person 1 has task 1 on day 0; task 2 must start on day 0.
    assign(&mut doc, 1, 1);
    doc.apply(Command::SetConstraint {
        uid: 2,
        constraint_type: sheafly_engine::model::ConstraintType::Mso,
        date: Some(at(0, 8, 0)),
    })
    .unwrap();
    let suggestions = suggest(&doc.project);
    // It comes first, as in leveling, and keeps its dates; person 1 is not offered.
    assert_eq!(suggestions[0].task_uid, 2);
    assert_eq!(
        suggestions[0].picks,
        vec![
            Pick {
                resource_uid: 2,
                start: at(0, 8, 0),
                finish: at(0, 17, 0)
            },
            Pick {
                resource_uid: 3,
                start: at(0, 8, 0),
                finish: at(0, 17, 0)
            },
        ]
    );
    // Person 2 took it, so the free task 3 goes to person 3 on day 0.
    assert_eq!(
        of(&suggestions, 3).picks[0],
        Pick {
            resource_uid: 3,
            start: at(0, 8, 0),
            finish: at(0, 17, 0)
        }
    );

    // With everyone busy at its dates there is no one to offer.
    let mut busy = doc.clone();
    for (task, person) in [(3, 2), (3, 3)] {
        assign(&mut busy, task, person);
    }
    let suggestion = of(&suggest(&busy.project), 2).clone();
    assert!(suggestion.picks.is_empty());
    assert_eq!(suggestion.reason, Some(NoPick::NoRoom));
}

#[test]
fn occurrences_of_a_recurring_series_get_no_suggestion() {
    use sheafly_engine::recurrence::{RecurrenceEnd, RecurrencePattern, RecurrenceSpec};
    let mut doc = doc(1, 1);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 1 }, 1.0)],
    );
    let spec = RecurrenceSpec {
        name: "Họp".into(),
        duration: 60,
        start: at(0, 8, 0),
        pattern: RecurrencePattern::Weekly {
            every: 1,
            weekdays: vec![0],
        },
        end: RecurrenceEnd::Count { count: 3 },
        calendar_id: None,
        resource_uids: Vec::new(),
        resources: Vec::new(),
        background: false,
    };
    doc.apply(Command::CreateRecurringTask {
        after_uid: Some(1),
        spec,
    })
    .unwrap();
    assert!(doc
        .project
        .tasks
        .iter()
        .any(|task| task.series_uid.is_some()));
    // Assigning each occurrence would take it out of its series.
    let uids: Vec<u32> = suggest(&doc.project).iter().map(|s| s.task_uid).collect();
    assert_eq!(uids, vec![1]);
}

#[test]
fn work_with_no_working_time_fits_anyone_at_once() {
    // A fixed-work task with no work takes no time whoever does it, so the people on
    // leave rank by load and uid like the others; their leave says nothing here.
    let mut doc = doc(1, 8);
    let skill = skill_for(&mut doc, &[1, 2, 3, 4, 5, 6, 7, 8]);
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Skill { id: skill, min_level: None }, 1.0)],
    );
    doc.apply(Command::SetTaskType {
        uid: 1,
        task_type: TaskType::FixedWork,
        effort_driven: true,
    })
    .unwrap();
    doc.apply(Command::SetWork { uid: 1, minutes: 0 }).unwrap();
    // Not a milestone: it has a duration until someone is assigned.
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: DAY,
    })
    .unwrap();
    assert!(!doc.project.task(1).unwrap().milestone);
    let leave = AvailabilityPeriod {
        from: Some(date(0)),
        to: Some(date(4)),
        units: 0.0,
    };
    for uid in 1..=6 {
        doc.apply(Command::SetResourceAvailability {
            uid,
            periods: vec![leave.clone()],
        })
        .unwrap();
    }
    assert_eq!(people(&suggest(&doc.project), 1), vec![1, 2, 3, 4, 5]);
}

#[test]
fn a_task_held_at_its_finish_ends_there_when_the_rule_changes_its_length() {
    let mut doc = doc(1, 1);
    doc.apply(Command::SetTaskType {
        uid: 1,
        task_type: TaskType::FixedWork,
        effort_driven: true,
    })
    .unwrap();
    doc.apply(Command::SetConstraint {
        uid: 1,
        constraint_type: sheafly_engine::model::ConstraintType::Mfo,
        date: Some(at(4, 17, 0)),
    })
    .unwrap();
    rules(
        &mut doc,
        vec![everything_to(RuleTarget::Resource { uid: 1 }, 0.5)],
    );
    // A day of work at half time takes two days, ending on the fixed finish.
    let picks = of(&suggest(&doc.project), 1).picks.clone();
    assert_eq!(
        picks,
        vec![Pick {
            resource_uid: 1,
            start: at(3, 8, 0),
            finish: at(4, 17, 0)
        }]
    );
}
