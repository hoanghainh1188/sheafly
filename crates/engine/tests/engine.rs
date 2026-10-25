use chrono::Datelike;

use sheafly_engine::command::{parse_predecessor_text, Command};
use sheafly_engine::evm::{burndown, evm, evm_at, BurndownPoint, BurndownSnapshot};
use sheafly_engine::level::level_project;
use sheafly_engine::model::{
    Accrual, Baseline, ConstraintType, DepKind, Dependency, ResourceKind, Task, TaskType,
};
use sheafly_engine::mspdi::{read_mspdi, write_mspdi};
use sheafly_engine::recurrence::{
    occurrence_dates, RecurrenceEnd, RecurrencePattern, RecurrenceSpec,
};
use sheafly_engine::schedule::schedule;
use sheafly_engine::store::{load, save, save_with_snapshot};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::work::{self, Changed};
use sheafly_engine::{Calendar, Document, Project};

fn monday() -> i64 {
    timestamp(2026, 1, 5, 8, 0)
}

fn at(day_offset: u32, hour: u32, minute: u32) -> i64 {
    // 2026-01-05 is Monday. day_offset 0 = Monday.
    let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(day_offset as i64))
        .unwrap();
    timestamp(date.year(), date.month(), date.day(), hour, minute)
}

fn project_with(tasks: Vec<(&str, i64)>, deps: Vec<(u32, u32, DepKind, i64)>) -> Project {
    let mut project = Project::new("Kiểm thử", monday());
    for (index, (name, duration)) in tasks.into_iter().enumerate() {
        let mut task = Task::new((index as u32) + 1, name);
        task.duration = duration;
        task.work = duration;
        task.milestone = duration == 0;
        task.remaining_duration = duration;
        task.remaining_work = duration;
        project.tasks.push(task);
    }
    project.next_task_uid = project.tasks.len() as u32 + 1;
    for (index, (pred, succ, kind, lag)) in deps.into_iter().enumerate() {
        project.dependencies.push(Dependency {
            uid: (index as u32) + 1,
            predecessor_uid: pred,
            successor_uid: succ,
            kind,
            lag,
        });
    }
    project.next_dependency_uid = project.dependencies.len() as u32 + 1;
    schedule(&mut project).expect("schedule");
    project
}

#[test]
fn fs_chain_is_critical() {
    let project = project_with(
        vec![("A", 960), ("B", 480), ("C", 480)],
        vec![(1, 2, DepKind::Fs, 0), (2, 3, DepKind::Fs, 0)],
    );
    let a = &project.tasks[0];
    let b = &project.tasks[1];
    let c = &project.tasks[2];
    assert_eq!(a.start, at(0, 8, 0));
    assert_eq!(a.finish, at(1, 17, 0));
    assert_eq!(b.start, at(2, 8, 0));
    assert_eq!(b.finish, at(2, 17, 0));
    assert_eq!(c.start, at(3, 8, 0));
    assert_eq!(c.finish, at(3, 17, 0));
    assert!(a.critical && b.critical && c.critical);
    assert_eq!(a.total_slack, 0);
    assert_eq!(b.predecessors, "1");
    assert_eq!(c.predecessors, "2");
}

#[test]
fn lag_crosses_a_holiday() {
    let mut project = Project::new("Nghỉ", monday());
    project
        .calendar_mut()
        .set_day_off(chrono::NaiveDate::from_ymd_opt(2026, 1, 6).unwrap());
    let mut a = Task::new(1, "A");
    a.duration = 480;
    a.work = 480;
    let mut b = Task::new(2, "B");
    b.duration = 480;
    b.work = 480;
    project.tasks = vec![a, b];
    project.next_task_uid = 3;
    project.dependencies.push(Dependency {
        uid: 1,
        predecessor_uid: 1,
        successor_uid: 2,
        kind: DepKind::Fs,
        lag: 480,
    });
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].finish, at(0, 17, 0));
    // Tuesday is a holiday, so the one-day lag lands on Wednesday and B starts Thursday.
    assert_eq!(project.tasks[1].start, at(3, 8, 0));
}

#[test]
fn ss_ff_and_sf() {
    // A is held until Wednesday so SS, FF, and SF are each the binding constraint.
    let mut project = Project::new("Liên kết", monday());
    let mut anchor = Task::new(1, "A");
    anchor.duration = 480;
    anchor.work = 480;
    anchor.constraint_type = ConstraintType::Snet;
    anchor.constraint_date = Some(at(2, 8, 0));
    let mut ss = Task::new(2, "SS");
    ss.duration = 480;
    ss.work = 480;
    let mut ff = Task::new(3, "FF");
    ff.duration = 480;
    ff.work = 480;
    let mut sf = Task::new(4, "SF");
    sf.duration = 480;
    sf.work = 480;
    project.tasks = vec![anchor, ss, ff, sf];
    project.next_task_uid = 5;
    project.dependencies = vec![
        Dependency {
            uid: 1,
            predecessor_uid: 1,
            successor_uid: 2,
            kind: DepKind::Ss,
            lag: 480,
        },
        Dependency {
            uid: 2,
            predecessor_uid: 1,
            successor_uid: 3,
            kind: DepKind::Ff,
            lag: 0,
        },
        Dependency {
            uid: 3,
            predecessor_uid: 1,
            successor_uid: 4,
            kind: DepKind::Sf,
            lag: 0,
        },
    ];
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].start, at(2, 8, 0));
    assert_eq!(project.tasks[0].finish, at(2, 17, 0));
    assert_eq!(project.tasks[1].start, at(3, 8, 0), "SS + 1 ngày");
    assert_eq!(project.tasks[2].finish, at(2, 17, 0), "FF");
    assert_eq!(project.tasks[2].start, at(2, 8, 0));
    assert_eq!(project.tasks[3].start, at(1, 8, 0), "SF");
    assert_eq!(project.tasks[3].finish, at(2, 8, 0));
}

#[test]
fn summary_rolls_up_children() {
    let mut project = Project::new("WBS", monday());
    let mut summary = Task::new(1, "Tóm tắt");
    summary.outline_level = 1;
    let mut a = Task::new(2, "A");
    a.outline_level = 2;
    a.duration = 960;
    a.work = 960;
    let mut b = Task::new(3, "B");
    b.outline_level = 2;
    b.duration = 480;
    b.work = 480;
    project.tasks = vec![summary, a, b];
    project.next_task_uid = 4;
    project.dependencies.push(Dependency {
        uid: 1,
        predecessor_uid: 2,
        successor_uid: 3,
        kind: DepKind::Fs,
        lag: 0,
    });
    schedule(&mut project).unwrap();
    let summary = &project.tasks[0];
    assert!(summary.summary);
    assert_eq!(summary.wbs, "1");
    assert_eq!(project.tasks[1].wbs, "1.1");
    assert_eq!(project.tasks[2].wbs, "1.2");
    assert_eq!(summary.start, project.tasks[1].start);
    assert_eq!(summary.finish, project.tasks[2].finish);
    assert_eq!(summary.duration, 1440);
    assert!(summary.critical);
}

