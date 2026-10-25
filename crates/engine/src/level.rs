//! Resource overallocation and leveling.
//!
//! A resource is overallocated when, at some instant, the units of the assignments that
//! are working then add up to more than it has: its max units while its own calendar
//! works, nothing otherwise. Work counts only where the task's calendar works and inside
//! the task's segments, so two tasks on the same day at different hours do not clash,
//! and a split gap carries no load.
//!
//! Each resource is swept along time: load and availability change only at piece and
//! calendar boundaries, so the elementary stretches between boundaries have one load
//! and one availability each.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;

use crate::availability::Capacity;
use crate::calendar::Calendar;
use crate::error::EngineError;
use crate::graph::Graph;
use crate::level_cycle::{midnight_of, weekly_from, Cycle};
use crate::model::{ConstraintType, Overallocation, Project, Resource, Task};
use crate::ripple::Ripple;
use crate::schedule::{prepare, publish_derived, schedule_dates};
use crate::split;
use crate::timeutil::{civil, compose, date_key};

/// Units below this are rounding noise.
const EPSILON: f64 = 1e-6;

#[cfg(test)]
thread_local! {
    /// Tests turn off ending a search that has come round, to check it changes nothing.
    pub(crate) static SEARCH_IN_FULL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Searches that came round (or would have, with `SEARCH_IN_FULL`).
    pub(crate) static CAME_ROUND: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Rounds of every search for a place.
    pub(crate) static ROUNDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn publish_overallocations(project: &mut Project) {
    project.overallocations = find_overallocations(project);
}

/// One row per resource and day with an overload: the peak units that day and what the
/// resource had at that moment.
pub fn find_overallocations(project: &Project) -> Vec<Overallocation> {
    let mut days: BTreeMap<(u32, NaiveDate), (f64, f64)> = BTreeMap::new();
    for sweep in sweeps(project) {
        for stretch in sweep.stretches.iter().filter(|stretch| stretch.over()) {
            let (date, _) = civil(stretch.start);
            let entry = days
                .entry((sweep.resource_uid, date))
                .or_insert((stretch.load, stretch.available));
            if stretch.load > entry.0 {
                *entry = (stretch.load, stretch.available);
            }
        }
    }
    days.into_iter()
        .map(
            |((resource_uid, date), (units, max_units))| Overallocation {
                resource_uid,
                date: date_key(date),
                units,
                max_units,
            },
        )
        .collect()
}

/// Work above what each person has, in working minutes, by resource uid: the units over
/// their availability times the length of every overallocated stretch, from the same sweep
/// as `find_overallocations`. People with no overload are left out.
pub fn over_minutes(project: &Project) -> BTreeMap<u32, i64> {
    sweeps(project)
        .into_iter()
        .filter_map(|sweep| {
            let excess: f64 = sweep
                .stretches
                .iter()
                .filter(|stretch| stretch.over())
                .map(|stretch| (stretch.load - stretch.available) * (stretch.end - stretch.start) as f64 / 60_000.0)
                .sum();
            let minutes = excess.round() as i64;
            (minutes > 0).then_some((sweep.resource_uid, minutes))
        })
        .collect()
}

/// A piece of one assignment's work inside one calendar range of one day.
#[derive(Clone, Copy)]
struct Piece {
    start: i64,
    end: i64,
    units: f64,
    /// Position of the task the work belongs to, or `NO_OWNER` where it does not matter.
    owner: usize,
}

/// The owner of pieces that only count load (overallocation, a task being tried).
const NO_OWNER: usize = usize::MAX;

/// A stretch of time with one load and one availability.
struct Stretch {
    start: i64,
    end: i64,
    load: f64,
    available: f64,
}

impl Stretch {
    fn over(&self) -> bool {
        self.load > self.available + EPSILON
    }
}

struct Sweep {
    resource_uid: u32,
    stretches: Vec<Stretch>,
}

fn sweeps(project: &Project) -> Vec<Sweep> {
    let mut pieces: BTreeMap<u32, Vec<Piece>> = BTreeMap::new();
    for assignment in &project.assignments {
        if assignment.units <= 0.0 {
            continue;
        }
        let Some(task) = project.task(assignment.task_uid) else {
            continue;
        };
        if task.summary || task.finish <= task.start {
            continue;
        }
        let into = pieces.entry(assignment.resource_uid).or_default();
        working_pieces(project, task, assignment.units, NO_OWNER, into);
    }
    pieces
        .into_iter()
        .filter_map(|(resource_uid, pieces)| {
            let resource = project.resources.iter().find(|r| r.uid == resource_uid)?;
            let calendar = project.resource_calendar(resource);
            let capacity = Capacity::of(resource);
            let stretches = stretches(
                &pieces,
                |date| working_ranges(calendar, date),
                |date| capacity.units_on(date),
            );
            Some(Sweep {
                resource_uid,
                stretches,
            })
        })
        .collect()
}

/// The task's work cut at its own calendar's ranges, day by day.
fn working_pieces(project: &Project, task: &Task, units: f64, owner: usize, into: &mut Vec<Piece>) {
    let calendar = project.task_calendar(task);
    cut_to_calendar(calendar, &split::work_intervals(task), units, owner, into);
}

fn cut_to_calendar(
    calendar: &Calendar,
    intervals: &[(i64, i64)],
    units: f64,
    owner: usize,
    into: &mut Vec<Piece>,
) {
    for &(low, high) in intervals {
        if high <= low {
            continue;
        }
        let (mut date, _) = civil(low);
        let (last, _) = civil(high);
        loop {
            for range in calendar.ranges_on(date) {
                let start = low.max(compose(date, range.start_minute));
                let end = high.min(compose(date, range.end_minute));
                if end > start {
                    into.push(Piece { start, end, units, owner });
                }
            }
            match date.succ_opt() {
                Some(next) if date < last => date = next,
                _ => break,
            }
        }
    }
}

/// Load and availability between every pair of consecutive boundaries, in time order,
/// from the first day with work to the last. The resource has `units_on` a day while
/// its calendar works that day.
fn stretches(
    pieces: &[Piece],
    ranges_on: impl Fn(NaiveDate) -> Vec<(i64, i64)>,
    units_on: impl Fn(NaiveDate) -> f64,
) -> Vec<Stretch> {
    // (instant, load change, availability change)
    let mut events: Vec<(i64, f64, f64)> = Vec::with_capacity(pieces.len() * 2);
    for piece in pieces {
        events.push((piece.start, piece.units, 0.0));
        events.push((piece.end, -piece.units, 0.0));
    }
    let first = pieces.iter().map(|piece| civil(piece.start).0).min();
    let last = pieces.iter().map(|piece| civil(piece.start).0).max();
    if let (Some(mut date), Some(last)) = (first, last) {
        loop {
            let units = units_on(date);
            for (start, end) in ranges_on(date) {
                events.push((start, 0.0, units));
                events.push((end, 0.0, -units));
            }
            match date.succ_opt() {
                Some(next) if date < last => date = next,
                _ => break,
            }
        }
    }
    events.sort_by_key(|event| event.0);
    let mut stretches = Vec::new();
    let (mut load, mut available) = (0.0f64, 0.0f64);
    let mut index = 0;
    while index < events.len() {
        let at = events[index].0;
        while index < events.len() && events[index].0 == at {
            load += events[index].1;
            available += events[index].2;
            index += 1;
        }
        if let Some(next) = events.get(index).map(|event| event.0) {
            if next > at {
                stretches.push(Stretch {
                    start: at,
                    end: next,
                    load: load.max(0.0),
                    available: available.max(0.0),
                });
            }
        }
    }
    stretches
}

/// Serial leveling, close to Project's. Tasks are placed one at a time: those leveling
/// will not delay first, then tasks whose predecessors are placed, then by the project's
/// leveling order (`level_order.rs`; by default earliest start, higher priority, less
/// total slack before leveling). Each goes to the first moment
/// every one of its resources has room for it for its whole duration, next to the tasks
/// already placed, and placing it reschedules the dates so its successors move before
/// they are placed. Placed tasks do not move again, so no idle gap is left that a later
/// choice could fill.
///
/// Writes the result into `leveling_delay` and, only to let an urgent task in
/// (`level_interrupt.rs`), into splits marked as leveling's; otherwise existing splits
/// move with the task. Tasks pinned by hand, by an actual start, or by a must-start/finish-on
/// date, and tasks with priority 1000, stay where they are.
pub fn level_project(project: &mut Project) -> Result<(), EngineError> {
    level_with(project, true)
}

/// `level_project`, with the quick reschedule allowed or not; tests check both agree.
pub(crate) fn level_with(project: &mut Project, quick: bool) -> Result<(), EngineError> {
    for task in &mut project.tasks {
        if movable(task) {
            task.leveling_delay = 0;
        }
        // Splits an earlier run made are made again from scratch, so leveling twice agrees.
        task.splits.retain(|split| !split.leveling);
    }
    // Without an urgent task nothing below splits work: leveling is what it always was.
    let interrupts = project.tasks.iter().any(|task| task.interrupts && !task.summary);
    // Leveling changes dates, not links or the outline, so the graph is built once.
    let graph = prepare(project)?;
    schedule_dates(project, &graph);
    let mut ripple = Ripple::new(project, &graph).filter(|_| quick);
    let lookup = Lookup::new(project);
    let mut order = Order::new(project, &graph);
    let mut placed: Vec<usize> = Vec::new();
    let mut is_placed = vec![false; project.tasks.len()];
    let mut busy: BTreeMap<u32, Busy> = BTreeMap::new();
    while let Some(index) = order.next(project) {
        let mut before: HashMap<usize, (i64, i64)> = HashMap::new();
        let resources = &lookup.units[index];
        let task = &project.tasks[index];
        let mut affected: BTreeSet<u32> = BTreeSet::new();
        if levelable(task) && !resources.is_empty() {
            if interrupts {
                let mut placing = interrupt::Placing {
                    graph: &graph,
                    ripple: &mut ripple,
                    lookup: &lookup,
                    order: &order,
                    busy: &busy,
                    is_placed: &is_placed,
                    before: &mut before,
                };
                affected = placing.place(project, index);
            } else {
                let start = earliest_fit(project, &lookup, task, resources, &busy);
                delay_to(project, &graph, &mut ripple, index, start, &mut before);
            }
        }
        // A placed task can still move when a predecessor placed after it is delayed
        // (a summary link, say); keep the busy profile true to the current dates.
        let moved: Vec<usize> = before
            .iter()
            .filter(|(&other, &dates)| {
                is_placed[other] && dates != (project.tasks[other].start, project.tasks[other].finish)
            })
            .map(|(&other, _)| other)
            .collect();
        placed.push(index);
        is_placed[index] = true;
        if interrupts && !(moved.is_empty() && affected.is_empty()) {
            // Only the people of the work that moved need their load swept again.
            for &other in moved.iter().chain([index].iter()) {
                affected.extend(lookup.units[other].iter().map(|(uid, _)| *uid));
            }
            interrupt::rebuild(project, &lookup, &placed, &affected, &mut busy);
        } else if !moved.is_empty() {
            busy = busy_of(project, &lookup, &placed);
        } else {
            add_to_busy(project, &lookup, index, &mut busy);
        }
        order.placed(project, index, before.keys().copied());
    }
    // The quick reschedule leaves late dates, slack, and summaries behind.
    schedule_dates(project, &graph);
    publish_derived(project, &graph);
    Ok(())
}

/// Delay the leaf at `index` so it starts at `start` or later, if that is later than now.
fn delay_to(
    project: &mut Project,
    graph: &Graph,
    ripple: &mut Option<Ripple>,
    index: usize,
    start: i64,
    before: &mut HashMap<usize, (i64, i64)>,
) {
    let task = &project.tasks[index];
    if start <= task.start {
        return;
    }
    let delay = task.leveling_delay;
    let bump = project.task_calendar(task).working_between(task.start, start).max(1);
    project.tasks[index].leveling_delay = delay + bump;
    reschedule(project, graph, ripple, index, before);
    // A constraint can hold the task where it was: keep the clash rather than a delay
    // that does nothing.
    if project.tasks[index].start < start {
        project.tasks[index].leveling_delay = delay;
        reschedule(project, graph, ripple, index, before);
    }
}

/// Reschedule after the leveling delay of the leaf at `index` changed: only it and the
/// work after it when one pass settles the dates, else the whole project. `before`
/// gains the earlier dates of every task that moved.
fn reschedule(
    project: &mut Project,
    graph: &Graph,
    ripple: &mut Option<Ripple>,
    index: usize,
    before: &mut HashMap<usize, (i64, i64)>,
) {
    if let Some(ripple) = ripple {
        ripple.run(project, graph, index, before);
        return;
    }
    let dates: Vec<(i64, i64)> = project
        .tasks
        .iter()
        .map(|task| (task.start, task.finish))
        .collect();
    schedule_dates(project, graph);
    for (position, old) in dates.into_iter().enumerate() {
        let task = &project.tasks[position];
        if (task.start, task.finish) != old {
            before.entry(position).or_insert(old);
        }
    }
}

/// A task whose dates leveling may not touch: pinned by hand, by an actual start, or by
/// a hard date. Its leveling delay is kept as it is.
fn movable(task: &Task) -> bool {
    !(task.summary
        || task.manual
        || task.actual_start.is_some()
        || matches!(
            task.constraint_type,
            ConstraintType::Mso | ConstraintType::Mfo
        ))
}

/// A task leveling may delay: movable, and not marked "do not level" (priority 1000).
pub(crate) fn levelable(task: &Task) -> bool {
    movable(task) && task.priority < 1000
}

/// Lookups leveling repeats for every task, built once: assignments do not change.
struct Lookup {
    /// Units per resource for each task position, by resource uid.
    units: Vec<Vec<(u32, f64)>>,
    resource: HashMap<u32, usize>,
}

impl Lookup {
    fn new(project: &Project) -> Self {
        let position: HashMap<u32, usize> = project
            .tasks
            .iter()
            .enumerate()
            .map(|(index, task)| (task.uid, index))
            .collect();
        let mut by_task: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); project.tasks.len()];
        for assignment in project.assignments.iter().filter(|a| a.units > 0.0) {
            if let Some(&index) = position.get(&assignment.task_uid) {
                *by_task[index].entry(assignment.resource_uid).or_insert(0.0) += assignment.units;
            }
        }
        Self {
            units: by_task.into_iter().map(|units| units.into_iter().collect()).collect(),
            resource: project
                .resources
                .iter()
                .enumerate()
                .map(|(index, resource)| (resource.uid, index))
                .collect(),
        }
    }

    fn resource<'a>(&self, project: &'a Project, uid: u32) -> Option<&'a Resource> {
        self.resource.get(&uid).map(|&index| &project.resources[index])
    }
}

