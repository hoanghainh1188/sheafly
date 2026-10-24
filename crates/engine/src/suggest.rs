//! Who should take each unassigned task, from the project's assignment rules.
//!
//! A read: suggestions never enter the command log. Accepting them sends `AssignTasks`,
//! so replay stays deterministic whatever the rules or loads were.
//!
//! Tasks are taken in the project's leveling order, so the work that comes first gets the
//! free people first. For each, the first matching rule names one person or a skill; each
//! candidate is dated as if assigned at the rule's units (the task type decides the
//! duration, as assigning would) and placed at the first moment their load, calendar and
//! capacity by date leave room, with leveling's own search. Candidates rank by the
//! earliest finish, then less assigned work, then the lower uid. The top one is taken to
//! have the task before the next task is ranked (greedy, like LiquidPlanner), so a batch
//! of tasks with one skill spreads over the people who have it. Links between suggested
//! tasks are not followed: the dates only rank people, and leveling after the assignments
//! gives the real ones.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::level::{levelable, Loads};
use crate::level_order::{self, Parts};
use crate::model::{ConstraintType, Project, Task};
use crate::skill_levels;
use crate::skills::{rule_for, RuleTarget};
use crate::split;
use crate::focus::focus_of_uid;
use crate::work::with_first_assignment;

/// People offered for one task at most.
pub const MAX_PICKS: usize = 5;

#[cfg(test)]
thread_local! {
    /// Tests place every candidate, to check that stopping early changes nothing.
    static PLACE_ALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the candidates past `bound` can be left unplaced.
fn may_stop() -> bool {
    #[cfg(test)]
    if PLACE_ALL.with(|all| all.get()) {
        return false;
    }
    true
}

/// One person offered for a task, with the dates they could do it in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub resource_uid: u32,
    pub start: i64,
    pub finish: i64,
}

/// The people offered for one unassigned task, best first, or why there are none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub task_uid: u32,
    /// Index of the rule that matched.
    pub rule: Option<usize>,
    /// Units the assignment would have: the rule's.
    pub units: f64,
    pub picks: Vec<Pick>,
    pub reason: Option<NoPick>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoPick {
    /// No rule matches the task.
    NoRule,
    /// The rule names a skill no resource has.
    NoOneWithSkill,
    /// Nobody the rule names has room for it within four years, or ever has its units.
    NoRoom,
}

/// A suggestion for every unassigned leaf task not started yet that is neither a
/// milestone nor an occurrence of a series, in the order they were considered.
pub fn suggest(project: &Project) -> Vec<Suggestion> {
    let mut loads = Loads::of(project);
    let mut assigned_work: HashMap<u32, i64> = HashMap::new();
    let mut staffed: HashSet<u32> = HashSet::new();
    for assignment in &project.assignments {
        *assigned_work.entry(assignment.resource_uid).or_insert(0) += assignment.work;
        staffed.insert(assignment.task_uid);
    }
    let mut suggestions = Vec::new();
    for task in open_tasks(project, &staffed) {
        let Some(rule) = rule_for(project, task) else {
            suggestions.push(Suggestion {
                task_uid: task.uid,
                rule: None,
                units: 0.0,
                picks: Vec::new(),
                reason: Some(NoPick::NoRule),
            });
            continue;
        };
        let units = project.assignment_rules[rule].units;
        let candidates: Vec<u32> = match project.assignment_rules[rule].target {
            RuleTarget::Resource { uid } => vec![uid],
            RuleTarget::Skill { id, min_level } => project
                .resources
                .iter()
                .filter(|resource| skill_levels::qualifies(project, resource, id, min_level))
                .map(|resource| resource.uid)
                .collect(),
        };
        let mut trial = task.clone();
        let ranked = rank(
            project,
            &loads,
            &assigned_work,
            &mut trial,
            &candidates,
            units,
        );
        let reason = match (ranked.first(), candidates.is_empty()) {
            (Some(_), _) => None,
            (None, true) => Some(NoPick::NoOneWithSkill),
            (None, false) => Some(NoPick::NoRoom),
        };
        if let Some(top) = ranked.first() {
            if levelable(&trial) {
                date(project, &mut trial, top.pick.start, top.duration);
            } else {
                // Its dates are its own; only the pick's duration can change them.
                date_pinned(project, &mut trial, top.duration);
            }
            loads.add(project, &trial, top.pick.resource_uid, units);
            *assigned_work.entry(top.pick.resource_uid).or_insert(0) += top.work;
        }
        suggestions.push(Suggestion {
            task_uid: task.uid,
            rule: Some(rule),
            units,
            picks: ranked.into_iter().map(|ranked| ranked.pick).collect(),
            reason,
        });
    }
    suggestions
}