#[test]
fn dependency_cycle_is_an_error() {
    let mut project = project_with(
        vec![("A", 480), ("B", 480)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    project.dependencies.push(Dependency {
        uid: 2,
        predecessor_uid: 2,
        successor_uid: 1,
        kind: DepKind::Fs,
        lag: 0,
    });
    let error = schedule(&mut project).unwrap_err();
    assert!(matches!(error, sheafly_engine::EngineError::DependencyCycle));
}

#[test]
fn milestone_has_zero_duration() {
    let project = project_with(
        vec![("Mốc", 0), ("Sau", 480)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    assert!(project.tasks[0].milestone);
    assert_eq!(project.tasks[0].start, project.tasks[0].finish);
    assert_eq!(project.tasks[0].start, at(0, 8, 0));
    assert_eq!(project.tasks[1].start, at(0, 8, 0));
}

#[test]
fn snet_constraint_holds_the_start() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    project.tasks[0].constraint_type = ConstraintType::Snet;
    project.tasks[0].constraint_date = Some(at(2, 8, 0));
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].start, at(2, 8, 0));
    assert_eq!(project.tasks[0].finish, at(2, 17, 0));
}

#[test]
fn remaining_constraints_are_honored() {
    let asap = project_with(vec![("A", 480)], vec![]);
    assert_eq!(asap.tasks[0].start, at(0, 8, 0));

    let mut alap = project_with(vec![("Dài", 1440), ("Muộn", 480)], vec![]);
    alap.tasks[1].constraint_type = ConstraintType::Alap;
    schedule(&mut alap).unwrap();
    assert_eq!(alap.tasks[1].start, at(2, 8, 0));
    assert_eq!(alap.tasks[1].finish, at(2, 17, 0));

    let mut mso = project_with(vec![("Ghim", 480)], vec![]);
    mso.tasks[0].constraint_type = ConstraintType::Mso;
    mso.tasks[0].constraint_date = Some(at(2, 10, 0));
    schedule(&mut mso).unwrap();
    assert_eq!(mso.tasks[0].start, at(2, 10, 0));

    let mut mfo = project_with(vec![("Xong", 480)], vec![]);
    mfo.tasks[0].constraint_type = ConstraintType::Mfo;
    mfo.tasks[0].constraint_date = Some(at(2, 17, 0));
    schedule(&mut mfo).unwrap();
    assert_eq!(mfo.tasks[0].finish, at(2, 17, 0));
    assert_eq!(mfo.tasks[0].start, at(2, 8, 0));

    let mut fnet = project_with(vec![("F", 480)], vec![]);
    fnet.tasks[0].constraint_type = ConstraintType::Fnet;
    fnet.tasks[0].constraint_date = Some(at(3, 17, 0));
    schedule(&mut fnet).unwrap();
    assert!(fnet.tasks[0].finish >= at(3, 17, 0));

    let mut snlt = project_with(
        vec![("A", 1440), ("B", 480)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    snlt.tasks[1].constraint_type = ConstraintType::Snlt;
    snlt.tasks[1].constraint_date = Some(at(0, 8, 0));
    schedule(&mut snlt).unwrap();
    assert!(snlt.tasks[1].total_slack < 0 || snlt.tasks[1].conflict);

    let mut fnlt = project_with(vec![("A", 1440)], vec![]);
    fnlt.tasks[0].constraint_type = ConstraintType::Fnlt;
    fnlt.tasks[0].constraint_date = Some(at(0, 12, 0));
    schedule(&mut fnlt).unwrap();
    assert!(fnlt.tasks[0].conflict || fnlt.tasks[0].total_slack < 0);
}

#[test]
fn alap_with_flexible_successor_stays_early() {
    let mut project = project_with(
        vec![("Muộn", 480), ("Kế", 480), ("Dài", 2400)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    project.tasks[0].constraint_type = ConstraintType::Alap;
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].start, at(0, 8, 0));
    assert_eq!(project.tasks[0].finish, at(0, 17, 0));
    assert_eq!(project.tasks[1].start, at(1, 8, 0));
    assert_eq!(project.tasks[2].finish, at(4, 17, 0));
}

#[test]
fn alap_without_successor_sits_at_project_end() {
    let mut project = project_with(vec![("Dài", 1440), ("Muộn", 480)], vec![]);
    project.tasks[1].constraint_type = ConstraintType::Alap;
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[1].start, at(2, 8, 0));
    assert_eq!(project.tasks[1].finish, at(2, 17, 0));
    assert_eq!(project.tasks[0].start, at(0, 8, 0));
}

#[test]
fn alap_chain_slides_together() {
    let mut project = project_with(
        vec![("Muộn", 480), ("Cũng muộn", 480), ("Dài", 2400)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    project.tasks[0].constraint_type = ConstraintType::Alap;
    project.tasks[1].constraint_type = ConstraintType::Alap;
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[2].start, at(0, 8, 0));
    assert_eq!(project.tasks[2].finish, at(4, 17, 0));
    assert_eq!(project.tasks[1].start, at(4, 8, 0));
    assert_eq!(project.tasks[1].finish, at(4, 17, 0));
    assert_eq!(project.tasks[0].start, at(3, 8, 0));
    assert_eq!(project.tasks[0].finish, at(3, 17, 0));
}

#[test]
fn successor_constraint_limits_alap_predecessor() {
    let mut snet = project_with(
        vec![("Muộn", 480), ("Ghim", 480), ("Dài", 2400)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    snet.tasks[0].constraint_type = ConstraintType::Alap;
    snet.tasks[1].constraint_type = ConstraintType::Snet;
    snet.tasks[1].constraint_date = Some(at(2, 8, 0));
    schedule(&mut snet).unwrap();
    assert_eq!(snet.tasks[1].start, at(2, 8, 0));
    assert_eq!(snet.tasks[0].start, at(1, 8, 0));
    assert_eq!(snet.tasks[0].finish, at(1, 17, 0));

    let mut mso = project_with(
        vec![("Muộn", 480), ("Ghim", 480), ("Dài", 2400)],
        vec![(1, 2, DepKind::Fs, 0)],
    );
    mso.tasks[0].constraint_type = ConstraintType::Alap;
    mso.tasks[1].constraint_type = ConstraintType::Mso;
    mso.tasks[1].constraint_date = Some(at(2, 10, 0));
    schedule(&mut mso).unwrap();
    assert_eq!(mso.tasks[1].start, at(2, 10, 0));
    assert_eq!(mso.tasks[0].finish, at(2, 10, 0));
    assert!(mso.tasks[0].start < mso.tasks[1].start);
    assert!(mso.tasks[0].start > at(0, 8, 0));
}

#[test]
fn tracking_edits_undo() {
    let mut doc = Document::from_project(Project::new("Theo dõi", monday()));
    let uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetActualStart {
        uid,
        start: Some(at(1, 9, 0)),
    })
    .unwrap();
    assert_eq!(doc.project.tasks[0].actual_start, Some(at(1, 9, 0)));
    doc.apply(Command::SetActualFinish {
        uid,
        finish: Some(at(1, 17, 0)),
    })
    .unwrap();
    assert_eq!(doc.project.tasks[0].actual_finish, Some(at(1, 17, 0)));
    doc.apply(Command::SetRemainingWork { uid, minutes: 120 }).unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.project.tasks[0].actual_start, None);
    assert_eq!(doc.project.tasks[0].actual_finish, None);

    let calendar_id = doc.project.calendar_id;
    doc.apply(Command::SetTaskCalendar {
        uid,
        calendar_id: Some(calendar_id),
    })
    .unwrap();
    assert_eq!(doc.project.tasks[0].calendar_id, Some(calendar_id));
    doc.undo().unwrap();
    assert_eq!(doc.project.tasks[0].calendar_id, None);

    doc.apply(Command::AddResource {
        name: "An".into(),
        kind: ResourceKind::Person,
    })
    .unwrap();
    let resource_uid = doc.project.resources[0].uid;
    doc.apply(Command::AssignResource {
        task_uid: uid,
        resource_uid,
        units: 1.0,
    })
    .unwrap();
    doc.apply(Command::SetOvertimeWork { uid, minutes: 120 }).unwrap();
    assert_eq!(doc.project.assignments[0].overtime_work, 120);
    doc.undo().unwrap();
    assert_eq!(doc.project.assignments[0].overtime_work, 0);
    doc.redo().unwrap();
    assert_eq!(doc.project.assignments[0].overtime_work, 120);
}

#[test]
fn manual_task_stays_pinned() {
    let mut project = project_with(vec![("Tay", 480), ("Sau", 480)], vec![(1, 2, DepKind::Fs, 0)]);
    project.tasks[0].manual = true;
    project.tasks[0].start = at(1, 8, 0);
    project.tasks[0].finish = at(1, 17, 0);
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].start, at(1, 8, 0));
    assert_eq!(project.tasks[1].start, at(2, 8, 0));
}

#[test]
fn task_type_recomputes_duration_units_and_work() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    assert_eq!(project.tasks[0].work, 480);

    project.tasks[0].duration = 960;
    work::apply_change(&mut project, 1, Changed::Duration);
    assert_eq!(project.tasks[0].work, 960);
    assert!((project.tasks[0].units - 1.0).abs() < 0.001);

    project.tasks[0].work = 480;
    work::apply_change(&mut project, 1, Changed::Work);
    assert_eq!(project.tasks[0].duration, 480);

    project.tasks[0].units = 2.0;
    project.tasks[0].effort_driven = true;
    project.tasks[0].task_type = TaskType::FixedUnits;
    work::apply_change(&mut project, 1, Changed::Units);
    assert_eq!(project.tasks[0].duration, 240);
    assert_eq!(project.tasks[0].work, 480);

    project.tasks[0].task_type = TaskType::FixedDuration;
    project.tasks[0].duration = 480;
    project.tasks[0].units = 2.0;
    work::apply_change(&mut project, 1, Changed::Units);
    assert_eq!(project.tasks[0].duration, 480);
    assert_eq!(project.tasks[0].work, 960);

    project.tasks[0].work = 480;
    work::apply_change(&mut project, 1, Changed::Work);
    assert_eq!(project.tasks[0].duration, 480);
    assert!((project.tasks[0].units - 1.0).abs() < 0.001);

    project.tasks[0].task_type = TaskType::FixedWork;
    project.tasks[0].work = 480;
    project.tasks[0].units = 1.0;
    project.tasks[0].duration = 960;
    work::apply_change(&mut project, 1, Changed::Duration);
    assert!((project.tasks[0].units - 0.5).abs() < 0.001);
    assert_eq!(project.tasks[0].work, 480);

    project.tasks[0].units = 2.0;
    work::apply_change(&mut project, 1, Changed::Units);
    assert_eq!(project.tasks[0].duration, 240);
}

