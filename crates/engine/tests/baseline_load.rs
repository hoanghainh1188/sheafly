use sheafly_engine::baseline_load::{
    assignment_baseline_days, baseline_load_by_resource, project_baseline_load_by_day,
    project_baseline_load_in_range, resource_baseline_load_by_day, resource_baseline_load_in_range,
};
use sheafly_engine::checkpoint::ENGINE_REVISION;
use sheafly_engine::command::Command;
use sheafly_engine::document::Document;
use sheafly_engine::model::ResourceKind;
use sheafly_engine::mspdi::{read_mspdi, write_mspdi};

#[test]
#[allow(clippy::assertions_on_constants)]
fn engine_revision_is_at_least_the_one_that_added_assignment_baselines() {
    // Checkpoints saved before revision 7 hold no assignment baselines, so they must not be reused.
    assert!(ENGINE_REVISION >= 7);
}

#[test]
fn set_baseline_records_assignment_baselines_and_clear_removes_them() {
    let mut doc = Document::new_default();

    // 1. Add resource and task
    doc.apply(Command::AddResource {
        name: "An".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();
    doc.apply(Command::AddTask {
        name: "Phân tích".into(),
        after_uid: None,
    })
    .unwrap();

    // 2. Assign resource (1.0 units, 480 min work = 8h)
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();

    assert_eq!(doc.project.assignments.len(), 1);
    assert_eq!(doc.project.assignments[0].work, 480);
    assert!(doc.project.assignments[0].baselines.is_empty());

    // 3. Set Baseline 0
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    assert_eq!(doc.project.tasks[0].baselines.len(), 1);
    assert_eq!(doc.project.tasks[0].baselines[0].number, 0);
    assert_eq!(doc.project.tasks[0].baselines[0].work, 480);

    assert_eq!(doc.project.assignments[0].baselines.len(), 1);
    assert_eq!(doc.project.assignments[0].baselines[0].number, 0);
    assert_eq!(doc.project.assignments[0].baselines[0].work, 480);

    // 4. Update task duration in current schedule
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 960,
    })
    .unwrap();

    // Current assignment work changed to 960 min
    assert_eq!(doc.project.assignments[0].work, 960);
    // But baseline work remains locked at 480 min
    assert_eq!(doc.project.assignments[0].baselines[0].work, 480);

    // 5. Clear Baseline 0
    doc.apply(Command::ClearBaseline { number: 0 }).unwrap();
    assert!(doc.project.tasks[0].baselines.is_empty());
    assert!(doc.project.assignments[0].baselines.is_empty());
}

#[test]
fn daily_baseline_load_without_splits_distributes_proportionally() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "An".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();

    // Task takes 3 working days = 1440 min
    doc.apply(Command::AddTask {
        name: "Phát triển".into(),
        after_uid: None,
    })
    .unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 1440,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();

    // Set baseline 0
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    let task = &doc.project.tasks[0];
    let assignment = &doc.project.assignments[0];

    // Derive daily load from baseline 0
    let days = assignment_baseline_days(&doc.project, task, assignment, 0);
    assert_eq!(days.len(), 3);

    let (d1, _) = sheafly_engine::timeutil::civil(task.start);
    let d2 = d1.succ_opt().unwrap();
    let d3 = d2.succ_opt().unwrap();

    assert_eq!(days[0], (d1, 480));
    assert_eq!(days[1], (d2, 480));
    assert_eq!(days[2], (d3, 480));

    let sum_work: i64 = days.iter().map(|(_, w)| *w).sum();
    assert_eq!(sum_work, 1440);

    // Check resource-level query
    let res_days = resource_baseline_load_by_day(&doc.project, 1, 0);
    assert_eq!(res_days.get(&d1), Some(&480));
    assert_eq!(res_days.get(&d2), Some(&480));
    assert_eq!(res_days.get(&d3), Some(&480));

    let range_work = resource_baseline_load_in_range(&doc.project, 1, 0, d1, d2);
    assert_eq!(range_work, 960);

    let total_proj = project_baseline_load_in_range(&doc.project, 0, d1, d3);
    assert_eq!(total_proj, 1440);

    // Even if current task duration or start changes, baseline load stays at original dates
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 2400,
    })
    .unwrap();

    let moved_days = resource_baseline_load_by_day(&doc.project, 1, 0);
    assert_eq!(moved_days.get(&d1), Some(&480));
    assert_eq!(moved_days.get(&d2), Some(&480));
    assert_eq!(moved_days.get(&d3), Some(&480));
}

