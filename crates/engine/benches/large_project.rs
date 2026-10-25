//! Timing for large projects: apply, undo, redo, save, and load.
//!
//! `cargo bench -p sheafly-engine` builds a project of grouped tasks with chained
//! and summary-to-summary links, then pads the log with edits. Pass `-- <tasks> <commands>`
//! to change the size (default 1000 tasks, 10000 commands).
//!
//! `-- leveling` levels 500 tasks on ten people. `-- capacity [<people> <tasks>]` levels a
//! sprint-sized backlog (default 100 people, 1000 tasks): unlinked tasks of one to sixteen
//! hours with random priorities, one person each, built straight into the model so the
//! setup does not replay thousands of commands. Add `field` to level the backlog by a
//! custom choice field first (`[Field asc, Priority desc, Start asc]`), most tasks having
//! a value, and `leave` to give each person up to three periods of other units (a week
//! off, weeks at half time), and `interrupt` to mark 5% of the tasks urgent (priority
//! 900) and let half the tasks start up to two weeks late, so urgent work lands on work
//! in progress.
//!
//! `-- suggest [<people> <tasks>]` times the assignment suggestions for an unassigned
//! backlog (default 500 people, 5000 tasks): twenty skills, one to three per person, and
//! twenty rules from the values of a list field to the skills.
//!
//! `-- loadmap [<people> <tasks>]` levels a capacity backlog (default 500 people, 5000
//! tasks), then times `load_map` for a year by week and for twelve weeks by day.
//!
//! `-- import [<tasks>]` times `check_import` and `ImportRows` for a table of people and
//! tasks (default 5000 tasks in groups of ten, one person per ten tasks, each leaf with
//! effort, one person and, every other row, a wait on the row above).

use std::time::{Duration, Instant};

use chrono::NaiveDate;

use sheafly_engine::availability::AvailabilityPeriod;
use sheafly_engine::command::Command;
use sheafly_engine::fields::{FieldDef, FieldKind, FieldValue};
use sheafly_engine::forecast::{forecast, forecast_with_threads};
use sheafly_engine::import::{check_import, ImportAssignee, ImportResource, ImportTask};
use sheafly_engine::level::level_project;
use sheafly_engine::level_order::{Criterion, SortKey};
use sheafly_engine::load_map::{load_map, Bucket, LoadGroup};
use sheafly_engine::model::{Assignment, ConstraintType, DepKind, Dependency, Resource, ResourceKind, Task, TaskType};
use sheafly_engine::skills::{AssignmentRule, Condition, RuleTarget, SkillDef};
use sheafly_engine::suggest::suggest;
use sheafly_engine::store::{load, save};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{schedule, Document, Project};

const GROUP: u32 = 10;
const DAY: i64 = 480;

/// Commands for `tasks` tasks in groups of ten: the first task of a group is its summary,
/// leaves are chained FS, and each summary follows the previous one. Edits pad the log.
fn build_commands(tasks: u32, total: usize) -> Vec<Command> {
    let mut commands = Vec::new();
    // Task 1 already exists in a new document.
    for uid in 2..=tasks {
        commands.push(Command::AddTask {
            name: format!("Việc {uid}"),
            after_uid: Some(uid - 1),
        });
    }
    for uid in 1..=tasks {
        if !(uid - 1).is_multiple_of(GROUP) {
            commands.push(Command::Indent { uid });
        }
    }
    for uid in 1..=tasks {
        if !(uid - 1).is_multiple_of(GROUP) {
            commands.push(Command::SetDuration {
                uid,
                minutes: DAY * i64::from(1 + uid % 4),
            });
        }
    }
    for uid in 1..=tasks {
        let position = (uid - 1) % GROUP;
        if position >= 2 {
            commands.push(dependency(uid - 1, uid));
        } else if position == 0 && uid > GROUP {
            commands.push(dependency(uid - GROUP, uid));
        }
    }
    let mut edit = 0u32;
    while commands.len() < total {
        let uid = 2 + edit % (tasks - 1);
        if !(uid - 1).is_multiple_of(GROUP) {
            commands.push(Command::SetDuration {
                uid,
                minutes: DAY * i64::from(1 + edit % 5),
            });
        } else {
            commands.push(Command::RenameTask {
                uid,
                name: format!("Nhóm {uid}.{edit}"),
            });
        }
        edit += 1;
    }
    commands
}