#[test]
fn tracking_baseline_and_cost() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    let uid = project.alloc_resource_uid();
    let mut resource = sheafly_engine::Resource::new(uid, "An", ResourceKind::Person);
    resource.standard_rate = 10.0;
    resource.overtime_rate = 20.0;
    resource.cost_per_use = 5.0;
    project.resources.push(resource);
    work::add_assignment(&mut project, 1, uid, 1.0);
    project.tasks[0].fixed_cost = 20.0;
    project.tasks[0].fixed_cost_accrual = Accrual::Prorated;
    project.tasks[0].percent_complete = 50.0;
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].remaining_duration, 240);
    assert_eq!(project.tasks[0].work, 480);
    // 8h * 10 + 5 per use + 20 fixed = 105. Half is actual.
    assert!((project.tasks[0].cost - 105.0).abs() < 0.01);
    assert!((project.tasks[0].actual_cost - 52.5).abs() < 0.01);

    // The accrual is the fixed cost's only: labour (85) is half spent at 50 % whatever the
    // accrual, and the 20 fixed is spent once started (start) or once finished (end).
    project.tasks[0].fixed_cost_accrual = Accrual::Start;
    schedule(&mut project).unwrap();
    assert!((project.tasks[0].actual_cost - 62.5).abs() < 0.01);
    project.tasks[0].fixed_cost_accrual = Accrual::End;
    project.tasks[0].percent_complete = 50.0;
    schedule(&mut project).unwrap();
    assert!((project.tasks[0].actual_cost - 42.5).abs() < 0.01);
    assert!((project.assignments[0].actual_cost - 42.5).abs() < 0.01);

    project.tasks[0].percent_complete = 100.0;
    schedule(&mut project).unwrap();
    let start = project.tasks[0].start;
    let finish = project.tasks[0].finish;
    let mut doc = Document::from_project(project);
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetBaseline { number: 10 }).unwrap();
    let saved = doc.project.tasks[0]
        .baselines
        .iter()
        .find(|baseline| baseline.number == 0)
        .unwrap();
    assert_eq!(saved.start, start);
    assert_eq!(saved.finish, finish);
    // The baseline keeps its fixed-cost part beside the total.
    assert_eq!(saved.fixed_cost, Some(20.0));
    assert!((saved.cost - 105.0).abs() < 0.01);
    assert!(doc.project.tasks[0].baselines.iter().any(|b| b.number == 10));
}

#[test]
fn overallocation_and_leveling_delay() {
    let mut project = project_with(vec![("A", 960), ("B", 960)], vec![]);
    project.tasks[1].priority = 100;
    let resource = project.alloc_resource_uid();
    project.resources.push(sheafly_engine::Resource::new(
        resource,
        "Máy cắt",
        ResourceKind::Machine,
    ));
    work::add_assignment(&mut project, 1, resource, 1.0);
    work::add_assignment(&mut project, 2, resource, 1.0);
    schedule(&mut project).unwrap();
    assert!(!project.overallocations.is_empty());
    level_project(&mut project).unwrap();
    let delayed = project.tasks.iter().find(|task| task.uid == 2).unwrap();
    assert!(delayed.leveling_delay > 0);
    assert!(delayed.start > project.tasks[0].start);
    assert!(project.overallocations.is_empty());
}

#[test]
fn undo_redo_and_sqlite_roundtrip() {
    let mut doc = Document::from_project(Project::new("Tệp", monday()));
    let before = doc.project.tasks.len();
    doc.apply(Command::AddTask {
        name: "Thêm".into(),
        after_uid: None,
    })
    .unwrap();
    assert_eq!(doc.project.tasks.len(), before + 1);
    assert_eq!(doc.snapshot().history_cursor, 1);
    doc.undo().unwrap();
    assert_eq!(doc.project.tasks.len(), before);
    assert_eq!(doc.snapshot().history_cursor, 0);
    doc.redo().unwrap();
    assert_eq!(doc.project.tasks.len(), before + 1);
    assert_eq!(doc.snapshot().history_cursor, 1);

    let path = std::env::temp_dir().join(format!("sheafly-test-{}.pdproj", std::process::id()));
    let path = path.to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.project.tasks.len(), doc.project.tasks.len());
    assert_eq!(loaded.project.tasks.last().unwrap().name, "Thêm");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn mspdi_roundtrip_covers_phase_fields() {
    let mut project = project_with(
        vec![("Thiết kế", 960), ("Xây", 480)],
        vec![(1, 2, DepKind::Fs, 480)],
    );
    project
        .calendar_mut()
        .set_day_off(chrono::NaiveDate::from_ymd_opt(2026, 1, 7).unwrap());
    project.tasks[0].notes = "Ghi chú <A&B>".to_string();
    project.tasks[0].priority = 700;
    project.tasks[0].constraint_type = ConstraintType::Snet;
    project.tasks[0].constraint_date = Some(at(0, 8, 0));
    project.tasks[1].task_type = TaskType::FixedDuration;
    project.tasks[1].percent_complete = 25.0;
    let resource_uid = project.alloc_resource_uid();
    let mut resource = sheafly_engine::Resource::new(resource_uid, "Bình", ResourceKind::Person);
    resource.standard_rate = 12.0;
    resource.max_units = 1.0;
    project.resources.push(resource);
    work::add_assignment(&mut project, 2, resource_uid, 1.0);
    schedule(&mut project).unwrap();
    let baseline = {
        let task = &project.tasks[0];
        sheafly_engine::model::Baseline {
            number: 0,
            start: task.start,
            finish: task.finish,
            work: task.work,
            cost: task.cost,
            segments: Vec::new(),
            fixed_cost: None,
        }
    };
    project.tasks[0].baselines.push(baseline);
    let xml = write_mspdi(&project).unwrap();
    assert!(xml.contains("http://schemas.microsoft.com/project"));
    let imported = read_mspdi(&xml).unwrap();
    assert_eq!(imported.tasks.len(), 2);
    assert_eq!(imported.tasks[0].name, "Thiết kế");
    assert!(imported.tasks[1].predecessors.contains("1FS"));
    assert!(imported.calendar().exceptions.contains_key("2026-01-07"));
    assert_eq!(imported.resources.len(), 1);
    assert_eq!(imported.resources[0].name, "Bình");
    assert_eq!(imported.assignments.len(), 1);
    assert!(imported.tasks[0].baselines.iter().any(|b| b.number == 0));
    assert_eq!(imported.tasks[0].notes, "Ghi chú <A&B>");
}

#[test]
fn effort_driven_assignment_keeps_work() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    let task = project.tasks[0].uid;
    let first = project.alloc_resource_uid();
    let second = project.alloc_resource_uid();
    project.resources.push(sheafly_engine::Resource::new(
        first,
        "An",
        ResourceKind::Person,
    ));
    project.resources.push(sheafly_engine::Resource::new(
        second,
        "Bình",
        ResourceKind::Person,
    ));
    project.tasks[0].task_type = TaskType::FixedUnits;
    project.tasks[0].effort_driven = true;
    work::add_assignment(&mut project, task, first, 1.0);
    work::add_assignment(&mut project, task, second, 1.0);
    schedule(&mut project).unwrap();
    assert_eq!(project.tasks[0].duration, 240);
    assert_eq!(project.tasks[0].work, 480);
    let extra = project
        .assignments
        .iter()
        .find(|assignment| assignment.resource_uid == second)
        .unwrap()
        .uid;
    assert!(work::remove_assignment(&mut project, extra));
    schedule(&mut project).unwrap();
    assert_eq!(project.assignments.len(), 1);
    assert_eq!(project.tasks[0].duration, 480);
    assert_eq!(project.tasks[0].work, 480);
    assert_eq!(project.assignments[0].work, 480);

    let mut fixed = project_with(vec![("B", 480)], vec![]);
    let task = fixed.tasks[0].uid;
    fixed.tasks[0].task_type = TaskType::FixedDuration;
    fixed.tasks[0].effort_driven = true;
    let first = fixed.alloc_resource_uid();
    let second = fixed.alloc_resource_uid();
    fixed.resources.push(sheafly_engine::Resource::new(
        first,
        "Máy",
        ResourceKind::Machine,
    ));
    fixed.resources.push(sheafly_engine::Resource::new(
        second,
        "Máy 2",
        ResourceKind::Machine,
    ));
    work::add_assignment(&mut fixed, task, first, 1.0);
    work::add_assignment(&mut fixed, task, second, 1.0);
    schedule(&mut fixed).unwrap();
    assert_eq!(fixed.tasks[0].duration, 480);
    assert_eq!(fixed.tasks[0].work, 480);
    assert!(fixed
        .assignments
        .iter()
        .all(|assignment| (assignment.units - 0.5).abs() < 0.001));

    fixed.tasks[0].effort_driven = false;
    let third = fixed.alloc_resource_uid();
    fixed.resources.push(sheafly_engine::Resource::new(
        third,
        "Máy 3",
        ResourceKind::Machine,
    ));
    work::add_assignment(&mut fixed, task, third, 1.0);
    schedule(&mut fixed).unwrap();
    assert_eq!(fixed.tasks[0].duration, 480);
    assert_eq!(fixed.tasks[0].work, 960);
}

