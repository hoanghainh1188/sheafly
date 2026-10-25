//! Integration tests for S-curves and periodic curves calculation (PR 6, issue #150).

use sheafly_engine::command::Command;
use sheafly_engine::curves::{curves, CurveFilter, CurveMeasure};
use sheafly_engine::evm;
use sheafly_engine::load_map::Bucket;
use sheafly_engine::model::{Accrual, ResourceKind};
use sheafly_engine::skill_levels::{SkillLevelInput, SkillLevelOf};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::Document;

const DAY: i64 = 480; // 8 hours in minutes

fn at(day: u32, hour: u32, minute: u32) -> i64 {
    timestamp(2026, 3, 2 + day, hour, minute)
}

fn setup_base_document() -> Document {
    let mut doc = Document::new_default();
    // Remove the initial default task so tests start with a clean empty slate
    if let Some(first) = doc.project.tasks.first() {
        let uid = first.uid;
        doc.apply(Command::DeleteTask { uid }).unwrap();
    }
    doc.apply(Command::SetProjectStart {
        start: at(0, 8, 0),
    })
    .unwrap();
    doc
}

#[test]
fn test_curves_empty_project() {
    let doc = setup_base_document();
    let report = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, None).unwrap();
    assert_eq!(report.points.len(), 0);
    assert_eq!(report.total_scheduled, 0.0);
    assert!(report.total_baseline.is_none());
}

#[test]
fn test_curves_hand_computed_single_task() {
    let mut doc = setup_base_document();
    // Create Task 1: 5 days (40h), starting Mon 2026-03-02
    doc.apply(Command::AddTask { name: "Task 1".into(), after_uid: None }).unwrap();
    let task_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: task_uid, minutes: 5 * DAY }).unwrap();
    doc.apply(Command::SetFixedCost { uid: task_uid, cost: 500.0, accrual: Accrual::Prorated }).unwrap();

    // Set Baseline 0
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    // 1. Measure: Work (hours)
    let work_rep = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert_eq!(work_rep.points.len(), 5);
    assert_eq!(work_rep.total_scheduled, 40.0);
    assert_eq!(work_rep.total_baseline, Some(40.0));
    for (i, p) in work_rep.points.iter().enumerate() {
        assert_eq!(p.scheduled_period, 8.0);
        assert_eq!(p.baseline_period, Some(8.0));
        assert_eq!(p.scheduled_cumulative, (i as f64 + 1.0) * 8.0);
        assert_eq!(p.baseline_cumulative, Some((i as f64 + 1.0) * 8.0));
    }

    // 2. Measure: Cost
    let cost_rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert_eq!(cost_rep.total_scheduled, 500.0);
    assert_eq!(cost_rep.total_baseline, Some(500.0));
    for (i, p) in cost_rep.points.iter().enumerate() {
        assert!((p.scheduled_period - 100.0).abs() < 1e-4);
        assert!((p.baseline_period.unwrap() - 100.0).abs() < 1e-4);
        assert!((p.scheduled_cumulative - (i as f64 + 1.0) * 100.0).abs() < 1e-4);
    }

    // 3. Measure: Progress
    let prog_rep = curves(&doc.project, CurveMeasure::Progress, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert!((prog_rep.total_scheduled - 100.0).abs() < 1e-4);
    assert!((prog_rep.total_baseline.unwrap() - 100.0).abs() < 1e-4);
    for (i, p) in prog_rep.points.iter().enumerate() {
        assert!((p.scheduled_period - 20.0).abs() < 1e-4);
        assert!((p.scheduled_cumulative - (i as f64 + 1.0) * 20.0).abs() < 1e-4);
    }
}

#[test]
fn test_curves_no_baseline() {
    let mut doc = setup_base_document();
    doc.apply(Command::AddTask { name: "Task 1".into(), after_uid: None }).unwrap();
    let task_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: task_uid, minutes: 2 * DAY }).unwrap();

    // No baseline set:
    let report = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, None).unwrap();
    assert_eq!(report.total_scheduled, 16.0);
    assert!(report.total_baseline.is_none());
    assert_eq!(report.points.len(), 2);
    for p in &report.points {
        assert!(p.baseline_period.is_none());
        assert!(p.baseline_cumulative.is_none());
    }

    // Progress without baseline normalizes against scheduled work
    let prog = curves(&doc.project, CurveMeasure::Progress, Bucket::Day, CurveFilter::Project, None).unwrap();
    assert!((prog.total_scheduled - 100.0).abs() < 1e-4);
    assert_eq!(prog.points[0].scheduled_period, 50.0);
    assert_eq!(prog.points[1].scheduled_cumulative, 100.0);
}

