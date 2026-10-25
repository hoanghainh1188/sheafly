//! Cutting the project summary to `max_chars` (decision D7), in three passes so neither
//! people nor tasks crowd the other out:
//! 1. people, overallocated first then by assigned work, until they take a third of the room
//!    left after the fixed part;
//! 2. tasks that are late or miss a deadline, critical, milestones, with a deadline, on
//!    overallocated people, the rest by slack, finished ones last, until one does not fit;
//! 3. the remaining people, then the remaining groups, until one does not fit.
//!
//! A task brings the groups above it, so every `parent` it names is in the summary. Kept
//! lines go back in project order. The fixed part is measured with the largest counts it
//! can show, so the JSON never exceeds the limit unless the fixed part alone does.

use std::collections::HashSet;

use crate::model::{Project, Task};
use crate::summary::{is_open, Lookup, ProjectSummary, ResourceLine, TaskLine};

/// Each list in the order lines should go in.
struct Order {
    people: Vec<usize>,
    leaves: Vec<usize>,
    groups: Vec<usize>,
}

/// The lines kept so far and the characters they take.
struct Budget<'a> {
    project: &'a Project,
    lookup: &'a Lookup,
    task_len: Vec<usize>,
    resource_len: Vec<usize>,
    kept_tasks: Vec<bool>,
    kept_resources: Vec<bool>,
    used: usize,
}

impl Budget<'_> {
    /// Keep a person when it fits under `limit`.
    fn person(&mut self, at: usize, limit: usize) -> bool {
        if self.kept_resources[at] {
            return true;
        }
        // Each line also takes a comma, except the first of its list.
        let cost = self.resource_len[at] + 1;
        if self.used + cost > limit {
            return false;
        }
        self.used += cost;
        self.kept_resources[at] = true;
        true
    }

    /// Keep a task and the groups above it when they all fit under `limit`.
    fn task(&mut self, at: usize, limit: usize) -> bool {
        let mut adding = Vec::new();
        let mut next = Some(at);
        while let Some(at) = next {
            if !self.kept_tasks[at] {
                adding.push(at);
            }
            next = self.project.tasks[at]
                .parent_uid
                .and_then(|uid| self.lookup.index.get(&uid).copied());
        }
        let cost: usize = adding.iter().map(|at| self.task_len[*at] + 1).sum();
        if self.used + cost > limit {
            return false;
        }
        self.used += cost;
        for at in adding {
            self.kept_tasks[at] = true;
        }
        true
    }
}

pub(crate) fn fit(
    project: &Project,
    lookup: &Lookup,
    mut summary: ProjectSummary,
    tasks: Vec<TaskLine>,
    resources: Vec<ResourceLine>,
    max_chars: usize,
) -> ProjectSummary {
    summary.truncated = false;
    summary.omitted_tasks = tasks.len();
    summary.omitted_resources = resources.len();
    let fixed = json_len(&summary);
    let mut budget = Budget {
        project,
        lookup,
        task_len: tasks.iter().map(json_len).collect(),
        resource_len: resources.iter().map(json_len).collect(),
        kept_tasks: vec![false; tasks.len()],
        kept_resources: vec![false; resources.len()],
        used: fixed,
    };
    let order = order(project, lookup);

    let people_limit = fixed + max_chars.saturating_sub(fixed) / 3;
    let first_people = order
        .people
        .iter()
        .take_while(|at| budget.person(**at, people_limit))
        .count();
    let all_tasks = order.leaves.iter().all(|at| budget.task(*at, max_chars));
    let rest = all_tasks
        && order.people[first_people..]
            .iter()
            .all(|at| budget.person(*at, max_chars))
        && order.groups.iter().all(|at| budget.task(*at, max_chars));

    let Budget {
        kept_tasks,
        kept_resources,
        ..
    } = budget;
    summary.tasks = keep(tasks, &kept_tasks);
    summary.resources = keep(resources, &kept_resources);
    summary.omitted_tasks = kept_tasks.len() - summary.tasks.len();
    summary.omitted_resources = kept_resources.len() - summary.resources.len();
    summary.truncated = !rest;
    summary
}

fn keep<T>(lines: Vec<T>, kept: &[bool]) -> Vec<T> {
    lines
        .into_iter()
        .zip(kept)
        .filter(|(_, kept)| **kept)
        .map(|(line, _)| line)
        .collect()
}

fn json_len<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_string(value).map_or(0, |text| text.len())
}

fn order(project: &Project, lookup: &Lookup) -> Order {
    let mut people: Vec<usize> = (0..project.resources.len()).collect();
    let work = |at: &usize| {
        lookup
            .work
            .get(&project.resources[*at].uid)
            .copied()
            .unwrap_or(0)
    };
    people.sort_by_key(|at| {
        (
            !lookup.overallocated(project.resources[*at].uid),
            std::cmp::Reverse(work(at)),
            *at,
        )
    });

    let busy: HashSet<u32> = lookup.overloaded.keys().copied().collect();
    let mut leaves: Vec<usize> = (0..project.tasks.len())
        .filter(|at| !project.tasks[*at].summary)
        .collect();
    leaves.sort_by_key(|at| {
        let task = &project.tasks[*at];
        (
            rank(task, lookup, &busy),
            task.total_slack,
            task.start,
            task.uid,
        )
    });
    let groups = (0..project.tasks.len())
        .filter(|at| project.tasks[*at].summary)
        .collect();
    Order {
        people,
        leaves,
        groups,
    }
}

fn rank(task: &Task, lookup: &Lookup, busy: &HashSet<u32>) -> u8 {
    if !is_open(task) {
        return 6;
    }
    let misses = task.deadline.is_some_and(|deadline| task.finish > deadline);
    let on_busy = lookup
        .people
        .get(&task.uid)
        .is_some_and(|uids| uids.iter().any(|uid| busy.contains(uid)));
    match () {
        _ if task.late || misses => 0,
        _ if task.critical => 1,
        _ if task.milestone => 2,
        _ if task.deadline.is_some() => 3,
        _ if on_busy => 4,
        _ => 5,
    }
}