fn dependency(predecessor_uid: u32, successor_uid: u32) -> Command {
    Command::AddDependency {
        predecessor_uid,
        successor_uid,
        kind: DepKind::Fs,
        lag: 0,
    }
}

fn timed<T>(label: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let value = f();
    let elapsed = started.elapsed();
    println!("{label:<28} {:>10.1} ms", elapsed.as_secs_f64() * 1000.0);
    (value, elapsed)
}

/// 500 unlinked tasks of one to three days, all from the project start, shared by ten
/// people: fifty-way clashes for leveling to resolve.
fn leveling(tasks: u32) {
    let mut doc = Document::from_project(Project::new("Cân bằng", timestamp(2026, 1, 5, 8, 0)));
    let mut commands = Vec::new();
    for uid in 2..=tasks {
        commands.push(Command::AddTask { name: format!("Việc {uid}"), after_uid: Some(uid - 1) });
    }
    for uid in 1..=tasks {
        commands.push(Command::SetDuration { uid, minutes: DAY * i64::from(1 + uid % 3) });
    }
    for person in 0..10 {
        commands.push(Command::AddResource { name: format!("Người {person}"), kind: ResourceKind::Person });
    }
    for uid in 1..=tasks {
        commands.push(Command::AssignResource { task_uid: uid, resource_uid: 1 + uid % 10, units: 1.0 });
    }
    for command in commands {
        doc.apply(command).expect("command applies");
    }
    println!("{tasks} công việc, {} khoảng quá tải", doc.project.overallocations.len());
    timed("cân bằng nguồn lực", || doc.apply(Command::LevelResources).expect("level"));
    assert!(doc.project.overallocations.is_empty(), "{:?}", doc.project.overallocations.first());
    // No gaps would finish when the busiest person's work runs out.
    let busiest = (1..=10)
        .map(|person| (1..=tasks).filter(|uid| 1 + uid % 10 == person).map(|uid| 1 + uid % 3).sum::<u32>())
        .max()
        .unwrap_or(0);
    let finish = doc.project.tasks.iter().map(|task| task.finish).max().unwrap_or(0);
    let calendar = doc.project.calendar().clone();
    let days = calendar.working_between(doc.project.start, finish) as f64 / DAY as f64;
    println!("{:<28} {:>10.1} ngày (tối thiểu {busiest})", "độ dài sau cân bằng", days);
}

