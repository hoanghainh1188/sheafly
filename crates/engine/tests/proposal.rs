//! AI proposals (phase 22 PR D): `ApplyProposal` applies a list of allowed commands as one
//! undo step, all or nothing, and `trial_proposal` measures a proposal on a copy (D5).

use chrono::Datelike;

use sheafly_engine::advice::{Goal, Proposal};
use sheafly_engine::command::Command;
use sheafly_engine::level::over_minutes;
use sheafly_engine::model::{DepKind, ResourceKind, Task, TaskType};
use sheafly_engine::proposal::{allowed_in_proposal, trial_proposal, Measure, Verdict};
use sheafly_engine::store::{load, save, FORMAT_VERSION};
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

/// Tasks 1..=`tasks` of `days` of work each (Fixed Work), all on person 1 at full time, so
/// they run side by side and overallocate them.
fn doc(tasks: u32, days: i64) -> Document {
    let mut project = Project::new("Đề xuất", at(0, 8, 0));
    for uid in 1..=tasks {
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.task_type = TaskType::FixedWork;
        task.effort_driven = true;
        task.duration = DAY * days;
        task.work = DAY * days;
        task.remaining_duration = DAY * days;
        task.remaining_work = DAY * days;
        project.tasks.push(task);
    }
    project.next_task_uid = tasks + 1;
    let mut doc = Document::from_project(project);
    doc.apply(Command::AddResource { name: "Lan".into(), kind: ResourceKind::Person }).unwrap();
    for uid in 1..=tasks {
        doc.apply(Command::AssignResource { task_uid: uid, resource_uid: 1, units: 1.0 }).unwrap();
    }
    doc
}

fn proposal(goal: Goal, commands: Vec<Command>, level: bool) -> Proposal {
    Proposal { id: "toolu_1".into(), goal, commands, level, reason: "Vì T1".into() }
}

fn link(from: u32, to: u32) -> Command {
    Command::AddDependency { predecessor_uid: from, successor_uid: to, kind: DepKind::Fs, lag: 0 }
}

#[test]
fn only_the_commands_of_d2_are_allowed() {
    let allowed = [
        Command::SetDuration { uid: 1, minutes: DAY },
        Command::SetWork { uid: 1, minutes: DAY },
        Command::RemoveAssignment { uid: 1 },
        Command::SetLevelingDelay { uid: 1, minutes: 0 },
        Command::ApplyLeveling { delays: vec![], splits: vec![] },
        link(1, 2),
    ];
    assert!(allowed.iter().all(allowed_in_proposal));
    let refused = [
        Command::LevelResources,
        Command::DeleteTask { uid: 1 },
        Command::SetPercentComplete { uid: 1, percent: 50.0 },
        Command::ApplyProposal { commands: vec![] },
        Command::ApplyPlan { assignments: vec![], delays: vec![], splits: vec![] },
        Command::SetStatusDate { date: None },
    ];
    assert!(!refused.iter().any(allowed_in_proposal));
}

#[test]
fn a_proposal_is_one_undo_step() {
    let mut doc = doc(2, 1);
    let before = doc.project.clone();
    doc.apply(Command::ApplyProposal {
        commands: vec![link(1, 2), Command::SetPriority { uid: 2, priority: 800 }],
    })
    .unwrap();
    assert_eq!(doc.project.task(2).unwrap().start, at(1, 8, 0));
    assert_eq!(doc.project.task(2).unwrap().priority, 800);
    doc.undo().unwrap();
    assert_eq!(doc.project, before);
    doc.redo().unwrap();
    assert_eq!(doc.project.task(2).unwrap().priority, 800);
}

#[test]
fn one_failing_command_fails_the_whole_proposal() {
    let mut doc = doc(2, 1);
    let before = doc.project.clone();
    let commands = vec![Command::SetPriority { uid: 2, priority: 800 }, Command::SetDuration { uid: 99, minutes: DAY }];
    assert!(doc.apply(Command::ApplyProposal { commands }).is_err());
    assert_eq!(doc.project, before);
}

#[test]
fn forbidden_nested_and_empty_proposals_are_refused() {
    let mut doc = doc(2, 1);
    let before = doc.project.clone();
    let refused = |commands: Vec<Command>| Command::ApplyProposal { commands };
    let not_allowed = EngineError::from(Invalid::NotInProposal);
    assert_eq!(doc.apply(refused(vec![Command::DeleteTask { uid: 1 }])).unwrap_err(), not_allowed);
    assert_eq!(doc.apply(refused(vec![link(1, 2), Command::LevelResources])).unwrap_err(), not_allowed);
    let nested = refused(vec![refused(vec![link(1, 2)])]);
    assert_eq!(doc.apply(nested).unwrap_err(), not_allowed);
    assert_eq!(doc.apply(refused(vec![])).unwrap_err(), EngineError::from(Invalid::EmptyProposal));
    assert_eq!(doc.project, before);
}