#[test]
fn summary_fixed_cost_includes_children_after_one_pass() {
    let mut project = Project::new("Chi phí", monday());
    let mut summary = Task::new(1, "Tóm tắt");
    summary.outline_level = 1;
    summary.fixed_cost = 20.0;
    summary.fixed_cost_accrual = Accrual::Prorated;
    let mut child = Task::new(2, "Việc");
    child.outline_level = 2;
    child.duration = 480;
    child.work = 480;
    project.tasks = vec![summary, child];
    project.next_task_uid = 3;
    let resource = project.alloc_resource_uid();
    let mut person = sheafly_engine::Resource::new(resource, "An", ResourceKind::Person);
    person.standard_rate = 10.0;
    project.resources.push(person);
    work::add_assignment(&mut project, 2, resource, 1.0);
    schedule(&mut project).unwrap();
    assert!((project.tasks[1].cost - 80.0).abs() < 0.01);
    assert!((project.tasks[0].cost - 100.0).abs() < 0.01);
}

#[test]
fn mspdi_roundtrip_keeps_exception_hours_and_ranges() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    let tuesday = chrono::NaiveDate::from_ymd_opt(2026, 1, 6).unwrap();
    project.calendar_mut().exceptions.insert(
        sheafly_engine::timeutil::date_key(tuesday),
        vec![sheafly_engine::calendar::TimeRange::new(
            8 * 60,
            12 * 60,
        )],
    );
    let xml = write_mspdi(&project).unwrap();
    let imported = read_mspdi(&xml).unwrap();
    let ranges = imported
        .calendar()
        .exceptions
        .get("2026-01-06")
        .expect("morning exception");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_minute, 8 * 60);
    assert_eq!(ranges[0].end_minute, 12 * 60);

    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Project xmlns="http://schemas.microsoft.com/project">
  <Name>Ngoại lệ</Name>
  <StartDate>2026-01-05T08:00:00</StartDate>
  <CalendarUID>1</CalendarUID>
  <Calendars>
    <Calendar>
      <UID>1</UID>
      <Name>Chuẩn</Name>
      <WeekDays>
        <WeekDay>
          <DayType>2</DayType>
          <DayWorking>1</DayWorking>
          <WorkingTimes>
            <WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime>
            <WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime>
          </WorkingTimes>
        </WeekDay>
      </WeekDays>
      <Exceptions>
        <Exception>
          <TimePeriod>
            <FromDate>2026-01-07T00:00:00</FromDate>
            <ToDate>2026-01-09T00:00:00</ToDate>
          </TimePeriod>
          <DayWorking>0</DayWorking>
        </Exception>
      </Exceptions>
    </Calendar>
  </Calendars>
  <Tasks></Tasks>
</Project>"#;
    let imported = read_mspdi(xml).unwrap();
    for day in ["2026-01-07", "2026-01-08", "2026-01-09"] {
        assert!(
            imported
                .calendar()
                .exceptions
                .get(day)
                .is_some_and(|ranges| ranges.is_empty()),
            "{day}"
        );
    }
}

fn standard_week() -> Vec<Vec<sheafly_engine::calendar::TimeRange>> {
    use sheafly_engine::calendar::TimeRange;
    let work = vec![
        TimeRange::new(8 * 60, 12 * 60),
        TimeRange::new(13 * 60, 17 * 60),
    ];
    let mut week = vec![Vec::new(); 7];
    for day in week.iter_mut().take(5) {
        *day = work.clone();
    }
    week
}

#[test]
fn changed_working_week_skips_morning_before_0800() {
    use sheafly_engine::calendar::TimeRange;
    let mut doc = Document::from_project(Project::new("Tuần", monday()));
    let calendar_id = doc.project.calendar_id;
    let mut early = standard_week();
    early[0] = vec![
        TimeRange::new(6 * 60, 12 * 60),
        TimeRange::new(13 * 60, 17 * 60),
    ];
    doc.apply(Command::SetCalendarWeek {
        calendar_id,
        week: early,
    })
    .unwrap();
    doc.apply(Command::SetProjectStart {
        start: timestamp(2026, 1, 5, 6, 30),
    })
    .unwrap();
    assert_eq!(doc.project.start, timestamp(2026, 1, 5, 6, 30));

    doc.apply(Command::SetCalendarWeek {
        calendar_id,
        week: standard_week(),
    })
    .unwrap();
    assert!(!doc
        .project
        .calendar()
        .is_working_instant(timestamp(2026, 1, 5, 7, 30)));
    assert_eq!(
        doc.project
            .calendar()
            .next_work_start(timestamp(2026, 1, 5, 7, 30)),
        timestamp(2026, 1, 5, 8, 0)
    );
    assert_eq!(doc.project.start, timestamp(2026, 1, 5, 8, 0));
    assert_eq!(doc.project.tasks[0].start, timestamp(2026, 1, 5, 8, 0));

    doc.undo().unwrap();
    assert!(doc
        .project
        .calendar()
        .is_working_instant(timestamp(2026, 1, 5, 7, 30)));
    doc.redo().unwrap();
    assert_eq!(doc.project.start, timestamp(2026, 1, 5, 8, 0));
}

#[test]
fn full_day_holiday_skips_that_day() {
    let mut doc = Document::from_project(Project::new("Nghỉ", monday()));
    let calendar_id = doc.project.calendar_id;
    let first = doc.project.tasks[0].uid;
    doc.apply(Command::AddTask {
        name: "Sau".into(),
        after_uid: Some(first),
    })
    .unwrap();
    let second = doc.project.tasks[1].uid;
    doc.apply(Command::AddDependency {
        predecessor_uid: first,
        successor_uid: second,
        kind: DepKind::Fs,
        lag: 0,
    })
    .unwrap();
    assert_eq!(doc.project.tasks[1].start, at(1, 8, 0));

    doc.apply(Command::SetCalendarExceptions {
        calendar_id,
        from: "2026-01-06".into(),
        to: "2026-01-06".into(),
        ranges: Vec::new(),
    })
    .unwrap();
    assert!(doc
        .project
        .calendar()
        .exceptions
        .get("2026-01-06")
        .is_some_and(|ranges| ranges.is_empty()));
    assert_eq!(doc.project.tasks[0].finish, at(0, 17, 0));
    assert_eq!(doc.project.tasks[1].start, at(2, 8, 0));

    doc.undo().unwrap();
    assert_eq!(doc.project.tasks[1].start, at(1, 8, 0));
}

#[test]
fn ranged_exception_applies_to_each_day() {
    let mut doc = Document::from_project(Project::new("Khoảng nghỉ", monday()));
    let calendar_id = doc.project.calendar_id;
    let first = doc.project.tasks[0].uid;
    doc.apply(Command::AddTask {
        name: "Sau".into(),
        after_uid: Some(first),
    })
    .unwrap();
    let second = doc.project.tasks[1].uid;
    doc.apply(Command::AddDependency {
        predecessor_uid: first,
        successor_uid: second,
        kind: DepKind::Fs,
        lag: 0,
    })
    .unwrap();
    doc.apply(Command::SetCalendarExceptions {
        calendar_id,
        from: "2026-01-06".into(),
        to: "2026-01-08".into(),
        ranges: Vec::new(),
    })
    .unwrap();
    for day in ["2026-01-06", "2026-01-07", "2026-01-08"] {
        assert!(
            doc.project
                .calendar()
                .exceptions
                .get(day)
                .is_some_and(|ranges| ranges.is_empty()),
            "{day}"
        );
    }
    assert_eq!(doc.project.tasks[0].finish, at(0, 17, 0));
    assert_eq!(doc.project.tasks[1].start, at(4, 8, 0));

    doc.apply(Command::ClearCalendarExceptions {
        calendar_id,
        from: "2026-01-06".into(),
        to: "2026-01-08".into(),
    })
    .unwrap();
    assert!(doc.project.calendar().exceptions.is_empty());
    assert_eq!(doc.project.tasks[1].start, at(1, 8, 0));

    doc.apply(Command::SetCalendarExceptions {
        calendar_id,
        from: "2026-01-06".into(),
        to: "2026-01-08".into(),
        ranges: vec![sheafly_engine::calendar::TimeRange::new(9 * 60, 12 * 60)],
    })
    .unwrap();
    for day in ["2026-01-06", "2026-01-07", "2026-01-08"] {
        let ranges = doc
            .project
            .calendar()
            .exceptions
            .get(day)
            .expect(day);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start_minute, 9 * 60);
        assert_eq!(ranges[0].end_minute, 12 * 60);
    }
    assert_eq!(doc.project.tasks[1].start, at(1, 9, 0));
}