/// A backlog of `tasks` effort-only tasks spread over `people`, the way a sprint plan
/// arrives: no links, every task free to start at the project start, priority deciding
/// the order. Prints the leveling time and the idle hours left between a person's tasks.
fn capacity(people: u32, tasks: u32, by_field: bool, leave: bool, interrupt: bool, linked: bool) -> Project {
    // A fixed linear congruential generator, so every run builds the same backlog.
    let mut seed: u64 = 42;
    let mut random = |below: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % below
    };
    let mut project = Project::new("Năng lực", timestamp(2026, 1, 5, 8, 0));
    let monday = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    for uid in 1..=people {
        let mut resource = Resource::new(uid, format!("Người {uid}"), ResourceKind::Person);
        if leave {
            let mut day = random(10) as i64;
            for _ in 0..random(4) {
                let length = if random(2) == 0 { 4 } else { 11 };
                let from = monday + chrono::Duration::days(day);
                let to = from + chrono::Duration::days(length);
                let units = if random(2) == 0 { 0.0 } else { 0.5 };
                resource.availability.push(AvailabilityPeriod { from: Some(from), to: Some(to), units });
                day += length + 1 + random(14) as i64;
            }
        }
        project.resources.push(resource);
    }
    for uid in 1..=tasks {
        let work = 60 * (1 + random(16) as i64);
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.task_type = TaskType::FixedWork;
        task.duration = work;
        task.remaining_duration = work;
        task.work = work;
        task.remaining_work = work;
        task.priority = 100 + random(800) as i32;
        task.start = project.start;
        if interrupt {
            if random(2) == 0 {
                task.constraint_type = ConstraintType::Snet;
                task.constraint_date = Some(project.start + random(10) as i64 * 86_400_000);
            }
            if random(20) == 0 {
                task.interrupts = true;
                task.priority = 900;
            }
        }
        if by_field && random(10) > 0 {
            let option = ["Gấp", "Cao", "Vừa", "Thấp"][random(4) as usize];
            task.values.insert(1, FieldValue::Text(option.into()));
        }
        project.tasks.push(task);
        // Chains of ten, and a link from an earlier task on about a quarter of them.
        if linked && uid % 10 != 1 {
            let next = project.dependencies.len() as u32 + 1;
            project.dependencies.push(Dependency { uid: next, predecessor_uid: uid - 1, successor_uid: uid, kind: DepKind::Fs, lag: 0 });
            if uid > 20 && random(4) == 0 {
                let from = 1 + random(u64::from(uid - 11)) as u32;
                project.dependencies.push(Dependency { uid: next + 1, predecessor_uid: from, successor_uid: uid, kind: DepKind::Fs, lag: 0 });
            }
        }
        project.assignments.push(Assignment {
            uid,
            task_uid: uid,
            resource_uid: 1 + random(u64::from(people)) as u32,
            units: 1.0,
            work,
            overtime_work: 0,
            actual_work: 0,
            remaining_work: work,
            cost: 0.0,
            actual_cost: 0.0,
            manual_actual_cost: None,
            baselines: Vec::new(),
        });
    }
    project.next_task_uid = tasks + 1;
    project.next_resource_uid = people + 1;
    project.next_assignment_uid = tasks + 1;
    project.next_dependency_uid = project.dependencies.len() as u32 + 1;
    if by_field {
        let options = ["Gấp", "Cao", "Vừa", "Thấp"].iter().map(|o| o.to_string()).collect();
        let kind = FieldKind::Choice { options };
        project.fields.push(FieldDef { id: 1, name: "Mức".into(), kind });
        project.next_field_id = 2;
        project.leveling_order = vec![
            SortKey { criterion: Criterion::Field { id: 1 }, descending: false },
            SortKey { criterion: Criterion::Priority, descending: true },
            SortKey { criterion: Criterion::Start, descending: false },
        ];
    }

    timed("lập lịch", || schedule(&mut project).expect("schedule"));
    println!("{people} người, {tasks} công việc, {} khoảng quá tải", project.overallocations.len());
    timed("cân bằng nguồn lực", || level_project(&mut project).expect("level"));
    if interrupt {
        let made = project.tasks.iter().flat_map(|task| &task.splits).filter(|split| split.leveling).count();
        println!("{:<28} {:>10}", "khoảng tách do chen ngang", made);
        println!("{:<28} {:>10}", "khoảng quá tải còn lại", project.overallocations.len());
        return project;
    }
    assert!(project.overallocations.is_empty(), "{:?}", project.overallocations.first());

    // Working time from the project start to a person's last finish, less their work:
    // zero when leveling packs each person's tasks back to back (with `leave`, the days
    // off and the half days count too).
    let calendar = project.calendar().clone();
    let idle: i64 = (1..=people)
        .filter_map(|person| {
            let mine: Vec<&Task> = project
                .assignments
                .iter()
                .filter(|assignment| assignment.resource_uid == person)
                .filter_map(|assignment| project.task(assignment.task_uid))
                .collect();
            let finish = mine.iter().map(|task| task.finish).max()?;
            let work: i64 = mine.iter().map(|task| task.work).sum();
            Some(calendar.working_between(project.start, finish) - work)
        })
        .sum();
    println!("{:<28} {:>10.1} giờ", "giờ rảnh xen giữa", idle as f64 / 60.0);
    project
}