#[test]
fn a_file_with_a_proposal_replays_and_needs_the_new_format() {
    const { assert!(FORMAT_VERSION >= 25, "ApplyProposal needs a format older apps refuse") };
    let mut doc = doc(2, 1);
    let trial = trial_proposal(&doc.project, &proposal(Goal::Overallocation { resource: 1 }, vec![], true)).unwrap();
    doc.apply(Command::ApplyProposal { commands: trial.commands }).unwrap();
    let dir = std::env::temp_dir().join(format!("sheafly-proposal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("proposal.sheafly").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let opened = load(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(opened.project, doc.project);
    assert!(over_minutes(&opened.project).is_empty());
}

#[test]
fn leveling_relieves_the_person_and_counts_as_improved() {
    let doc = doc(2, 1);
    let before = doc.project.clone();
    assert_eq!(over_minutes(&doc.project).get(&1), Some(&DAY));
    let trial = trial_proposal(&doc.project, &proposal(Goal::Overallocation { resource: 1 }, vec![], true)).unwrap();
    assert_eq!(doc.project, before, "the trial runs on a copy");
    assert!(matches!(trial.commands.last(), Some(Command::ApplyLeveling { .. })));
    assert_eq!((trial.target_before, trial.target_after), (DAY, 0));
    assert_eq!(trial.before.over_minutes, DAY);
    assert_eq!(trial.after.over_minutes, 0);
    // The finish moves by one working day: within the threshold.
    assert_eq!(trial.after.finish, at(1, 17, 0));
    assert_eq!(trial.verdict, Verdict::Improved);
    assert!(trial.worse.is_empty());
    assert_eq!(trial.moves.len(), 1);
    assert!(trial.overallocated.is_empty());

    // What the trial measured is what applying builds.
    let mut applied = doc;
    applied.apply(Command::ApplyProposal { commands: trial.commands.clone() }).unwrap();
    assert_eq!(applied.project.task(2).unwrap().finish, trial.moves[0].finish);
}

#[test]
fn a_fix_that_slips_the_finish_too_far_is_a_trade_off() {
    let doc = doc(2, 2);
    let trial = trial_proposal(&doc.project, &proposal(Goal::Overallocation { resource: 1 }, vec![link(1, 2)], false)).unwrap();
    assert_eq!(trial.target_after, 0);
    assert_eq!(trial.verdict, Verdict::TradeOff);
    assert_eq!(trial.worse, vec![Measure::Finish]);
}

#[test]
fn a_proposal_that_misses_its_goal_is_worse() {
    let doc = doc(2, 1);
    let longer = Command::SetDuration { uid: 1, minutes: 3 * DAY };
    let trial = trial_proposal(&doc.project, &proposal(Goal::Finish, vec![longer], false)).unwrap();
    assert_eq!(trial.verdict, Verdict::Worse);
    assert!(trial.worse.contains(&Measure::Finish));

    let nothing = Command::SetPriority { uid: 1, priority: 600 };
    let trial = trial_proposal(&doc.project, &proposal(Goal::Deadline { task: 1 }, vec![nothing], false)).unwrap();
    assert_eq!(trial.verdict, Verdict::Worse, "unchanged counts with worse");
}

#[test]
fn the_engine_refuses_what_it_cannot_apply() {
    let doc = doc(2, 1);
    let missing = proposal(Goal::Finish, vec![Command::SetDuration { uid: 99, minutes: DAY }], false);
    assert_eq!(trial_proposal(&doc.project, &missing).unwrap_err(), EngineError::TaskNotFound);
    let leveling = proposal(Goal::Finish, vec![Command::ApplyLeveling { delays: vec![], splits: vec![] }], false);
    assert_eq!(trial_proposal(&doc.project, &leveling).unwrap_err(), EngineError::from(Invalid::NotInProposal));
    let cycle = proposal(Goal::Finish, vec![link(1, 2), link(2, 1)], false);
    assert!(trial_proposal(&doc.project, &cycle).is_err());
    let nobody = proposal(Goal::Overallocation { resource: 9 }, vec![link(1, 2)], false);
    assert_eq!(trial_proposal(&doc.project, &nobody).unwrap_err(), EngineError::ResourceNotFound);
}

#[test]
fn an_estimate_fix_is_judged_by_the_measures_alone() {
    // Task 1 takes 1 day; putting its estimate right at 2 days slips nothing past a day.
    let one = doc(1, 1);
    let truer = Command::SetWork { uid: 1, minutes: 2 * DAY };
    let trial = trial_proposal(&one.project, &proposal(Goal::Estimate { task: 1 }, vec![truer], false)).unwrap();
    assert!(trial.target_after > trial.target_before, "the task finishes later, and that is the point");
    assert_eq!(trial.verdict, Verdict::Improved);

    // Linking two tasks on one person makes the finish a day later and more critical work.
    let two = doc(2, 2);
    let trial = trial_proposal(&two.project, &proposal(Goal::Estimate { task: 2 }, vec![link(1, 2)], false)).unwrap();
    assert_eq!(trial.verdict, Verdict::TradeOff);
    assert!(trial.worse.contains(&Measure::Finish));

    let gone = proposal(Goal::Estimate { task: 9 }, vec![link(1, 2)], false);
    assert_eq!(trial_proposal(&two.project, &gone).unwrap_err(), EngineError::TaskNotFound);
}