#[test]
fn task_calendar_overrides_project_calendar() {
    let mut doc = Document::from_project(Project::new("Lịch riêng", monday()));
    let task = doc.project.tasks[0].uid;
    doc.apply(Command::CreateCalendar {
        name: "Nghỉ thứ Hai".into(),
        base_id: None,
    })
    .unwrap();
    let calendar_id = doc.project.calendars.last().unwrap().id;
    assert_ne!(calendar_id, doc.project.calendar_id);
    doc.apply(Command::SetCalendarExceptions {
        calendar_id,
        from: "2026-01-05".into(),
        to: "2026-01-05".into(),
        ranges: Vec::new(),
    })
    .unwrap();
    doc.apply(Command::SetTaskCalendar {
        uid: task,
        calendar_id: Some(calendar_id),
    })
    .unwrap();
    assert!(!doc.project.calendar().exceptions.contains_key("2026-01-05"));
    assert_eq!(doc.project.tasks[0].start, at(1, 8, 0));
    assert_eq!(doc.project.tasks[0].finish, at(1, 17, 0));

    doc.apply(Command::AddTask {
        name: "Theo dự án".into(),
        after_uid: Some(task),
    })
    .unwrap();
    assert_eq!(doc.project.tasks[0].start, at(1, 8, 0));
    assert_eq!(doc.project.tasks[1].start, at(0, 8, 0));

    doc.undo().unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.project.tasks[0].calendar_id, None);
    assert_eq!(doc.project.tasks[0].start, at(0, 8, 0));
    doc.redo().unwrap();
    assert_eq!(doc.project.tasks[0].start, at(1, 8, 0));
}

#[test]
fn predecessor_text_parses_lag_units() {
    let links = parse_predecessor_text("1FS+1d, 2SS-4h, 3").unwrap();
    assert_eq!(links[0], (1, DepKind::Fs, 480));
    assert_eq!(links[1], (2, DepKind::Ss, -240));
    assert_eq!(links[2], (3, DepKind::Fs, 0));
}

#[test]
fn late_when_any_of_status_baseline_or_deadline_is_missed() {
    let project = project_with(vec![("A", 480)], vec![]);
    let mut task = project.tasks[0].clone();
    let cal = project.calendar().clone();
    let finish = task.finish;
    let start = task.start;

    // Microsoft Project's rule: late only when less is done than is due by the status date.
    assert!(!task.is_late(None, &cal));
    assert!(
        !task.is_late(Some(start), &cal),
        "nothing is due yet at the start"
    );
    assert!(task.is_late(Some(finish), &cal), "nothing done by the finish");
    task.percent_complete = 50.0;
    assert!(
        !task.is_late(Some(at(0, 13, 0)), &cal),
        "half done by the half-way point is on time"
    );
    assert!(task.is_late(Some(at(0, 14, 0)), &cal));

    task.percent_complete = 100.0;
    assert!(
        !task.is_late(Some(finish), &cal),
        "a complete task is not late from the status date alone"
    );

    task.baselines.push(sheafly_engine::model::Baseline {
        number: 1,
        start,
        finish: finish - 1,
        work: 0,
        cost: 0.0,
        segments: Vec::new(),
        fixed_cost: None,
    });
    assert!(!task.is_late(None, &cal), "only baseline 0 counts");
    task.baselines.push(sheafly_engine::model::Baseline {
        number: 0,
        start,
        finish,
        work: 0,
        cost: 0.0,
        segments: Vec::new(),
        fixed_cost: None,
    });
    assert!(
        !task.is_late(None, &cal),
        "finish equal to baseline 0 is not late"
    );
    task.baselines.retain(|baseline| baseline.number != 0);
    task.baselines.push(sheafly_engine::model::Baseline {
        number: 0,
        start,
        finish: finish - 1,
        work: 0,
        cost: 0.0,
        segments: Vec::new(),
        fixed_cost: None,
    });
    assert!(task.is_late(None, &cal));

    task.baselines.clear();
    task.deadline = Some(finish);
    assert!(
        !task.is_late(None, &cal),
        "finish equal to the deadline is not late"
    );
    task.deadline = Some(finish - 1);
    assert!(
        task.is_late(None, &cal),
        "a deadline still counts after the task is complete"
    );
}

#[test]
fn schedule_publishes_late_from_status_date_and_baseline() {
    let mut project = project_with(vec![("A", 480)], vec![]);
    let finish = project.tasks[0].finish;
    let start = project.tasks[0].start;

    project.status_date = Some(project.tasks[0].start);
    schedule(&mut project).unwrap();
    assert!(!project.tasks[0].late, "nothing is due at the start");

    project.status_date = Some(finish);
    schedule(&mut project).unwrap();
    assert!(project.tasks[0].late, "nothing done by the finish");

    project.tasks[0].percent_complete = 100.0;
    schedule(&mut project).unwrap();
    assert!(!project.tasks[0].late);

    project.status_date = None;
    project.tasks[0].percent_complete = 0.0;
    project.tasks[0]
        .baselines
        .push(sheafly_engine::model::Baseline {
            number: 0,
            start,
            finish,
            work: 0,
            cost: 0.0,
            segments: Vec::new(),
            fixed_cost: None,
        });
    schedule(&mut project).unwrap();
    assert!(!project.tasks[0].late);
    project.tasks[0].baselines[0].finish = finish - 1;
    schedule(&mut project).unwrap();
    assert!(project.tasks[0].late);

    project.tasks[0].baselines[0].number = 1;
    schedule(&mut project).unwrap();
    assert!(!project.tasks[0].late);
}

#[test]
fn deadline_command_is_undoable_and_is_not_a_constraint() {
    let mut doc = Document::from_project(project_with(vec![("A", 480)], vec![]));
    let start = doc.project.tasks[0].start;
    let finish = doc.project.tasks[0].finish;
    let uid = doc.project.tasks[0].uid;
    doc.apply(Command::SetDeadline {
        uid,
        date: Some(finish - 1),
    })
    .unwrap();
    assert_eq!(doc.project.tasks[0].start, start);
    assert_eq!(doc.project.tasks[0].finish, finish);
    assert_eq!(doc.project.tasks[0].constraint_type, ConstraintType::Asap);
    assert_eq!(doc.project.tasks[0].deadline, Some(finish - 1));
    assert!(doc.project.tasks[0].late);

    doc.undo().unwrap();
    assert_eq!(doc.project.tasks[0].deadline, None);
    assert!(!doc.project.tasks[0].late);
    doc.redo().unwrap();
    assert_eq!(doc.project.tasks[0].deadline, Some(finish - 1));

    let path = std::env::temp_dir().join(format!(
        "sheafly-deadline-{}-{}.pdproj",
        std::process::id(),
        uid
    ));
    let path = path.to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.project.tasks[0].deadline, Some(finish - 1));
    assert!(loaded.project.tasks[0].late);
    assert!(loaded
        .commands
        .iter()
        .any(|command| matches!(command, Command::SetDeadline { .. })));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn summary_deadline_does_not_make_children_late() {
    let mut project = Project::new("WBS", monday());
    let mut summary = Task::new(1, "Tóm tắt");
    summary.outline_level = 1;
    let mut child = Task::new(2, "Việc");
    child.outline_level = 2;
    child.duration = 480;
    child.work = 480;
    project.tasks = vec![summary, child];
    project.next_task_uid = 3;
    schedule(&mut project).unwrap();
    let finish = project.tasks[0].finish;
    project.tasks[0].deadline = Some(finish - 1);
    schedule(&mut project).unwrap();
    assert!(project.tasks[0].summary);
    assert!(project.tasks[0].late);
    assert!(project.tasks[1].deadline.is_none());
    assert!(!project.tasks[1].late);
    assert_eq!(project.tasks[0].constraint_type, ConstraintType::Asap);
}

// ---------------------------------------------------------------------------
// Phase 9: recurring tasks

fn day(year: i32, month: u32, date: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(year, month, date).unwrap()
}

fn midnight(year: i32, month: u32, date: u32) -> i64 {
    timestamp(year, month, date, 0, 0)
}