/// An unassigned backlog of `tasks` tasks, each with one of twenty values of a list field
/// that a rule maps to one of twenty skills; `people` have one to three skills each.
/// Prints how long the suggestions take and how evenly the top picks spread.
fn suggestions(people: u32, tasks: u32) {
    let mut seed: u64 = 7;
    let mut random = |below: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % below
    };
    const SKILLS: u32 = 20;
    let mut project = Project::new("Gợi ý", timestamp(2026, 1, 5, 8, 0));
    project.skills = (1..=SKILLS).map(|id| SkillDef { id, name: format!("Kỹ năng {id}") }).collect();
    project.next_skill_id = SKILLS + 1;
    for uid in 1..=people {
        let mut resource = Resource::new(uid, format!("Người {uid}"), ResourceKind::Person);
        for _ in 0..=random(3) {
            resource.skills.push(1 + random(u64::from(SKILLS)) as u32);
        }
        resource.skills.sort_unstable();
        resource.skills.dedup();
        project.resources.push(resource);
    }
    let options: Vec<String> = (1..=SKILLS).map(|value| format!("Mảng {value}")).collect();
    project.fields.push(FieldDef { id: 1, name: "Mảng".into(), kind: FieldKind::Choice { options: options.clone() } });
    project.next_field_id = 2;
    project.assignment_rules = (1..=SKILLS)
        .map(|id| AssignmentRule {
            conditions: vec![Condition { field: 1, values: vec![FieldValue::Text(options[id as usize - 1].clone())] }],
            target: RuleTarget::Skill { id, min_level: None },
            units: 1.0,
        })
        .collect();
    for uid in 1..=tasks {
        let work = 60 * (1 + random(16) as i64);
        let mut task = Task::new(uid, format!("Việc {uid}"));
        task.task_type = TaskType::FixedWork;
        task.duration = work;
        task.remaining_duration = work;
        task.work = work;
        task.remaining_work = work;
        task.priority = 100 + random(800) as i32;
        let option = options[random(u64::from(SKILLS)) as usize].clone();
        task.values.insert(1, FieldValue::Text(option));
        project.tasks.push(task);
    }
    project.next_task_uid = tasks + 1;
    project.next_resource_uid = people + 1;
    timed("lập lịch", || schedule(&mut project).expect("schedule"));
    println!("{people} người, {tasks} công việc, {SKILLS} kỹ năng");
    let (suggested, _) = timed("gợi ý giao việc", || suggest(&project));
    let picked: Vec<u32> = suggested.iter().filter_map(|s| s.picks.first().map(|pick| pick.resource_uid)).collect();
    let mut per_person = vec![0u32; people as usize + 1];
    for uid in &picked {
        per_person[*uid as usize] += 1;
    }
    let busiest = per_person.iter().max().copied().unwrap_or(0);
    let finish = suggested.iter().filter_map(|s| s.picks.first().map(|pick| pick.finish)).max().unwrap_or(0);
    let days = project.calendar().working_between(project.start, finish) as f64 / DAY as f64;
    println!("{:<28} {:>10} / {tasks}", "có gợi ý", picked.len());
    println!("{:<28} {:>10} việc", "nhiều nhất một người", busiest);
    println!("{:<28} {:>10.1} ngày", "xong muộn nhất (ước tính)", days);
}

