//! Planning preview (phase 18 P3, step 3c): see what assigning people and leveling would
//! do before anything changes, then commit exactly that.
//!
//! `preview_plan` works on a copy: it assigns the people chosen (the UI starts from the
//! suggestions of `suggest.rs` and lets them be changed), schedules, levels as
//! `LevelResources` would, and reports the result: the tasks whose dates move, the tasks
//! that end after their deadline, and the people still overallocated. It also returns the
//! commands' data, so `ApplyPlan` replays it as one undo step without rerunning the
//! heuristic. The Tauri layer refuses the plan when the project changed since the preview.

use serde::Serialize;

use crate::command::{apply_one, leveled, Command, LevelingDelay, LevelingSplits};
use crate::error::{EngineError, Invalid};
use crate::model::Project;
use crate::schedule::schedule;
use crate::skills::TaskAssignment;

/// A task whose dates the plan changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanMove {
    pub uid: u32,
    pub start_before: i64,
    pub finish_before: i64,
    pub start: i64,
    pub finish: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanPreview {
    pub assignments: Vec<TaskAssignment>,
    pub delays: Vec<LevelingDelay>,
    pub splits: Vec<LevelingSplits>,
    /// Leaves whose start or finish moves, in outline order.
    pub moves: Vec<PlanMove>,
    /// Leaves that would end after their deadline, in outline order.
    pub late: Vec<u32>,
    /// People still overallocated after leveling (work it cannot move), by uid.
    pub overallocated: Vec<u32>,
}

/// What assigning `assignments` and then leveling would do to `project`.
pub fn preview_plan(project: &Project, assignments: &[TaskAssignment]) -> Result<PlanPreview, EngineError> {
    let mut trial = project.clone();
    if !assignments.is_empty() {
        let assign = Command::AssignTasks {
            assignments: assignments.to_vec(),
        };
        apply_one(&mut trial, &assign)?;
        schedule(&mut trial)?;
    }
    let Command::ApplyLeveling { delays, splits } = leveled(&trial)? else {
        unreachable!("leveled always gives ApplyLeveling");
    };
    let leveling = Command::ApplyLeveling {
        delays: delays.clone(),
        splits: splits.clone(),
    };
    apply_one(&mut trial, &leveling)?;
    schedule(&mut trial)?;

    let (moves, late, overallocated) = outcome(project, &trial);
    Ok(PlanPreview {
        assignments: assignments.to_vec(),
        delays,
        splits,
        moves,
        late,
        overallocated,
    })
}

/// What changed from `before` to `after` (both scheduled): the leaves whose dates move,
/// the leaves that end after their deadline, and the people still overallocated.
pub(crate) fn outcome(before: &Project, after: &Project) -> (Vec<PlanMove>, Vec<u32>, Vec<u32>) {
    let moves = after
        .tasks
        .iter()
        .filter(|task| !task.summary)
        .filter_map(|task| {
            let old = before.task(task.uid)?;
            (old.start != task.start || old.finish != task.finish).then_some(PlanMove {
                uid: task.uid,
                start_before: old.start,
                finish_before: old.finish,
                start: task.start,
                finish: task.finish,
            })
        })
        .collect();
    let late = after
        .tasks
        .iter()
        .filter(|task| !task.summary && task.deadline.is_some_and(|deadline| task.finish > deadline))
        .map(|task| task.uid)
        .collect();
    let mut overallocated: Vec<u32> = after.overallocations.iter().map(|over| over.resource_uid).collect();
    overallocated.sort_unstable();
    overallocated.dedup();
    (moves, late, overallocated)
}

/// `ApplyPlan`: the assignments, then the leveling a preview chose.
pub(crate) fn apply_plan(
    project: &mut Project,
    assignments: &[TaskAssignment],
    delays: &[LevelingDelay],
    splits: &[LevelingSplits],
) -> Result<(), EngineError> {
    if assignments.is_empty() && delays.is_empty() && splits.is_empty() {
        return Err(Invalid::EmptyPlan.into());
    }
    if !assignments.is_empty() {
        apply_one(
            project,
            &Command::AssignTasks {
                assignments: assignments.to_vec(),
            },
        )?;
    }
    apply_one(
        project,
        &Command::ApplyLeveling {
            delays: delays.to_vec(),
            splits: splits.to_vec(),
        },
    )
}