#[test]
fn daily_baseline_load_with_split_segments_skips_gaps() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "Bình".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();

    // 2-day task (960 min)
    doc.apply(Command::AddTask {
        name: "Kiểm thử".into(),
        after_uid: None,
    })
    .unwrap();
    doc.apply(Command::SetDuration {
        uid: 1,
        minutes: 960,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();

    // Split task: 4 hours on day 1 (240 min), gap of 1 working day (480 min), remaining on day 3
    let cal = doc.project.task_calendar(&doc.project.tasks[0]);
    let split_at = cal.add_working(doc.project.tasks[0].start, 240);
    doc.apply(Command::SplitTask {
        uid: 1,
        at: split_at,
        gap: 480,
    })
    .unwrap();

    assert_eq!(doc.project.tasks[0].segments.len(), 2);

    // Save baseline with split
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    assert_eq!(doc.project.tasks[0].baselines[0].segments.len(), 2);

    let (d1, _) = sheafly_engine::timeutil::civil(doc.project.tasks[0].start);
    let d2 = d1.succ_opt().unwrap();
    let d3 = d2.succ_opt().unwrap();

    let days = resource_baseline_load_by_day(&doc.project, 1, 0);
    // Day 1 (morning 4h = 240m), gap covers Mon afternoon + Tue morning (480m),
    // Day 2 (afternoon 4h = 240m), Day 3 (remaining 8h = 480m)
    assert_eq!(days.get(&d1), Some(&240));
    assert_eq!(days.get(&d2), Some(&240));
    assert_eq!(days.get(&d3), Some(&480));

    let total: i64 = days.values().sum();
    assert_eq!(total, 960);
}

#[test]
fn mspdi_round_trip_preserves_assignment_baselines() {
    let mut doc = Document::new_default();

    doc.apply(Command::AddResource {
        name: "Chi".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();
    doc.apply(Command::AddTask {
        name: "Triển khai".into(),
        after_uid: None,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();

    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    // Progress writes actual work as timephased data, which must follow the baseline.
    doc.apply(Command::SetPercentComplete { uid: 1, percent: 50.0 }).unwrap();

    // Export to MSPDI XML
    let xml = write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<Assignment>"));
    assert!(xml.contains("<Baseline>"));
    assert!(xml.contains("<Number>0</Number>"));
    // The schema puts an assignment's baselines before its timephased data.
    let assignment_xml = &xml[xml.find("<Assignment>").unwrap()..xml.find("</Assignment>").unwrap()];
    let baseline_at = assignment_xml.find("<Baseline>").expect("assignment baseline");
    let timephased_at = assignment_xml.find("<TimephasedData>").expect("actual work");
    assert!(baseline_at < timephased_at, "{assignment_xml}");

    // Import from MSPDI XML
    let imported = read_mspdi(&xml).unwrap();
    assert_eq!(imported.assignments.len(), 1);
    assert_eq!(imported.assignments[0].baselines.len(), 1);
    assert_eq!(imported.assignments[0].baselines[0].number, 0);
    assert_eq!(imported.assignments[0].baselines[0].work, 480);
}

fn one_person_on_one_task(minutes: i64) -> Document {
    let mut doc = Document::new_default();
    doc.apply(Command::AddResource {
        name: "Chi".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();
    doc.apply(Command::AddTask {
        name: "Triển khai".into(),
        after_uid: None,
    })
    .unwrap();
    doc.apply(Command::AssignResource {
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
    })
    .unwrap();
    doc.apply(Command::SetDuration { uid: 1, minutes }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc
}

#[test]
fn a_baseline_longer_than_ten_years_keeps_all_its_work() {
    // Eleven years of working days.
    let doc = one_person_on_one_task(11 * 261 * 480);
    let assignment = &doc.project.assignments[0];
    let days = assignment_baseline_days(&doc.project, &doc.project.tasks[0], assignment, 0);
    let span = days.last().unwrap().0 - days.first().unwrap().0;
    assert!(span.num_days() > 3660, "{span}");
    assert_eq!(days.iter().map(|(_, w)| *w).sum::<i64>(), assignment.baselines[0].work);
}

#[test]
fn the_load_of_every_resource_matches_asking_for_each_one() {
    let doc = one_person_on_one_task(3 * 480);
    let all = baseline_load_by_resource(&doc.project, 0);
    assert_eq!(all.len(), 1);
    assert_eq!(all[&1], resource_baseline_load_by_day(&doc.project, 1, 0));
    assert_eq!(all[&1], project_baseline_load_by_day(&doc.project, 0));
    assert!(baseline_load_by_resource(&doc.project, 1).is_empty());
}