#[test]
fn test_curves_split_task_skips_gap() {
    let mut doc = setup_base_document();
    doc.apply(Command::AddTask { name: "Split Task".into(), after_uid: None }).unwrap();
    let task_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: task_uid, minutes: 4 * DAY }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    // Split after 2 days (Mon, Tue), skip 2 days (Wed, Thu)
    doc.apply(Command::SplitTask { uid: task_uid, at: at(2, 8, 0), gap: 2 * DAY }).unwrap();

    let report = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    // Monday (03-02), Tuesday (03-03), Wednesday (03-04), Thursday (03-05), Friday (03-06), Monday (03-09)
    // Wed & Thu are in split gap for scheduled work
    let wed_pt = report.points.iter().find(|p| p.label == "2026-03-04").unwrap();
    let thu_pt = report.points.iter().find(|p| p.label == "2026-03-05").unwrap();
    assert_eq!(wed_pt.scheduled_period, 0.0);
    assert_eq!(thu_pt.scheduled_period, 0.0);
    // Baseline was set before split, so baseline continues on Wed & Thu
    assert_eq!(wed_pt.baseline_period, Some(8.0));
    assert_eq!(thu_pt.baseline_period, Some(8.0));
}

#[test]
fn test_curves_progress_and_actual_up_to_status_date() {
    let mut doc = setup_base_document();
    doc.apply(Command::AddTask { name: "Task 1".into(), after_uid: None }).unwrap();
    let task_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: task_uid, minutes: 4 * DAY }).unwrap();
    doc.apply(Command::SetFixedCost { uid: task_uid, cost: 400.0, accrual: Accrual::Prorated }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    // Set Status date to end of Day 2 (Tuesday 17:00)
    let status = at(1, 17, 0);
    doc.apply(Command::SetStatusDate { date: Some(status) }).unwrap();

    // 50% progress completed
    doc.apply(Command::SetPercentComplete { uid: task_uid, percent: 50.0 }).unwrap();
    doc.apply(Command::SetEditableActualCosts { editable: true }).unwrap();
    doc.apply(Command::SetTaskActualCost { uid: task_uid, cost: Some(250.0) }).unwrap();

    let cost_rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert_eq!(cost_rep.total_actual, Some(250.0));
    assert_eq!(cost_rep.total_earned, Some(200.0)); // 50% of 400

    let mon = cost_rep.points.iter().find(|p| p.label == "2026-03-02").unwrap();
    let tue = cost_rep.points.iter().find(|p| p.label == "2026-03-03").unwrap();
    let wed = cost_rep.points.iter().find(|p| p.label == "2026-03-04").unwrap();

    assert!(mon.actual_period.is_some());
    assert!(tue.actual_period.is_some());
    assert_eq!(tue.actual_cumulative, Some(250.0));
    assert_eq!(tue.earned_cumulative, Some(200.0));

    // Wednesday is past status date -> actual must be None
    assert!(wed.actual_period.is_none());
    assert!(wed.actual_cumulative.is_none());
}