/// The work already placed on one resource, with its load swept along time.
#[derive(Default)]
struct Busy {
    pieces: Vec<Piece>,
    stretches: Vec<Stretch>,
    /// End of the last placed piece: the resource is free after it.
    until: i64,
}

/// The busy profile of every resource from the placed tasks' current dates.
fn busy_of(project: &Project, lookup: &Lookup, placed: &[usize]) -> BTreeMap<u32, Busy> {
    let mut busy = BTreeMap::new();
    for &index in placed {
        add_to_busy(project, lookup, index, &mut busy);
    }
    busy
}

fn add_to_busy(project: &Project, lookup: &Lookup, index: usize, busy: &mut BTreeMap<u32, Busy>) {
    let task = &project.tasks[index];
    for &(resource_uid, units) in &lookup.units[index] {
        let Some(resource) = lookup.resource(project, resource_uid) else {
            continue;
        };
        let entry = busy.entry(resource_uid).or_default();
        working_pieces(project, task, units, index, &mut entry.pieces);
        let calendar = project.resource_calendar(resource);
        let capacity = Capacity::of(resource);
        entry.stretches = stretches(
            &entry.pieces,
            |date| working_ranges(calendar, date),
            |date| capacity.units_on(date),
        );
        entry.until = entry
            .pieces
            .iter()
            .map(|piece| piece.end)
            .max()
            .unwrap_or(0);
    }
}