/// The tasks to suggest for, in the project's leveling order.
fn open_tasks<'a>(project: &'a Project, staffed: &HashSet<u32>) -> Vec<&'a Task> {
    let keys = level_order::effective(project);
    let mut open: Vec<(Parts, &Task)> = project
        .tasks
        .iter()
        .filter(|task| {
            !task.summary
                && !task.milestone
                && task.series_uid.is_none()
                && task.actual_start.is_none()
                && task.percent_complete <= 0.0
                && !staffed.contains(&task.uid)
        })
        .map(|task| (Parts::of(project, &keys, task, task.total_slack), task))
        .collect();
    // As leveling does: tasks it would not move first, since the others fit around them.
    open.sort_by(|(a, task_a), (b, task_b)| {
        levelable(task_a)
            .cmp(&levelable(task_b))
            .then_with(|| a.cmp(task_a.start, b, task_b.start))
            .then(task_a.uid.cmp(&task_b.uid))
    });
    open.into_iter().map(|(_, task)| task).collect()
}

/// A candidate placed, with what decides its rank.
struct Ranked {
    pick: Pick,
    work: i64,
    duration: i64,
    /// Work already assigned to the person.
    load: i64,
}

impl Ranked {
    fn key(&self) -> (i64, i64, u32) {
        (self.pick.finish, self.load, self.pick.resource_uid)
    }
}

/// The best `MAX_PICKS` candidates who have room, best first. `trial` is the task, whose
/// dates and duration are overwritten.
///
/// A task leveling would not move (pinned by hand, by an actual start or a hard date, or
/// marked not to level) keeps its dates: only people with room at those dates are kept.
///
/// Others slide to where each candidate has room. Nobody can start before the first
/// moment they have room for the task's units, so candidates are tried in the order of
/// the finish from that moment; once the last kept pick ranks before the next bound, the
/// rest cannot do better and are not placed. Work with no working time fits anywhere at
/// once, so it has no such bound.
fn rank(
    project: &Project,
    loads: &Loads,
    assigned_work: &HashMap<u32, i64>,
    trial: &mut Task,
    candidates: &[u32],
    units: f64,
) -> Vec<Ranked> {
    // Each candidate works at their own rate (`focus.rs`), so durations differ.
    let base = trial.clone();
    let sized = |uid: u32| with_first_assignment(&base, units * focus_of_uid(project, uid));
    let start = trial.start;
    let load_of = |uid: u32| assigned_work.get(&uid).copied().unwrap_or(0);
    if !levelable(trial) {
        let mut kept: Vec<Ranked> = Vec::new();
        for &uid in candidates {
            let (duration, work) = sized(uid);
            *trial = base.clone();
            date_pinned(project, trial, duration);
            if !loads.fits_at(project, trial, uid, units) {
                continue;
            }
            kept.push(Ranked {
                pick: Pick {
                    resource_uid: uid,
                    start: trial.start,
                    finish: trial.finish,
                },
                work,
                duration,
                load: load_of(uid),
            });
        }
        *trial = base.clone();
        kept.sort_unstable_by_key(Ranked::key);
        kept.truncate(MAX_PICKS);
        return kept;
    }
    let mut order: Vec<(i64, i64, u32)> = candidates
        .iter()
        .map(|&uid| {
            let (duration, _) = sized(uid);
            date(project, trial, start, duration);
            // Work with no working time fits anywhere at once: its finish is its bound.
            if trial.finish > trial.start {
                let room = loads.earliest_room(project, uid, units, start);
                date(project, trial, room, duration);
            }
            (trial.finish, load_of(uid), uid)
        })
        .collect();
    order.sort_unstable();
    let mut kept: Vec<Ranked> = Vec::new();
    for (bound, load, uid) in order {
        if kept.len() >= MAX_PICKS && kept[MAX_PICKS - 1].key() < (bound, load, uid) && may_stop() {
            break;
        }
        let (duration, work) = sized(uid);
        trial.start = start;
        date(project, trial, start, duration);
        let Some(fit) = loads.fit(project, trial, uid, units) else {
            continue;
        };
        date(project, trial, fit, duration);
        let ranked = Ranked {
            pick: Pick {
                resource_uid: uid,
                start: trial.start,
                finish: trial.finish,
            },
            work,
            duration,
            load,
        };
        let at = kept.partition_point(|other| other.key() < ranked.key());
        kept.insert(at, ranked);
        kept.truncate(MAX_PICKS);
    }
    trial.start = start;
    kept
}