#[test]
fn test_curves_filter_person_role_and_skill() {
    let mut doc = setup_base_document();

    // Skill 1: Rust
    doc.apply(Command::AddSkill { name: "Rust".into() }).unwrap();
    let skill_uid = doc.project.skills[0].id;
    doc.apply(Command::SetSkillLevels {
        levels: vec![
            SkillLevelInput { id: None, name: "Junior".into() },
            SkillLevelInput { id: None, name: "Senior".into() },
            SkillLevelInput { id: None, name: "Lead".into() },
        ],
    }).unwrap();

    // Resource 1: Alice (Role: Backend, Rust Senior [id 2])
    doc.apply(Command::AddResource { name: "Alice".into(), kind: ResourceKind::Person }).unwrap();
    let r1_uid = doc.project.resources[0].uid;
    doc.apply(Command::SetResourceGroup { uid: r1_uid, group: "Backend".into() }).unwrap();
    doc.apply(Command::SetResourceSkills {
        uid: r1_uid,
        skills: vec![skill_uid],
        levels: vec![SkillLevelOf { skill: skill_uid, level: 2 }],
    }).unwrap();

    // Resource 2: Bob (Role: Frontend, No Rust skill)
    doc.apply(Command::AddResource { name: "Bob".into(), kind: ResourceKind::Person }).unwrap();
    let r2_uid = doc.project.resources[1].uid;
    doc.apply(Command::SetResourceGroup { uid: r2_uid, group: "Frontend".into() }).unwrap();

    // Task 1: 3 days, assigned to Alice
    doc.apply(Command::AddTask { name: "Task 1".into(), after_uid: None }).unwrap();
    let t1_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: t1_uid, minutes: 3 * DAY }).unwrap();
    doc.apply(Command::AssignResource { task_uid: t1_uid, resource_uid: r1_uid, units: 1.0 }).unwrap();

    // Task 2: 2 days, assigned to Bob
    doc.apply(Command::AddTask { name: "Task 2".into(), after_uid: None }).unwrap();
    let t2_uid = doc.project.tasks[1].uid;
    doc.apply(Command::SetDuration { uid: t2_uid, minutes: 2 * DAY }).unwrap();
    doc.apply(Command::AssignResource { task_uid: t2_uid, resource_uid: r2_uid, units: 1.0 }).unwrap();

    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    // Filter Project: Total scheduled work = 3*8 + 2*8 = 40 hours
    let rep_all = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert_eq!(rep_all.total_scheduled, 40.0);

    // Filter Person Alice: Total scheduled work = 24 hours
    let rep_alice = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Person { uid: r1_uid }, Some(0)).unwrap();
    assert_eq!(rep_alice.total_scheduled, 24.0);

    // Filter Group Backend: matches Alice (24 hours)
    let rep_backend = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Group { name: "Backend".into() }, Some(0)).unwrap();
    assert_eq!(rep_backend.total_scheduled, 24.0);

    // Filter Skill Rust Min Senior (rank 2): matches Alice (24 hours)
    let rep_skill = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Skill { id: skill_uid, min_level: Some(2) }, Some(0)).unwrap();
    assert_eq!(rep_skill.total_scheduled, 24.0);

    // Filter Skill Rust Min rank 3: nobody qualifies (0 hours)
    let rep_skill_high = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Skill { id: skill_uid, min_level: Some(3) }, Some(0)).unwrap();
    assert_eq!(rep_skill_high.total_scheduled, 0.0);
}

#[test]
fn test_evm_s_curve_equality() {
    let mut doc = setup_base_document();
    doc.apply(Command::AddTask { name: "Task 1".into(), after_uid: None }).unwrap();
    let t1_uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: t1_uid, minutes: 5 * DAY }).unwrap();
    doc.apply(Command::SetFixedCost { uid: t1_uid, cost: 1000.0, accrual: Accrual::Prorated }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    let status = at(2, 17, 0); // Wed 17:00 (3 days in)
    doc.apply(Command::SetStatusDate { date: Some(status) }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: t1_uid, percent: 60.0 }).unwrap();
    doc.apply(Command::SetEditableActualCosts { editable: true }).unwrap();
    doc.apply(Command::SetTaskActualCost { uid: t1_uid, cost: Some(650.0) }).unwrap();

    let evm_s = evm::s_curve(&doc.project);
    let curve_rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();

    // 1. Total baseline matches EVM BAC / final PV
    let final_evm_pv = evm_s.points.last().unwrap().pv;
    assert_eq!(curve_rep.total_baseline, Some(final_evm_pv));
    assert_eq!(curve_rep.total_scheduled, final_evm_pv);

    // 2. Total actual matches EVM AC at status date
    assert_eq!(curve_rep.total_actual, Some(650.0));

    // 3. Total earned matches EVM EV at status date
    assert_eq!(curve_rep.total_earned, Some(600.0));

    // 4. Status date point in EVM curve matches curve's cumulative up to Wednesday
    let evm_status_pt = evm_s.points.iter().find(|p| p.date == status).unwrap();
    let wed_curve_pt = curve_rep.points.iter().find(|p| p.label == "2026-03-04").unwrap();

    assert!((wed_curve_pt.baseline_cumulative.unwrap() - evm_status_pt.pv).abs() < 1e-4);
    assert!((wed_curve_pt.actual_cumulative.unwrap() - evm_status_pt.ac.unwrap()).abs() < 1e-4);
    assert!((wed_curve_pt.earned_cumulative.unwrap() - evm_status_pt.ev.unwrap()).abs() < 1e-4);
}