fn recurring_spec(pattern: RecurrencePattern, end: RecurrenceEnd) -> RecurrenceSpec {
    RecurrenceSpec {
        name: "Họp".into(),
        duration: 120,
        start: at(0, 8, 0),
        pattern,
        end,
        calendar_id: None,
        resource_uids: Vec::new(),
        resources: Vec::new(),
        background: false,
    }
}

fn series_doc(project: Project, spec: RecurrenceSpec) -> (Document, u32) {
    let mut doc = Document::from_project(project);
    let before: Vec<u32> = doc.project.tasks.iter().map(|task| task.uid).collect();
    doc.apply(Command::CreateRecurringTask {
        after_uid: Some(before[0]),
        spec,
    })
    .unwrap();
    let summary = doc
        .project
        .tasks
        .iter()
        .find(|task| task.recurrence.is_some())
        .unwrap()
        .uid;
    (doc, summary)
}

fn occurrences(project: &Project, summary: u32) -> Vec<Task> {
    project
        .tasks
        .iter()
        .filter(|task| task.series_uid == Some(summary))
        .cloned()
        .collect()
}

fn nominal_dates(project: &Project, summary: u32) -> Vec<i64> {
    occurrences(project, summary)
        .iter()
        .map(|task| task.occurrence_date.unwrap())
        .collect()
}

#[test]
fn daily_series_counts_working_days_for_count_and_end_date() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 2 },
        RecurrenceEnd::Count { count: 4 },
    );
    // Every 2 working days from Monday: Mon 5, Wed 7, Fri 9, Tue 13 (weekend not counted).
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates, vec![day(2026, 1, 5), day(2026, 1, 7), day(2026, 1, 9), day(2026, 1, 13)]);

    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Until { date: at(7, 8, 0) },
    );
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates.len(), 6);
    assert_eq!(dates.last(), Some(&day(2026, 1, 12)));
}

#[test]
fn weekly_series_uses_every_n_weeks_on_chosen_days() {
    let spec = recurring_spec(
        RecurrencePattern::Weekly { every: 2, weekdays: vec![0, 3] },
        RecurrenceEnd::Count { count: 4 },
    );
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates, vec![day(2026, 1, 5), day(2026, 1, 8), day(2026, 1, 19), day(2026, 1, 22)]);

    let spec = recurring_spec(
        RecurrencePattern::Weekly { every: 1, weekdays: vec![4] },
        RecurrenceEnd::Until { date: midnight(2026, 1, 30) },
    );
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates, vec![day(2026, 1, 9), day(2026, 1, 16), day(2026, 1, 23), day(2026, 1, 30)]);
}

#[test]
fn monthly_series_clamps_day_and_supports_both_end_conditions() {
    let mut spec = recurring_spec(
        RecurrencePattern::Monthly { every: 1, day: 31 },
        RecurrenceEnd::Until { date: midnight(2026, 4, 15) },
    );
    spec.start = midnight(2026, 1, 1);
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates, vec![day(2026, 1, 31), day(2026, 2, 28), day(2026, 3, 31)]);

    spec.pattern = RecurrencePattern::Monthly { every: 2, day: 10 };
    spec.end = RecurrenceEnd::Count { count: 3 };
    let dates = occurrence_dates(&spec, &Calendar::standard()).unwrap();
    assert_eq!(dates, vec![day(2026, 1, 10), day(2026, 3, 10), day(2026, 5, 10)]);
}

#[test]
fn series_rejects_empty_or_oversized_patterns() {
    let spec = recurring_spec(
        RecurrencePattern::Weekly { every: 1, weekdays: vec![] },
        RecurrenceEnd::Count { count: 2 },
    );
    assert!(occurrence_dates(&spec, &Calendar::standard()).is_err());
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 1000 },
    );
    assert!(occurrence_dates(&spec, &Calendar::standard()).is_err());
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Until { date: at(0, 8, 0) - 86_400_000 },
    );
    assert!(occurrence_dates(&spec, &Calendar::standard()).is_err());
}

#[test]
fn series_is_a_summary_of_real_occurrence_tasks() {
    let mut spec = recurring_spec(
        RecurrencePattern::Weekly { every: 1, weekdays: vec![0, 2] },
        RecurrenceEnd::Count { count: 4 },
    );
    let mut project = Project::new("Lặp", monday());
    project.resources.push(sheafly_engine::Resource::new(1, "An", ResourceKind::Person));
    project.next_resource_uid = 2;
    spec.resource_uids = vec![1];
    let (doc, summary_uid) = series_doc(project, spec);
    let project = &doc.project;
    let summary = project.task(summary_uid).unwrap();
    assert!(summary.summary);
    assert_eq!(summary.name, "Họp");
    let occ = occurrences(project, summary_uid);
    assert_eq!(occ.len(), 4);
    let names: Vec<&str> = occ.iter().map(|task| task.name.as_str()).collect();
    assert_eq!(names, vec!["Họp 1", "Họp 2", "Họp 3", "Họp 4"]);
    for task in &occ {
        assert_eq!(task.parent_uid, Some(summary_uid));
        assert_eq!(task.outline_level, summary.outline_level + 1);
        assert_eq!(task.duration, 120);
        assert!(!task.summary && !task.detached);
        assert_eq!(task.constraint_type, ConstraintType::Snet);
        let assigned: Vec<_> = project
            .assignments
            .iter()
            .filter(|assignment| assignment.task_uid == task.uid)
            .collect();
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].work, 120);
    }
    assert_eq!(occ[0].start, at(0, 8, 0));
    assert_eq!(occ[0].finish, at(0, 10, 0));
    assert_eq!(occ[1].start, at(2, 8, 0));
    assert_eq!(occ[3].start, at(9, 8, 0));
    assert_eq!(summary.start, occ[0].start);
    assert_eq!(summary.finish, occ[3].finish);
    // CPM runs on occurrences: only the last one drives the project finish.
    assert!(occ[3].critical);
    assert!(!occ[0].critical);
}

#[test]
fn occurrences_on_non_working_days_move_to_next_working_time() {
    let mut project = Project::new("Nghỉ", monday());
    project.calendar_mut().set_day_off(day(2026, 1, 8));
    let spec = recurring_spec(
        RecurrencePattern::Weekly { every: 1, weekdays: vec![3] },
        RecurrenceEnd::Count { count: 2 },
    );
    let (doc, summary) = series_doc(project, spec);
    let occ = occurrences(&doc.project, summary);
    // Thursday 8 is a holiday, so the first occurrence starts Friday 08:00.
    assert_eq!(occ[0].occurrence_date, Some(midnight(2026, 1, 8)));
    assert_eq!(occ[0].start, at(4, 8, 0));
    assert_eq!(occ[1].start, at(10, 8, 0));

    // Monthly day 31 on a Saturday moves to Monday.
    let mut spec = recurring_spec(
        RecurrencePattern::Monthly { every: 1, day: 31 },
        RecurrenceEnd::Count { count: 1 },
    );
    spec.start = midnight(2026, 1, 1);
    let (doc, summary) = series_doc(Project::new("Tháng", monday()), spec);
    let occ = occurrences(&doc.project, summary);
    assert_eq!(occ[0].start, timestamp(2026, 2, 2, 8, 0));

    // Daily counting skips the holiday instead of landing on it.
    let mut project = Project::new("Ngày", monday());
    project.calendar_mut().set_day_off(day(2026, 1, 6));
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 3 },
    );
    let (doc, summary) = series_doc(project, spec);
    assert_eq!(
        nominal_dates(&doc.project, summary),
        vec![midnight(2026, 1, 5), midnight(2026, 1, 7), midnight(2026, 1, 8)]
    );
}

#[test]
fn series_create_and_edit_undo_redo() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 3 },
    );
    let (mut doc, summary) = series_doc(Project::new("Undo", monday()), spec.clone());
    let with_series = doc.project.tasks.len();
    assert_eq!(with_series, 1 + 1 + 3);

    let mut longer = spec.clone();
    longer.end = RecurrenceEnd::Count { count: 5 };
    longer.duration = 240;
    doc.apply(Command::UpdateRecurringTask { uid: summary, spec: longer }).unwrap();
    assert_eq!(occurrences(&doc.project, summary).len(), 5);
    assert!(occurrences(&doc.project, summary).iter().all(|task| task.duration == 240));

    doc.undo().unwrap();
    assert_eq!(occurrences(&doc.project, summary).len(), 3);
    assert!(occurrences(&doc.project, summary).iter().all(|task| task.duration == 120));
    doc.undo().unwrap();
    assert_eq!(doc.project.tasks.len(), 1);
    assert!(doc.project.tasks.iter().all(|task| task.recurrence.is_none()));
    doc.redo().unwrap();
    assert_eq!(doc.project.tasks.len(), with_series);
    doc.redo().unwrap();
    assert_eq!(occurrences(&doc.project, summary).len(), 5);
}

