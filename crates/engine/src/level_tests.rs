//! Unit tests of `level.rs`.

use super::queue::Key;
use super::*;
use crate::level_order::{self, Parts};

fn piece(start: i64, end: i64, units: f64) -> Piece {
    Piece { start, end, units, owner: NO_OWNER }
}

/// The default order must place tasks exactly as the key leveling had before the
/// order could be set: (levelable, waiting, start, -priority, slack, uid).
#[test]
fn the_default_order_is_the_old_key() {
    let mut seed: u64 = 7;
    let mut random = |below: u64| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) % below) as i64
    };
    let mut project = Project::new("Khóa", 0);
    let keys = level_order::default_order();
    let mut both = Vec::new();
    for uid in 1..=400 {
        let mut task = Task::new(uid, "Việc");
        task.start = random(6) * 3_600_000;
        task.priority = [0, 300, 500, 500, 700, 1000][random(6) as usize];
        task.manual = random(8) == 0;
        let slack = random(5) * 60 - 120;
        let waiting = random(4) == 0;
        let old = (levelable(&task), waiting, task.start, -task.priority, slack, task.uid);
        let new = Key {
            levelable: levelable(&task),
            waiting,
            start: task.start,
            parts: Parts::of(&project, &keys, &task, slack),
            uid: task.uid,
        };
        project.tasks.push(task);
        both.push((old, new));
    }
    for (old_a, new_a) in &both {
        for (old_b, new_b) in &both {
            assert_eq!(old_a.cmp(old_b), new_a.cmp(new_b), "{old_a:?} against {old_b:?}");
        }
    }
}

/// A leave is passed in one go, not one working range at a time.
#[test]
fn a_long_leave_takes_few_rounds() {
    use crate::availability::AvailabilityPeriod;
    use crate::model::{Assignment, ResourceKind};
    let monday = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    let mut project = Project::new("Nghỉ", compose(monday, 480));
    let mut resource = Resource::new(1, "An", ResourceKind::Person);
    resource.availability = vec![AvailabilityPeriod {
        from: Some(monday),
        to: Some(monday + chrono::Duration::days(180)),
        units: 0.0,
    }];
    project.resources.push(resource);
    project.tasks.push(Task::new(1, "Việc"));
    project.assignments.push(Assignment {
        uid: 1,
        task_uid: 1,
        resource_uid: 1,
        units: 1.0,
        work: 480,
        overtime_work: 0,
        actual_work: 0,
        remaining_work: 480,
        cost: 0.0,
        actual_cost: 0.0,
        manual_actual_cost: None,
        baselines: Vec::new(),
    });
    crate::schedule::schedule(&mut project).unwrap();
    ROUNDS.with(|count| count.set(0));
    level_project(&mut project).unwrap();
    let rounds = ROUNDS.with(std::cell::Cell::get);
    assert!(rounds <= 3, "{rounds} rounds");
    let back = monday + chrono::Duration::days(182);
    assert_eq!(project.tasks[0].start, compose(back, 480));
}

#[test]
fn stretches_follow_load_and_availability() {
    let day = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
    let at = |minute: u16| compose(day, minute);
    let pieces = [piece(at(480), at(720), 1.0), piece(at(600), at(720), 0.5)];
    let found = stretches(&pieces, |_| vec![(at(480), at(720))], |_| 1.0);
    let summary: Vec<(i64, f64, f64)> = found
        .iter()
        .map(|s| (s.start, s.load, s.available))
        .collect();
    assert_eq!(summary, vec![(at(480), 1.0, 1.0), (at(600), 1.5, 1.0)]);
    assert!(!found[0].over());
    assert!(found[1].over());
}