/// Two people, one task each over the same three days, a baseline, and both half done on
/// the status date: Alice at 10/h plus 50 per use, Bob at 20/h.
fn two_people_half_done() -> (Document, u32, u32) {
    let mut doc = setup_base_document();
    for (name, rate, per_use) in [("Alice", 10.0, 50.0), ("Bob", 20.0, 0.0)] {
        doc.apply(Command::AddResource { name: name.into(), kind: ResourceKind::Person }).unwrap();
        let uid = doc.project.resources.last().unwrap().uid;
        doc.apply(Command::UpdateResource {
            uid,
            name: name.into(),
            kind: ResourceKind::Person,
            max_units: 1.0,
            calendar_id: None,
            standard_rate: rate,
            overtime_rate: 0.0,
            cost_per_use: per_use,
        })
        .unwrap();
        doc.apply(Command::AddTask { name: format!("{name}'s task"), after_uid: None }).unwrap();
        let task = doc.project.tasks.last().unwrap().uid;
        doc.apply(Command::SetDuration { uid: task, minutes: 3 * DAY }).unwrap();
        doc.apply(Command::AssignResource { task_uid: task, resource_uid: uid, units: 1.0 }).unwrap();
    }
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetStatusDate { date: Some(at(1, 12, 0)) }).unwrap();
    for task in doc.project.tasks.iter().map(|t| t.uid).collect::<Vec<_>>() {
        doc.apply(Command::SetPercentComplete { uid: task, percent: 50.0 }).unwrap();
    }
    let alice = doc.project.resources[0].uid;
    let bob = doc.project.resources[1].uid;
    (doc, alice, bob)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn no_baseline_chosen_draws_neither_baseline_nor_earned_value() {
    let (doc, alice, _) = two_people_half_done();
    for filter in [CurveFilter::Project, CurveFilter::Person { uid: alice }] {
        let rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, filter.clone(), None).unwrap();
        assert_eq!(rep.baseline, None, "{filter:?}");
        assert_eq!(rep.total_baseline, None, "{filter:?}");
        assert_eq!(rep.total_earned, None, "{filter:?}");
        assert!(rep.points.iter().all(|p| p.baseline_period.is_none() && p.earned_period.is_none()));
        // The scheduled and actual series stay.
        assert!(rep.total_scheduled > 0.0);
        assert!(rep.total_actual.is_some());
    }
    // A baseline that no task holds is the same as none.
    let rep = curves(&doc.project, CurveMeasure::Work, Bucket::Day, CurveFilter::Project, Some(3)).unwrap();
    assert_eq!(rep.baseline, None);
    assert_eq!(rep.total_baseline, None);
}

#[test]
fn earned_value_follows_the_filter() {
    let (doc, alice, bob) = two_people_half_done();
    let cost = |filter| {
        curves(&doc.project, CurveMeasure::Cost, Bucket::Day, filter, Some(0)).unwrap()
    };
    let all = cost(CurveFilter::Project);
    let a = cost(CurveFilter::Person { uid: alice });
    let b = cost(CurveFilter::Person { uid: bob });
    // Alice: 24 h × 10 + 50 per use = 290 planned, half earned; Bob: 24 h × 20 = 480.
    assert!(close(a.total_earned.unwrap(), 145.0), "{:?}", a.total_earned);
    assert!(close(b.total_earned.unwrap(), 240.0), "{:?}", b.total_earned);
    assert!(close(a.total_earned.unwrap() + b.total_earned.unwrap(), all.total_earned.unwrap()));
}