#[test]
fn series_survives_store_round_trip() {
    let spec = recurring_spec(
        RecurrencePattern::Weekly { every: 1, weekdays: vec![1, 3] },
        RecurrenceEnd::Until { date: midnight(2026, 1, 31) },
    );
    let (mut doc, summary) = series_doc(Project::new("Lưu", monday()), spec);
    let second = occurrences(&doc.project, summary)[1].uid;
    doc.apply(Command::SetDuration { uid: second, minutes: 60 }).unwrap();

    let path = std::env::temp_dir().join(format!("sheafly-recurring-{}.pdproj", std::process::id()));
    let path = path.to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    let loaded = load(&path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.project.tasks, doc.project.tasks);
    let restored = loaded.project.task(summary).unwrap();
    assert_eq!(
        restored.recurrence.as_ref().unwrap().spec.pattern,
        RecurrencePattern::Weekly { every: 1, weekdays: vec![1, 3] }
    );
    assert!(loaded.project.task(second).unwrap().detached);
    assert!(loaded
        .commands
        .iter()
        .any(|command| matches!(command, Command::CreateRecurringTask { .. })));
}

#[test]
fn detached_occurrence_survives_regeneration() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 4 },
    );
    let (mut doc, summary) = series_doc(Project::new("Tách", monday()), spec.clone());
    let occ = occurrences(&doc.project, summary);
    let edited = occ[1].uid;
    let first = occ[0].uid;
    doc.apply(Command::RenameTask { uid: edited, name: "Họp đặc biệt".into() }).unwrap();
    doc.apply(Command::SetDuration { uid: edited, minutes: 300 }).unwrap();
    doc.apply(Command::AddDependency {
        predecessor_uid: 1,
        successor_uid: first,
        kind: DepKind::Fs,
        lag: 0,
    })
    .unwrap();
    assert!(doc.project.task(edited).unwrap().detached);
    assert!(!doc.project.task(first).unwrap().detached);

    let mut changed = spec;
    changed.name = "Giao ban".into();
    changed.duration = 60;
    doc.apply(Command::UpdateRecurringTask { uid: summary, spec: changed }).unwrap();

    let occ = occurrences(&doc.project, summary);
    assert_eq!(occ.len(), 4);
    let kept = doc.project.task(edited).unwrap();
    assert_eq!(kept.name, "Họp đặc biệt");
    assert_eq!(kept.duration, 300);
    assert!(kept.detached);
    let names: Vec<&str> = occ.iter().map(|task| task.name.as_str()).collect();
    assert_eq!(names, vec!["Giao ban 1", "Họp đặc biệt", "Giao ban 3", "Giao ban 4"]);
    assert!(occ.iter().filter(|task| !task.detached).all(|task| task.duration == 60));
    // The attached first occurrence kept its UID, so its link survived.
    assert_eq!(occ[0].uid, first);
    assert!(doc
        .project
        .dependencies
        .iter()
        .any(|dep| dep.predecessor_uid == 1 && dep.successor_uid == first));
    assert_eq!(doc.project.task(summary).unwrap().name, "Giao ban");
}

#[test]
fn deleting_an_occurrence_keeps_it_out_after_regeneration() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 3 },
    );
    let (mut doc, summary) = series_doc(Project::new("Bỏ", monday()), spec.clone());
    let middle = occurrences(&doc.project, summary)[1].uid;
    doc.apply(Command::DeleteTask { uid: middle }).unwrap();
    let mut changed = spec;
    changed.duration = 90;
    doc.apply(Command::UpdateRecurringTask { uid: summary, spec: changed }).unwrap();
    assert_eq!(
        nominal_dates(&doc.project, summary),
        vec![midnight(2026, 1, 5), midnight(2026, 1, 7)]
    );
}

#[test]
fn deleting_series_removes_attached_and_keeps_detached() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 3 },
    );
    let (mut doc, summary) = series_doc(Project::new("Xóa", monday()), spec);
    let occ = occurrences(&doc.project, summary);
    let detached = occ[2].uid;
    doc.apply(Command::MoveTask { uid: detached, start: at(10, 8, 0) }).unwrap();
    doc.apply(Command::AddDependency {
        predecessor_uid: occ[0].uid,
        successor_uid: 1,
        kind: DepKind::Fs,
        lag: 0,
    })
    .unwrap();

    doc.apply(Command::DeleteTask { uid: summary }).unwrap();
    assert!(doc.project.task(summary).is_none());
    assert!(doc.project.task(occ[0].uid).is_none());
    assert!(doc.project.task(occ[1].uid).is_none());
    assert!(doc.project.dependencies.is_empty());
    let kept = doc.project.task(detached).unwrap();
    assert_eq!(kept.outline_level, 1);
    assert_eq!(kept.series_uid, None);
    assert_eq!(kept.start, at(10, 8, 0));
    assert_eq!(doc.project.tasks.len(), 2);

    doc.undo().unwrap();
    assert_eq!(occurrences(&doc.project, summary).len(), 3);
}

#[test]
fn occurrences_cannot_change_level_and_mspdi_keeps_them() {
    let spec = recurring_spec(
        RecurrencePattern::Daily { every: 1 },
        RecurrenceEnd::Count { count: 2 },
    );
    let (mut doc, summary) = series_doc(Project::new("XML", monday()), spec);
    let occ = occurrences(&doc.project, summary);
    assert!(doc.apply(Command::Indent { uid: occ[1].uid }).is_err());
    assert!(doc.apply(Command::Outdent { uid: occ[0].uid }).is_err());

    let xml = write_mspdi(&doc.project).unwrap();
    assert!(xml.contains("<Recurring>1</Recurring>"));
    let imported = read_mspdi(&xml).unwrap();
    let names: Vec<&str> = imported.tasks.iter().map(|task| task.name.as_str()).collect();
    assert!(names.contains(&"Họp 1") && names.contains(&"Họp 2"));
    let summary_row = imported.tasks.iter().find(|task| task.name == "Họp").unwrap();
    assert!(summary_row.summary);
    let first = imported.tasks.iter().find(|task| task.name == "Họp 1").unwrap();
    assert_eq!(first.parent_uid, Some(summary_row.uid));
    assert_eq!(first.start, occ[0].start);
}

// ---- Phase 10: EVM and burn-down ----

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "expected {expected}, got {actual}");
}

fn close_opt(actual: Option<f64>, expected: f64) {
    close(actual.expect("value should be defined"), expected);
}

fn set_baseline_zero(task: &mut Task, cost: f64) {
    task.baselines.retain(|baseline| baseline.number != 0);
    task.baselines.push(Baseline {
        number: 0,
        start: task.start,
        finish: task.finish,
        work: task.work,
        cost,
        segments: Vec::new(),
        // These tests price the task by the accrual alone: all of it is fixed cost.
        fixed_cost: Some(cost),
    });
}

/// Three parallel two-day tasks (Mon 08:00 to Tue 17:00), baseline cost 1000 each,
/// accruing at start, prorated and at end.
fn accrual_project() -> Project {
    let mut project = project_with(vec![("Đầu", 960), ("Phân bổ", 960), ("Cuối", 960)], vec![]);
    let accruals = [Accrual::Start, Accrual::Prorated, Accrual::End];
    for (task, accrual) in project.tasks.iter_mut().zip(accruals) {
        task.fixed_cost_accrual = accrual;
        set_baseline_zero(task, 1000.0);
    }
    project
}

#[test]
fn evm_planned_value_follows_each_accrual() {
    let project = accrual_project();
    assert_eq!(project.tasks[0].start, at(0, 8, 0));
    assert_eq!(project.tasks[0].finish, at(1, 17, 0));

    // Before any baseline start nothing is planned.
    close(evm_at(&project, at(0, 7, 0)).pv, 0.0);
    // Monday 17:00: start = 1000, prorated = 480/960 × 1000, end = 0.
    close(evm_at(&project, at(0, 17, 0)).pv, 1500.0);
    // Monday 12:30 is lunch: 240 working minutes elapsed → 250 prorated.
    close(evm_at(&project, at(0, 12, 30)).pv, 1250.0);
    // Tuesday 10:00: 480 + 120 = 600 of 960 → 625 prorated.
    close(evm_at(&project, at(1, 10, 0)).pv, 1625.0);
    // At the baseline finish all three accruals are fully planned.
    close(evm_at(&project, at(1, 17, 0)).pv, 3000.0);
    close(evm_at(&project, at(5, 9, 0)).pv, 3000.0);
}