/// The working ranges of a day, sorted and with overlaps merged, so availability is
/// never counted twice.
fn working_ranges(calendar: &Calendar, date: NaiveDate) -> Vec<(i64, i64)> {
    let mut ranges: Vec<(i64, i64)> = calendar
        .ranges_on(date)
        .iter()
        .filter(|range| range.start_minute < range.end_minute)
        .map(|range| {
            (
                compose(date, range.start_minute),
                compose(date, range.end_minute),
            )
        })
        .collect();
    ranges.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// A resource `earliest_fit` checks, with the units the task needs of it.
struct Checked<'a> {
    units: f64,
    placed: &'a Busy,
    calendar: &'a Calendar,
    capacity: Capacity<'a>,
}

/// The first start from the task's current start at which its work fits next to the
/// placed work of every resource, or the current start when it does not fit in four
/// years. A resource the task alone overloads (more units than it will ever have from the
/// task's start on) cannot be fixed by waiting and is left out of the check.
fn earliest_fit(
    project: &Project,
    lookup: &Lookup,
    task: &Task,
    resources: &[(u32, f64)],
    busy: &BTreeMap<u32, Busy>,
) -> i64 {
    fit_from(project, lookup, task, resources, busy).unwrap_or(task.start)
}

/// `earliest_fit`, or `None` when the work does not fit in four years.
fn fit_from(
    project: &Project,
    lookup: &Lookup,
    task: &Task,
    resources: &[(u32, f64)],
    busy: &BTreeMap<u32, Busy>,
) -> Option<i64> {
    let calendar = project.task_calendar(task);
    let duration = task.duration.max(0);
    let empty = Busy::default();
    let (first_day, _) = civil(task.start);
    let checked: Vec<Checked> = resources
        .iter()
        .filter_map(|(resource_uid, units)| {
            let resource = lookup.resource(project, *resource_uid)?;
            let capacity = Capacity::of(resource);
            // A resource the task alone overloads cannot be helped by waiting.
            if *units > capacity.most_from(first_day) + EPSILON {
                return None;
            }
            Some(Checked {
                units: *units,
                placed: busy.get(resource_uid).unwrap_or(&empty),
                calendar: project.resource_calendar(resource),
                capacity,
            })
        })
        .collect();
    let mut cycle = Cycle::new(horizon(&checked));
    let mut start = task.start;
    // Each round moves past one busy block. Past four years the resource will not have
    // room (its calendar never meets the task's, say): leave the task where it was.
    let limit = task.start.saturating_add(4 * 366 * 86_400_000);
    for _ in 0..10_000 {
        if start > limit {
            break;
        }
        #[cfg(test)]
        ROUNDS.with(|count| count.set(count.get() + 1));
        let segments = split::layout(calendar, start, duration, &task.splits);
        let intervals: Vec<(i64, i64)> = segments
            .iter()
            .map(|segment| (segment.start, segment.finish))
            .collect();
        let mut pieces = Vec::new();
        cut_to_calendar(calendar, &intervals, 0.0, NO_OWNER, &mut pieces);
        let clash = checked
            .iter()
            .filter_map(|resource| {
                let (from, to) = first_clash(resource, &pieces)?;
                Some((from, room_from(resource, to)))
            })
            .min();
        let Some((from, room)) = clash else {
            return Some(start);
        };
        if cycle.came_round(start, || steady(calendar, &checked)) {
            break;
        }
        // Slide the task so the work that met the clash starts where the resource next
        // has room, past the whole busy block at once.
        let shift = calendar.working_between(from, room).max(1);
        start = calendar.add_lag(start, shift);
    }
    None
}