fn import(tasks: u32) {
    let people = (tasks / 10).max(1);
    let resources: Vec<ImportResource> = (1..=people)
        .map(|uid| ImportResource {
            name: format!("Người {uid}"),
            kind: ResourceKind::Person,
            skills: vec![format!("Kỹ năng {}", uid % 20)],
            ..ImportResource::default()
        })
        .collect();
    let rows: Vec<ImportTask> = (0..tasks)
        .map(|row| {
            if row % GROUP == 0 {
                return ImportTask {
                    name: format!("Nhóm {row}"),
                    ..ImportTask::default()
                };
            }
            ImportTask {
                name: format!("Việc {row}"),
                level: 1,
                work: Some(60 * i64::from(1 + row % 16)),
                priority: Some(100 + (row % 800) as i32),
                assignees: vec![ImportAssignee {
                    name: format!("Người {}", 1 + row % people),
                    units: 1.0,
                }],
                after: if row % GROUP >= 2 && row % 2 == 0 { vec![row - 1] } else { vec![] },
                ..ImportTask::default()
            }
        })
        .collect();
    println!("nhập {people} người, {tasks} công việc");
    let mut doc = Document::from_project(Project::new("Nhập", timestamp(2026, 1, 5, 8, 0)));
    let (errors, _) = timed("check_import", || check_import(&doc.project, Some(1), &resources, &rows));
    assert!(errors.is_empty(), "{:?}", errors.first());
    timed("ImportRows", || {
        doc.apply(Command::ImportRows {
            after_uid: Some(1),
            resources: resources.clone(),
            tasks: rows.clone(),
        })
        .expect("import applies")
    });
    timed("undo", || doc.undo().expect("undo"));
    timed("redo", || doc.redo().expect("redo"));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let numbers: Vec<usize> = args.iter().filter_map(|arg| arg.parse().ok()).collect();
    if args.iter().any(|arg| arg == "matrix") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        let project = capacity(people, tasks, false, true, false, false);
        timed("assignment_matrix người x việc (500 người, 5000 việc)", || {
            sheafly_engine::assignment_matrix::assignment_matrix(&project, false)
        });
        timed("assignment_matrix vai trò x việc (500 người, 5000 việc)", || {
            sheafly_engine::assignment_matrix::assignment_matrix(&project, true)
        });
        return;
    }
    if args.iter().any(|arg| arg == "baseline") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        let mut doc = Document::from_project(capacity(people, tasks, false, true, false, false));
        doc.apply(Command::SetBaseline { number: 0 }).expect("baseline");
        let project = &doc.project;
        let (by_resource, _) = timed("baseline_load_by_resource (tất cả người)", || {
            sheafly_engine::baseline_load::baseline_load_by_resource(project, 0)
        });
        let minutes: i64 = by_resource.values().flat_map(|days| days.values()).sum();
        println!("{} người có tải baseline, {} giờ", by_resource.len(), minutes / 60);
        timed("project_baseline_load_by_day", || {
            sheafly_engine::baseline_load::project_baseline_load_by_day(project, 0)
        });
        let monday = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        timed("sum_in_range 52 tuần cho mọi người", || {
            by_resource
                .values()
                .map(|days| {
                    (0..52)
                        .map(|week| {
                            let from = monday + chrono::Duration::weeks(week);
                            sheafly_engine::baseline_load::sum_in_range(days, from, from + chrono::Duration::days(6))
                        })
                        .sum::<i64>()
                })
                .sum::<i64>()
        });
        return;
    }
    if args.iter().any(|arg| arg == "loadmap") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        let project = capacity(people, tasks, false, true, false, false);
        let monday = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        timed("load_map 52 tuần", || {
            load_map(&project, monday, monday + chrono::Duration::days(363), Bucket::Week, LoadGroup::Person, None).expect("load map")
        });
        timed("load_map 84 ngày", || {
            load_map(&project, monday, monday + chrono::Duration::days(83), Bucket::Day, LoadGroup::Person, None).expect("load map")
        });
        timed("load_map 12 tháng", || {
            load_map(&project, monday, monday + chrono::Duration::days(363), Bucket::Month, LoadGroup::Person, None).expect("load map")
        });
        timed("load_map vai trò 12 tháng", || {
            load_map(&project, monday, monday + chrono::Duration::days(363), Bucket::Month, LoadGroup::Group, None).expect("load map")
        });
        return;
    }
    if args.iter().any(|arg| arg == "curves") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        let mut doc = Document::from_project(capacity(people, tasks, false, true, false, false));
        doc.apply(Command::SetBaseline { number: 0 }).expect("baseline");
        let project = &doc.project;
        timed("curves cost tuần (500 người, 5000 việc)", || {
            sheafly_engine::curves::curves(project, sheafly_engine::curves::CurveMeasure::Cost, Bucket::Week, sheafly_engine::curves::CurveFilter::Project, Some(0)).expect("curves")
        });
        timed("curves work ngày (500 người, 5000 việc)", || {
            sheafly_engine::curves::curves(project, sheafly_engine::curves::CurveMeasure::Work, Bucket::Day, sheafly_engine::curves::CurveFilter::Project, Some(0)).expect("curves")
        });
        timed("curves progress tháng (500 người, 5000 việc)", || {
            sheafly_engine::curves::curves(project, sheafly_engine::curves::CurveMeasure::Progress, Bucket::Month, sheafly_engine::curves::CurveFilter::Project, Some(0)).expect("curves")
        });
        timed("curves filter vai trò tháng (500 người, 5000 việc)", || {
            sheafly_engine::curves::curves(project, sheafly_engine::curves::CurveMeasure::Work, Bucket::Month, sheafly_engine::curves::CurveFilter::Group { name: "Backend".into() }, Some(0)).expect("curves")
        });
        return;
    }
    if args.iter().any(|arg| arg == "summary") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        project_summaries(people, tasks);
        return;
    }
    if args.iter().any(|arg| arg == "import") {
        import(numbers.first().copied().unwrap_or(5000).max(1) as u32);
        return;
    }
    if args.iter().any(|arg| arg == "leveling") {
        leveling(500);
        return;
    }
    if args.iter().any(|arg| arg == "suggest") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        suggestions(people, tasks);
        return;
    }
    if args.iter().any(|arg| arg == "forecast") {
        let people = numbers.first().copied().unwrap_or(500).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(5000) as u32;
        let runs = numbers.get(2).copied().unwrap_or(1000) as u32;
        finish_forecast(people, tasks, runs);
        return;
    }
    if args.iter().any(|arg| arg == "capacity") {
        let people = numbers.first().copied().unwrap_or(100).max(1) as u32;
        let tasks = numbers.get(1).copied().unwrap_or(1000) as u32;
        let has = |name: &str| args.iter().any(|arg| arg == name);
        capacity(people, tasks, has("field"), has("leave"), has("interrupt"), has("linked"));
        return;
    }
    let args = numbers;
    let tasks = args.first().copied().unwrap_or(1000) as u32;
    let total = args.get(1).copied().unwrap_or(10_000);
    let commands = build_commands(tasks, total);
    println!("{tasks} công việc, {} lệnh", commands.len());

    let mut doc = Document::from_project(Project::new("Bench", timestamp(2026, 1, 5, 8, 0)));
    let started = Instant::now();
    let mut slowest = Duration::ZERO;
    for command in commands {
        let one = Instant::now();
        doc.apply(command).expect("command applies");
        slowest = slowest.max(one.elapsed());
    }
    println!(
        "{:<28} {:>10.1} ms",
        "apply (tổng)",
        started.elapsed().as_secs_f64() * 1000.0
    );
    println!(
        "{:<28} {:>10.1} ms",
        "apply (chậm nhất)",
        slowest.as_secs_f64() * 1000.0
    );

    let json = serde_json::to_string(&doc.project).expect("json");
    println!(
        "{:<28} {:>10.1} KB",
        "Project JSON",
        json.len() as f64 / 1024.0
    );
    timed("undo", || doc.undo().expect("undo"));
    timed("undo lần 2", || doc.undo().expect("undo"));
    timed("redo", || doc.redo().expect("redo"));
    timed("apply sau undo", || {
        doc.apply(Command::RenameTask {
            uid: 2,
            name: "Sau undo".into(),
        })
        .expect("apply")
    });

    let dir = std::env::temp_dir().join(format!("sheafly-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("bench.pdproj").to_string_lossy().to_string();
    timed("save", || save(&doc, &path).expect("save"));
    let (mut loaded, _) = timed("load", || load(&path).expect("load"));
    assert_eq!(loaded.project.tasks.len(), doc.project.tasks.len());
    let mut slowest = Duration::ZERO;
    for _ in 0..50 {
        let one = Instant::now();
        loaded.undo().expect("undo");
        slowest = slowest.max(one.elapsed());
    }
    println!(
        "{:<28} {:>10.1} ms",
        "undo sau khi mở (chậm nhất/50)",
        slowest.as_secs_f64() * 1000.0
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The finish forecast on the linked capacity project, every task with an effort range
/// from half to twice its effort, grouped by a field with medium correlation.
fn finish_forecast(people: u32, tasks: u32, runs: u32) {
    let mut project = capacity(people, tasks, false, false, false, true);
    for task in &mut project.tasks {
        task.work_low = Some(task.work / 2);
        task.work_high = Some(task.work * 2);
    }
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let threads = std::thread::available_parallelism().map_or(1, |count| count.get());
    let started = Instant::now();
    let result = forecast(&project, runs, &cancel).expect("forecast");
    println!(
        "{:<28} {:>10.1} ms  ({runs} vòng, {threads} luồng)",
        "dự báo",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let started = Instant::now();
    let _ = forecast_with_threads(&project, runs.min(200), 1, &cancel).expect("forecast");
    println!(
        "{:<28} {:>10.1} ms  ({} vòng, 1 luồng)",
        "dự báo",
        started.elapsed().as_secs_f64() * 1000.0,
        runs.min(200)
    );
    let hours = |ms: i64| (ms - result.leveled_finish) as f64 / 3_600_000.0;
    println!(
        "{:<28} +{:.0} h khi cân bằng",
        "ngày xong theo kế hoạch",
        (result.leveled_finish - result.planned_finish) as f64 / 3_600_000.0
    );
    println!(
        "{:<28} P50 +{:.0} h, P80 +{:.0} h, P90 +{:.0} h so với đã cân bằng",
        "ngày xong dự án",
        hours(result.finish.p50),
        hours(result.finish.p80),
        hours(result.finish.p90)
    );
}

/// The AI project summary on the linked capacity project, once leveled and once with the
/// leveling cleared so people are overloaded: cut to the default limit, masked, and whole.
fn project_summaries(people: u32, tasks: u32) {
    use sheafly_engine::summary::{project_summary, SummaryOptions};
    let mut doc = Document::from_project(capacity(people, tasks, false, true, false, true));
    for (label, clear) in [("đã cân bằng", false), ("quá tải", true)] {
        if clear {
            doc.apply(Command::ClearLeveling).expect("clear leveling");
        }
        let project = &doc.project;
        println!("{label}: {} khoảng quá tải", project.overallocations.len());
        let (cut, _) = timed("project_summary (giới hạn)", || {
            project_summary(project, &SummaryOptions::default())
        });
        let json = serde_json::to_string(&cut).expect("json");
        println!(
            "{:<28} {:>10} việc, {} người, {:.1} KB",
            "giữ lại",
            cut.tasks.len(),
            cut.resources.len(),
            json.len() as f64 / 1024.0
        );
        timed("project_summary (ẩn tên)", || {
            project_summary(project, &SummaryOptions { mask: true, ..SummaryOptions::default() })
        });
        let (whole, _) = timed("project_summary (không cắt)", || {
            project_summary(project, &SummaryOptions { max_chars: usize::MAX, ..SummaryOptions::default() })
        });
        let json = serde_json::to_string(&whole).expect("json");
        println!("{:<28} {:>10.1} KB", "summary đầy đủ", json.len() as f64 / 1024.0);
    }
}