#[test]
fn evm_metrics_match_hand_computed_values() {
    let mut project = accrual_project();
    let percents = [50.0, 25.0, 0.0];
    let actuals = [600.0, 400.0, 0.0];
    for ((task, pct), cost) in project.tasks.iter_mut().zip(percents).zip(actuals) {
        task.percent_complete = pct;
        task.actual_cost = cost;
    }
    let m = evm_at(&project, at(0, 17, 0));
    close(m.bac, 3000.0);
    close(m.pv, 1500.0);
    close(m.ev, 750.0); // 500 + 250 + 0
    close(m.ac, 1000.0);
    close(m.sv, -750.0);
    close(m.cv, -250.0);
    close_opt(m.spi, 0.5);
    close_opt(m.cpi, 0.75);
    close_opt(m.eac, 4000.0);
    close_opt(m.etc, 3000.0);
    close_opt(m.vac, -1000.0);

    project.status_date = Some(at(0, 17, 0));
    let report = evm(&project);
    assert!(report.has_baseline);
    assert_eq!(report.metrics, Some(m));
}

#[test]
fn evm_counts_leaf_tasks_only() {
    let mut doc = Document::from_project(project_with(
        vec![("Tổng", 480), ("A", 480), ("B", 480)],
        vec![],
    ));
    doc.apply(Command::Indent { uid: 2 }).unwrap();
    doc.apply(Command::Indent { uid: 3 }).unwrap();
    doc.apply(Command::SetFixedCost { uid: 2, cost: 1000.0, accrual: Accrual::Start }).unwrap();
    doc.apply(Command::SetFixedCost { uid: 3, cost: 500.0, accrual: Accrual::End }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: 2, percent: 100.0 }).unwrap();

    let summary = doc.project.task(1).unwrap();
    assert!(summary.summary);
    // The summary repeats its children's figures; adding it would double them.
    assert_eq!(summary.baselines[0].cost, 1500.0);
    assert_eq!(summary.actual_cost, 1000.0);

    let m = evm_at(&doc.project, at(0, 12, 0));
    close(m.bac, 1500.0);
    close(m.pv, 1000.0); // A accrues at start; B (end) finishes at 17:00.
    close(m.ev, 1000.0);
    close(m.ac, 1000.0);
    close_opt(m.spi, 1.0);
    close_opt(m.cpi, 1.0);
    close_opt(m.eac, 1500.0);
    close_opt(m.etc, 500.0);
    close_opt(m.vac, 0.0);
}

#[test]
fn evm_leaves_ratios_undefined_when_denominators_are_zero() {
    let mut project = accrual_project();

    // No status date: no metrics, even with a baseline.
    let report = evm(&project);
    assert!(report.has_baseline);
    assert_eq!(report.metrics, None);

    // No baseline 0: no metrics, even with a status date.
    let mut bare = project_with(vec![("A", 480)], vec![]);
    bare.status_date = Some(at(0, 17, 0));
    let report = evm(&bare);
    assert!(!report.has_baseline);
    assert_eq!(report.metrics, None);

    // PV = 0 and AC = 0: SPI, CPI, EAC, ETC, VAC are undefined.
    let m = evm_at(&project, at(0, 7, 0));
    close(m.pv, 0.0);
    close(m.ac, 0.0);
    assert_eq!((m.spi, m.cpi, m.eac, m.etc, m.vac), (None, None, None, None, None));
    close(m.sv, 0.0);
    close(m.cv, 0.0);

    // EV = 0 with AC > 0: CPI is 0, so EAC (and ETC, VAC) stay undefined.
    project.tasks[0].actual_cost = 200.0;
    let m = evm_at(&project, at(0, 17, 0));
    close_opt(m.spi, 0.0);
    close_opt(m.cpi, 0.0);
    assert_eq!((m.eac, m.etc, m.vac), (None, None, None));
    close(m.cv, -200.0);
}

#[test]
fn burndown_ideal_line_and_snapshots() {
    let mut doc = Document::from_project(project_with(
        vec![("A", 960), ("B", 480)],
        vec![(1, 2, DepKind::Fs, 0)],
    ));
    doc.apply(Command::SetFixedCost { uid: 1, cost: 1000.0, accrual: Accrual::Prorated }).unwrap();
    doc.apply(Command::SetFixedCost { uid: 2, cost: 400.0, accrual: Accrual::End }).unwrap();
    doc.apply(Command::SetBaseline { number: 0 }).unwrap();

    let report = doc.metrics().burndown;
    assert_eq!(report.baseline_start, Some(at(0, 8, 0)));
    assert_eq!(report.baseline_finish, Some(at(2, 17, 0)));
    assert_eq!(report.baseline_work, 1440);
    assert_eq!(
        report.ideal,
        vec![
            BurndownPoint { date: at(0, 8, 0), remaining_work: 1440 },
            BurndownPoint { date: at(2, 17, 0), remaining_work: 0 },
        ]
    );
    assert!(report.actual.is_empty());

    // Progress alone does not sample; a status date change does.
    doc.apply(Command::SetPercentComplete { uid: 1, percent: 50.0 }).unwrap();
    assert!(doc.burndown.is_empty());
    doc.apply(Command::SetStatusDate { date: Some(at(0, 17, 0)) }).unwrap();
    assert_eq!(
        doc.burndown,
        vec![BurndownSnapshot { date: at(0, 0, 0), remaining_work: 960, ev: 500.0, ac: 500.0 }]
    );

    doc.apply(Command::SetPercentComplete { uid: 1, percent: 100.0 }).unwrap();
    doc.apply(Command::SetStatusDate { date: Some(at(1, 17, 0)) }).unwrap();
    doc.apply(Command::SetPercentComplete { uid: 2, percent: 50.0 }).unwrap();

    // Saving samples again; the same day overwrites its snapshot.
    let path = std::env::temp_dir().join(format!("sheafly-burndown-{}.pdproj", std::process::id()));
    let path = path.to_string_lossy().to_string();
    save_with_snapshot(&mut doc, &path, at(9, 9, 0)).unwrap();
    let expected = vec![
        BurndownSnapshot { date: at(0, 0, 0), remaining_work: 960, ev: 500.0, ac: 500.0 },
        // B half done: 240 left, EV 1000 + 200, AC still 1000 because B accrues at end.
        BurndownSnapshot { date: at(1, 0, 0), remaining_work: 240, ev: 1200.0, ac: 1000.0 },
    ];
    assert_eq!(doc.burndown, expected);
    let report = doc.metrics().burndown;
    assert_eq!(report.actual, expected);
    assert_eq!(report.current_remaining_work, 240);

    // Snapshots survive save/load and undo.
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.burndown, expected);
    doc.undo().unwrap();
    assert_eq!(doc.burndown, expected);

    // Only snapshots up to the status date are drawn.
    let mut earlier = doc.project.clone();
    earlier.status_date = Some(at(0, 17, 0));
    assert_eq!(burndown(&earlier, &expected).actual, expected[..1].to_vec());

    // Files from before phase 10 have no burn-down table and load with none.
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute("DROP TABLE burndown", []).unwrap();
    drop(connection);
    assert!(load(&path).unwrap().burndown.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn burndown_without_status_date_samples_the_given_day() {
    let mut doc = Document::from_project(project_with(vec![("A", 480)], vec![]));
    doc.record_burndown(at(3, 15, 30));
    doc.record_burndown(at(3, 16, 0));
    assert_eq!(doc.burndown.len(), 1);
    assert_eq!(doc.burndown[0].date, at(3, 0, 0));
    assert_eq!(doc.burndown[0].remaining_work, 480);
    let report = doc.metrics().burndown;
    assert!(report.ideal.is_empty());
    assert_eq!(report.baseline_work, 0);
    assert_eq!(report.actual.len(), 1);
}

#[test]
fn network_layout_follows_scheduled_dependencies_and_cycles_are_rejected() {
    use sheafly_engine::network::{layout, NetworkOptions};

    let mut doc = Document::from_project(project_with(
        vec![("A", 480), ("B", 960), ("C", 480), ("Mốc", 0)],
        vec![(1, 2, DepKind::Fs, 0), (1, 3, DepKind::Ss, 240), (2, 4, DepKind::Fs, 0), (3, 4, DepKind::Fs, 0)],
    ));
    // CPM forbids cycles, so the diagram never has to break one.
    let err = doc.apply(Command::AddDependency {
        predecessor_uid: 4,
        successor_uid: 1,
        kind: DepKind::Fs,
        lag: 0,
    });
    assert!(err.is_err());

    let result = layout(&doc.project, &NetworkOptions::default());
    let column = |uid: u32| result.nodes.iter().find(|node| node.uid == uid).unwrap().column;
    assert_eq!((column(1), column(2), column(3), column(4)), (0, 1, 1, 2));
    let ss = result.edges.iter().find(|edge| edge.from_uid == 1 && edge.to_uid == 3).unwrap();
    assert_eq!((ss.kind, ss.lag), (DepKind::Ss, 240));
    assert_eq!(result.crossings, 0);
}