/// Where the placed work of every checked resource is over.
fn horizon(checked: &[Checked]) -> i64 {
    checked
        .iter()
        .map(|resource| {
            // `room_from` reads the placed stretches up to the last one's start.
            let last = resource.placed.stretches.last().map_or(i64::MIN, |s| s.start + 1);
            resource.placed.until.max(last)
        })
        .max()
        .unwrap_or(i64::MIN)
}

/// From when the task's calendar and every checked resource's calendar and capacity
/// repeat every week.
fn steady(calendar: &Calendar, checked: &[Checked]) -> i64 {
    checked
        .iter()
        .map(|resource| {
            weekly_from(resource.calendar).max(midnight_of(resource.capacity.steady_from()))
        })
        .fold(weekly_from(calendar), i64::max)
}

/// The first time from `at` on when the resource works and has room for its units
/// more: inside its placed work, or once that is over, when its calendar next works on a
/// day it has the units (a leave is passed in one go).
fn room_from(resource: &Checked, at: i64) -> i64 {
    let (busy, units) = (resource.placed, resource.units);
    // Before the placed work there is no load, so the clash came from the capacity: the
    // room may come before the placed work. Without periods this cannot happen in a way
    // the old search did not already handle, so it is left exactly as it was.
    if resource.capacity.has_periods() {
        let placed_from = busy.stretches.first().map_or(i64::MAX, |stretch| stretch.start);
        if at < placed_from {
            let free = free_from(resource, at);
            if free < placed_from {
                return free;
            }
        }
    }
    let first = busy.stretches.partition_point(|stretch| stretch.start < at);
    let inside = busy.stretches[first..].iter().find(|stretch| {
        stretch.available > EPSILON && stretch.load + units <= stretch.available + EPSILON
    });
    if let Some(stretch) = inside {
        return stretch.start;
    }
    free_from(resource, busy.until.max(at))
}

