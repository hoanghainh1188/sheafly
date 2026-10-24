//! Scheduled projects kept part-way through the command log, so undo, redo, and open
//! replay a few commands instead of the whole log.
//!
//! `base` plus the log stay the source of truth. A checkpoint is only a cache of
//! `replay(base, &commands[..applied])`, which holds because every command is
//! deterministic. Files store the engine that wrote each checkpoint; a different engine
//! may schedule differently, so its checkpoints are ignored and the log is replayed.

use std::collections::BTreeMap;

use crate::model::Project;

/// Bump when a change to the engine can move dates or derived fields for the same
/// commands, so checkpoints written before the change are not trusted.
/// Revision 2: actual dates drive the schedule (phase 13).
/// Revision 3: regenerating a series keeps occurrences with progress (phase 13).
/// Revision 4: overallocation by overlapping hours and the new leveling (phase 16).
/// Revision 5: calendars may have work weeks (phase 15); a file with them schedules
/// differently from an engine that ignores them.
/// Revision 6: leveling breaks a tie on start and priority by the slack before
/// leveling, not the slack as leveling goes (phase 18). Only files that logged the old
/// `LevelResources` replay the heuristic, so only they can level differently.
/// Revision 7: AssignmentBaseline { number, work } on assignments and baseline load derivation (PR 4, issue #150).
pub const ENGINE_REVISION: u32 = 7;

/// A new checkpoint every this many commands while editing or replaying.
pub const INTERVAL: usize = 25;

/// A project of 1,000 tasks is about 1 MB, so memory keeps a bounded number.
const MAX_STATES: usize = 24;

/// Older history keeps only these positions, so a deep undo still has a nearby start.
const SPARSE: usize = 500;

/// Identifies the engine that produced a checkpoint stored in a file.
pub fn engine_tag() -> String {
    format!("{}+{}", env!("CARGO_PKG_VERSION"), ENGINE_REVISION)
}

#[derive(Clone, Debug, Default)]
pub struct Checkpoints {
    /// Scheduled project after the first `key` commands. Key 0 is never stored: that is `base`.
    states: BTreeMap<usize, Project>,
}

impl Checkpoints {
    pub fn clear(&mut self) {
        self.states.clear();
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Keep `project` when `applied` falls on the interval.
    pub fn record(&mut self, applied: usize, project: &Project) {
        if applied > 0 && applied.is_multiple_of(INTERVAL) && !self.states.contains_key(&applied) {
            self.insert(applied, project.clone());
        }
    }

    /// Keep `project` as the state after `applied` commands, wherever that falls.
    pub fn insert(&mut self, applied: usize, project: Project) {
        if applied == 0 {
            return;
        }
        self.states.insert(applied, project);
        self.thin();
    }

    /// Forget states past `applied`, once the commands after it are replaced.
    pub fn drop_after(&mut self, applied: usize) {
        self.states.split_off(&(applied + 1));
    }

    /// Renumber after the first `start` commands become the new base: the state at
    /// `start + n` is now the state at `n`, and earlier ones are gone.
    pub fn rebase(&mut self, start: usize) {
        let kept = self.states.split_off(&(start + 1));
        self.states = kept
            .into_iter()
            .map(|(applied, project)| (applied - start, project))
            .collect();
    }

    /// The latest state at or before `applied`.
    pub fn nearest(&self, applied: usize) -> Option<(usize, &Project)> {
        self.states
            .range(..=applied)
            .next_back()
            .map(|(key, project)| (*key, project))
    }

    /// Stored states, oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (usize, &Project)> {
        self.states.iter().map(|(key, project)| (*key, project))
    }

    /// Over the limit, drop the oldest state that is not on a sparse position, so
    /// recent history stays dense and old history keeps a few anchors.
    fn thin(&mut self) {
        while self.states.len() > MAX_STATES {
            let victim = self
                .states
                .keys()
                .copied()
                .find(|key| !key.is_multiple_of(SPARSE))
                .or_else(|| self.states.keys().next().copied());
            match victim {
                Some(key) => {
                    self.states.remove(&key);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> Project {
        Project::new(name, 0)
    }

    #[test]
    fn records_only_on_the_interval() {
        let mut checkpoints = Checkpoints::default();
        checkpoints.record(INTERVAL - 1, &project("a"));
        checkpoints.record(0, &project("b"));
        assert!(checkpoints.is_empty());
        checkpoints.record(INTERVAL, &project("c"));
        assert_eq!(
            checkpoints.nearest(INTERVAL * 2).map(|(key, _)| key),
            Some(INTERVAL)
        );
        assert_eq!(checkpoints.nearest(INTERVAL - 1).map(|(key, _)| key), None);
    }

    #[test]
    fn drop_after_forgets_replaced_history() {
        let mut checkpoints = Checkpoints::default();
        for applied in [25, 50, 75] {
            checkpoints.insert(applied, project("p"));
        }
        checkpoints.drop_after(50);
        assert_eq!(
            checkpoints.iter().map(|(key, _)| key).collect::<Vec<_>>(),
            vec![25, 50]
        );
    }

    #[test]
    fn rebase_renumbers_the_states_after_the_new_base() {
        let mut checkpoints = Checkpoints::default();
        for applied in [25, 50, 75] {
            checkpoints.insert(applied, project("p"));
        }
        checkpoints.rebase(50);
        assert_eq!(checkpoints.iter().map(|(key, _)| key).collect::<Vec<_>>(), vec![25]);
    }

    #[test]
    fn thinning_keeps_recent_states_and_sparse_anchors() {
        let mut checkpoints = Checkpoints::default();
        for step in 1..=200 {
            checkpoints.record(step * INTERVAL, &project("p"));
        }
        let keys: Vec<usize> = checkpoints.iter().map(|(key, _)| key).collect();
        assert_eq!(keys.len(), MAX_STATES);
        assert_eq!(*keys.last().unwrap(), 200 * INTERVAL);
        // Every anchor in old history survives; the rest is the newest dense run.
        let anchors = keys.iter().filter(|key| *key % SPARSE == 0).count();
        assert_eq!(anchors, 200 * INTERVAL / SPARSE);
        let dense = &keys[anchors - 1..];
        assert!(dense.windows(2).all(|pair| pair[1] - pair[0] == INTERVAL));
    }
}
