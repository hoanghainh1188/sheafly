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