#[test]
fn filtered_costs_are_the_assignments_costs_and_add_up_to_the_project() {
    let (doc, alice, bob) = two_people_half_done();
    let cost = |filter| {
        curves(&doc.project, CurveMeasure::Cost, Bucket::Day, filter, Some(0)).unwrap()
    };
    let all = cost(CurveFilter::Project);
    let a = cost(CurveFilter::Person { uid: alice });
    let b = cost(CurveFilter::Person { uid: bob });
    // The cost per use is part of Alice's cost, scheduled and in the baseline she saved.
    let alice_cost = doc.project.assignments.iter().find(|x| x.resource_uid == alice).unwrap().cost;
    assert!(close(alice_cost, 290.0));
    assert!(close(a.total_scheduled, 290.0), "{}", a.total_scheduled);
    assert!(close(a.total_baseline.unwrap(), 290.0), "{:?}", a.total_baseline);
    assert_eq!(doc.project.assignments[0].baselines[0].cost, 290.0);
    for (series, whole) in [
        (a.total_scheduled + b.total_scheduled, all.total_scheduled),
        (a.total_baseline.unwrap() + b.total_baseline.unwrap(), all.total_baseline.unwrap()),
        (a.total_actual.unwrap() + b.total_actual.unwrap(), all.total_actual.unwrap()),
    ] {
        assert!(close(series, whole), "{series} vs {whole}");
    }
    // A raise after the baseline moves the scheduled cost, not the baseline.
    let mut raised = doc.clone();
    raised
        .apply(Command::UpdateResource {
            uid: alice,
            name: "Alice".into(),
            kind: ResourceKind::Person,
            max_units: 1.0,
            calendar_id: None,
            standard_rate: 30.0,
            overtime_rate: 0.0,
            cost_per_use: 50.0,
        })
        .unwrap();
    let after = curves(&raised.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Person { uid: alice }, Some(0)).unwrap();
    assert!(close(after.total_scheduled, 24.0 * 30.0 + 50.0));
    assert!(close(after.total_baseline.unwrap(), 290.0));
}

#[test]
fn a_baseline_without_assignment_baselines_is_read_from_the_task() {
    let (doc, alice, bob) = two_people_half_done();
    // As in a file baselined before engine revision 7: tasks hold the baseline, assignments none.
    let mut project = doc.project.clone();
    for assignment in &mut project.assignments {
        assignment.baselines.clear();
    }
    let work = |filter| curves(&project, CurveMeasure::Work, Bucket::Day, filter, Some(0)).unwrap();
    assert_eq!(work(CurveFilter::Project).total_baseline, Some(48.0));
    let a = work(CurveFilter::Person { uid: alice });
    assert!(close(a.total_baseline.unwrap(), 24.0), "{:?}", a.total_baseline);
    let b = work(CurveFilter::Person { uid: bob });
    assert!(close(b.total_baseline.unwrap(), 24.0));

    // Progress is measured against that baseline, so the scheduled curve ends at 100 %.
    let progress = curves(&project, CurveMeasure::Progress, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    assert!(close(progress.total_baseline.unwrap(), 100.0));
    assert!(close(progress.total_scheduled, 100.0), "{}", progress.total_scheduled);

    // With no saved cost, a person's baseline cost is the shared work at today's rate.
    let cost = curves(&project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Person { uid: alice }, Some(0)).unwrap();
    assert!(close(cost.total_baseline.unwrap(), 290.0), "{:?}", cost.total_baseline);
}

/// One person at 10/h on a four-day task (320 labour) with 200 fixed cost accruing at
/// `accrual`, baselined, half done on Wednesday noon (the status date).
fn labour_and_fixed(accrual: Accrual) -> Document {
    let mut doc = setup_base_document();
    doc.apply(Command::AddResource { name: "An".into(), kind: ResourceKind::Person }).unwrap();
    let person = doc.project.resources[0].uid;
    doc.apply(Command::UpdateResource {
        uid: person,
        name: "An".into(),
        kind: ResourceKind::Person,
        max_units: 1.0,
        calendar_id: None,
        standard_rate: 10.0,
        overtime_rate: 0.0,
        cost_per_use: 0.0,
    })
    .unwrap();
    doc.apply(Command::AddTask { name: "Lắp đặt".into(), after_uid: None }).unwrap();
    let task = doc.project.tasks[0].uid;
    doc.apply(Command::SetDuration { uid: task, minutes: 4 * DAY }).unwrap();
    doc.apply(Command::AssignResource { task_uid: task, resource_uid: person, units: 1.0 }).unwrap();
    doc.apply(Command::SetFixedCost { uid: task, cost: 200.0, accrual }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetStatusDate { date: Some(at(2, 12, 0)) }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: task, percent: 50.0 }).unwrap();
    doc
}