/// When the resource's calendar next works, from `at` on, on a day it has its units.
fn free_from(resource: &Checked, at: i64) -> i64 {
    let after = resource.calendar.next_work_start(at);
    let (day, _) = civil(after);
    match resource.capacity.next_with(day, resource.units) {
        Some(next) if next > day => resource.calendar.next_work_start(compose(next, 0)),
        // No such day comes: the search runs into its limit or comes round.
        _ => after,
    }
}

/// The earliest stretch in which `pieces` at the resource's units do not fit next to its
/// placed work, given what it has each day while its calendar works.
fn first_clash(resource: &Checked, pieces: &[Piece]) -> Option<(i64, i64)> {
    let (calendar, others, units) = (resource.calendar, &resource.placed.pieces, resource.units);
    for piece in pieces {
        let (date, _) = civil(piece.start);
        let mut cuts = vec![piece.start, piece.end];
        let overlapping: Vec<&Piece> = others
            .iter()
            .filter(|other| other.start < piece.end && other.end > piece.start)
            .collect();
        for other in &overlapping {
            cuts.push(other.start.clamp(piece.start, piece.end));
            cuts.push(other.end.clamp(piece.start, piece.end));
        }
        let ranges = working_ranges(calendar, date);
        let max_units = resource.capacity.units_on(date);
        for (start, end) in &ranges {
            cuts.push((*start).clamp(piece.start, piece.end));
            cuts.push((*end).clamp(piece.start, piece.end));
        }
        cuts.sort_unstable();
        cuts.dedup();
        for window in cuts.windows(2) {
            let (from, to) = (window[0], window[1]);
            let load: f64 = overlapping
                .iter()
                .filter(|other| other.start < to && other.end > from)
                .map(|other| other.units)
                .sum();
            let works = ranges
                .iter()
                .any(|(start, end)| *start <= from && to <= *end);
            let available = if works { max_units } else { 0.0 };
            if load + units > available + EPSILON {
                return Some((from, to));
            }
        }
    }
    None
}

#[path = "level_interrupt.rs"]
mod interrupt;

#[path = "level_queue.rs"]
mod queue;
use queue::Order;

#[path = "level_loads.rs"]
mod loads;
pub(crate) use loads::Loads;

#[cfg(test)]
#[path = "level_tests.rs"]
mod tests;