/// Date a task leveling would not move: its own dates while the assignment leaves the
/// duration as it is, else the new duration from its start, or up to its finish when the
/// finish is the fixed date.
fn date_pinned(project: &Project, task: &mut Task, duration: i64) {
    if duration == task.duration {
        return;
    }
    let start = if task.constraint_type == ConstraintType::Mfo {
        project
            .task_calendar(task)
            .subtract_working(task.finish, duration)
    } else {
        task.start
    };
    date(project, task, start, duration);
}

/// Give `task` its dates from `start` with `duration`, keeping its splits.
fn date(project: &Project, task: &mut Task, start: i64, duration: i64) {
    let calendar = project.task_calendar(task);
    let segments = split::layout(calendar, start, duration, &task.splits);
    task.duration = duration;
    task.start = segments.first().map_or(start, |segment| segment.start);
    task.finish = segments.last().map_or(start, |segment| segment.finish);
    task.segments = if task.splits.is_empty() {
        Vec::new()
    } else {
        segments
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::availability::AvailabilityPeriod;
    use crate::calendar::TimeRange;
    use crate::command::Command;
    use crate::document::Document;
    use crate::fields::{FieldKind, FieldValue};
    use crate::model::{ConstraintType, ResourceKind, TaskType};
    use crate::skills::{AssignmentRule, Condition};
    use crate::timeutil::timestamp;

    /// A project of people with one or two of three skills, some work already assigned
    /// (some of it later on), leaves, and rules from a list field to the skills.
    fn generated(seed: u64) -> Project {
        let mut seed = seed;
        let mut random = |below: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % below
        };
        let mut doc = Document::from_project(Project::new("Sinh", timestamp(2026, 1, 5, 8, 0)));
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let apply = |doc: &mut Document, command: Command| {
            doc.apply(command).expect("generated command applies");
        };
        for name in ["A", "B", "C"] {
            apply(&mut doc, Command::AddSkill { name: name.into() });
        }
        // A part-time calendar (mornings) some people work on.
        apply(
            &mut doc,
            Command::CreateCalendar {
                name: "Sáng".into(),
                base_id: None,
            },
        );
        let mornings = doc.project.calendars.last().expect("calendar").id;
        let morning = vec![TimeRange {
            start_minute: 480,
            end_minute: 720,
        }];
        let week = (0..7)
            .map(|day| {
                if (1..=5).contains(&day) {
                    morning.clone()
                } else {
                    Vec::new()
                }
            })
            .collect();
        apply(
            &mut doc,
            Command::SetCalendarWeek {
                calendar_id: mornings,
                week,
            },
        );
        let people = 4 + random(8) as u32;
        for uid in 1..=people {
            apply(
                &mut doc,
                Command::AddResource {
                    name: format!("P{uid}"),
                    kind: ResourceKind::Person,
                },
            );
            let mut skills = vec![1 + random(3) as u32];
            if random(2) == 0 {
                skills.push(1 + random(3) as u32);
            }
            apply(&mut doc, Command::SetResourceSkills { uid, skills, levels: Vec::new() });
            if random(4) == 0 {
                let resource = doc.project.resources.last().expect("resource").clone();
                apply(
                    &mut doc,
                    Command::UpdateResource {
                        uid,
                        name: resource.name,
                        kind: resource.kind,
                        max_units: resource.max_units,
                        calendar_id: Some(mornings),
                        standard_rate: 0.0,
                        overtime_rate: 0.0,
                        cost_per_use: 0.0,
                    },
                );
            }
            if random(3) == 0 {
                let from = monday + chrono::Duration::days(random(10) as i64);
                let to = from + chrono::Duration::days(random(8) as i64);
                let units = [0.0, 0.5][random(2) as usize];
                let periods = vec![AvailabilityPeriod {
                    from: Some(from),
                    to: Some(to),
                    units,
                }];
                apply(&mut doc, Command::SetResourceAvailability { uid, periods });
            }
        }
        let options = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        apply(
            &mut doc,
            Command::AddField {
                name: "Mảng".into(),
                kind: FieldKind::Choice { options },
            },
        );
        let tasks = 10 + random(30) as u32;
        for uid in 2..=tasks {
            apply(
                &mut doc,
                Command::AddTask {
                    name: format!("T{uid}"),
                    after_uid: None,
                },
            );
        }
        for uid in 1..=tasks {
            apply(
                &mut doc,
                Command::SetDuration {
                    uid,
                    minutes: 60 * (1 + random(24) as i64),
                },
            );
            if random(3) == 0 {
                apply(
                    &mut doc,
                    Command::SetTaskType {
                        uid,
                        task_type: TaskType::FixedWork,
                        effort_driven: true,
                    },
                );
                // Work with no working time: it fits anywhere at once.
                if random(4) == 0 {
                    apply(&mut doc, Command::SetWork { uid, minutes: 0 });
                    apply(&mut doc, Command::SetDuration { uid, minutes: 60 });
                }
            }
            if random(4) == 0 {
                let date = Some(timestamp(2026, 1, 5 + random(10) as u32, 8, 0));
                // A start-on-date task stays put; the others may slide.
                let constraint_type =
                    [ConstraintType::Snet, ConstraintType::Mso][random(2) as usize];
                apply(
                    &mut doc,
                    Command::SetConstraint {
                        uid,
                        constraint_type,
                        date,
                    },
                );
            }
            if random(4) == 0 {
                let resource_uid = 1 + random(u64::from(people)) as u32;
                apply(
                    &mut doc,
                    Command::AssignResource {
                        task_uid: uid,
                        resource_uid,
                        units: 1.0,
                    },
                );
            } else if random(5) > 0 {
                let value = Some(FieldValue::Text(["a", "b", "c"][random(3) as usize].into()));
                apply(
                    &mut doc,
                    Command::SetFieldValue {
                        uid,
                        field_id: 1,
                        value,
                    },
                );
            }
        }
        let mut rules: Vec<AssignmentRule> = (0..3)
            .map(|index| AssignmentRule {
                conditions: vec![Condition {
                    field: 1,
                    values: vec![FieldValue::Text(["a", "b", "c"][index].into())],
                }],
                target: RuleTarget::Skill {
                    id: 1 + index as u32,
                    min_level: None,
                },
                units: [1.0, 0.5, 2.0][random(3) as usize],
            })
            .collect();
        rules.push(AssignmentRule {
            conditions: vec![],
            target: RuleTarget::Skill { id: 1, min_level: None },
            units: 1.0,
        });
        apply(&mut doc, Command::SetAssignmentRules { rules });
        doc.project
    }

    #[test]
    fn stopping_early_gives_what_placing_everyone_gives() {
        let mut stopped = 0;
        for seed in 0..120 {
            let project = generated(seed);
            let quick = suggest(&project);
            PLACE_ALL.with(|all| all.set(true));
            let full = suggest(&project);
            PLACE_ALL.with(|all| all.set(false));
            assert_eq!(quick, full, "seed {seed}");
            stopped += usize::from(
                quick
                    .iter()
                    .any(|suggestion| suggestion.picks.len() == MAX_PICKS),
            );
        }
        // The early stop has to be tried for the check to mean anything.
        assert!(stopped > 20, "{stopped}");
    }
}