#[test]
fn accrual_moves_only_the_fixed_cost() {
    // End: labour is spent day by day (80 a day), the 200 fixed on the last day.
    let doc = labour_and_fixed(Accrual::End);
    let task = &doc.project.tasks[0];
    assert!(close(task.cost, 520.0));
    assert_eq!(task.baselines[0].fixed_cost, Some(200.0));
    // Half done: half the labour is spent, the fixed cost not yet.
    assert!(close(task.actual_cost, 160.0), "{}", task.actual_cost);
    let rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    let scheduled: Vec<f64> = rep.points.iter().map(|p| p.scheduled_period).collect();
    let baseline: Vec<f64> = rep.points.iter().map(|p| p.baseline_period.unwrap()).collect();
    for periods in [&scheduled, &baseline] {
        assert!(periods.iter().zip([80.0, 80.0, 80.0, 280.0]).all(|(a, b)| close(*a, b)), "{periods:?}");
    }
    assert!(close(rep.total_actual.unwrap(), 160.0));

    // Start: the fixed cost lands on the first day, planned and actual; labour as before.
    let doc = labour_and_fixed(Accrual::Start);
    assert!(close(doc.project.tasks[0].actual_cost, 360.0));
    let rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    let first = &rep.points[0];
    assert!(close(first.scheduled_period, 280.0) && close(first.baseline_period.unwrap(), 280.0));
    assert!(close(first.actual_period.unwrap(), 280.0), "{:?}", first.actual_period);
    assert!(close(rep.total_actual.unwrap(), 360.0));
}

#[test]
fn evm_splits_fixed_cost_from_labour_and_matches_the_curve() {
    let doc = labour_and_fixed(Accrual::End);
    // Tuesday 17:00: two of four baseline days. Labour 160 is planned, the fixed cost not yet.
    assert!(close(evm::evm_at(&doc.project, at(1, 17, 0)).pv, 160.0));
    assert!(close(evm::evm_at(&doc.project, at(3, 17, 0)).pv, 520.0));
    // Wednesday 12:00 (status): AC is the labour done, 160; nothing fixed until the finish.
    let m = doc.metrics().evm.metrics.unwrap();
    assert!(close(m.ac, 160.0), "{}", m.ac);
    assert!(close(m.pv, 200.0), "{}", m.pv); // 2.5 of 4 days of labour
    // The S-curve's cumulative PV and AC reach the same numbers on the status day.
    let rep = curves(&doc.project, CurveMeasure::Cost, Bucket::Day, CurveFilter::Project, Some(0)).unwrap();
    let wed = rep.points.iter().find(|p| p.label == "2026-03-04").unwrap();
    assert!(close(wed.actual_cumulative.unwrap(), m.ac));
    assert!(close(wed.baseline_cumulative.unwrap(), 240.0)); // whole Wednesday planned
}

#[test]
fn a_baseline_saved_before_its_fixed_part_was_kept_uses_the_task_fixed_cost() {
    let doc = labour_and_fixed(Accrual::End);
    let mut project = doc.project.clone();
    project.tasks[0].baselines[0].fixed_cost = None;
    assert!(close(project.tasks[0].baselines[0].fixed_part(&project.tasks[0]), 200.0));
    assert!(close(evm::evm_at(&project, at(1, 17, 0)).pv, 160.0));
    // Older files have no such field at all.
    let json = r#"{"number":0,"start":0,"finish":0,"work":0,"cost":50.0}"#;
    let old: sheafly_engine::model::Baseline = serde_json::from_str(json).unwrap();
    assert_eq!(old.fixed_cost, None);
    // A fixed cost raised above the old baseline cost is capped by it.
    project.tasks[0].fixed_cost = 9_999.0;
    assert!(close(project.tasks[0].baselines[0].fixed_part(&project.tasks[0]), 520.0));
}
