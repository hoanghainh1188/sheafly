//! AI proposals (phase 22 PR D, sheafly-pro#1): "the AI proposes, the engine verifies, the
//! user decides". A proposal is a list of ordinary commands. `trial_proposal` runs it on a
//! copy and measures what it does (decision D5); `ApplyProposal` applies exactly that list
//! as one undo step, all or nothing (decision D1). Only the commands of decision D2 may be
//! in one, checked here as well as where the model's answer is read, so a file edited by
//! hand cannot slip another command in.

use serde::Serialize;

use crate::advice::{Goal, Proposal};
use crate::command::{apply_one, leveled, Command};
use crate::error::{EngineError, Invalid};
use crate::level::over_minutes;
use crate::model::Project;
use crate::plan::{outcome, PlanMove};
use crate::schedule::schedule;
use crate::skills::TaskAssignment;
use crate::statistics::statistics;

/// A finish that moves by at most this many working minutes (one 8-hour day) is not worse.
pub const FINISH_SLACK_MINUTES: i64 = 480;
/// More overallocated work than this, in working minutes, is worse.
pub const OVER_SLACK_MINUTES: i64 = 60;

/// Whether `command` may be part of a proposal (decision D2). `ApplyProposal` itself is not,
/// so proposals never nest, and neither is `LevelResources`, which replay would rerun.
pub fn allowed_in_proposal(command: &Command) -> bool {
    matches!(
        command,
        Command::AssignResource { .. }
            | Command::UpdateAssignment { .. }
            | Command::RemoveAssignment { .. }
            | Command::SetWork { .. }
            | Command::SetDuration { .. }
            | Command::SetTaskType { .. }
            | Command::SetPriority { .. }
            | Command::AddDependency { .. }
            | Command::RemoveDependency { .. }
            | Command::SetPredecessors { .. }
            | Command::SetConstraint { .. }
            | Command::SetDeadline { .. }
            | Command::SplitTask { .. }
            | Command::SetInterrupts { .. }
            | Command::SetLevelingDelay { .. }
            | Command::ApplyLeveling { .. }
            | Command::SetWorkRange { .. }
    )
}

/// `ApplyProposal`: every command in order on the same project; the caller works on a copy,
/// so one failing leaves the project as it was.
pub(crate) fn apply_proposal(project: &mut Project, commands: &[Command]) -> Result<(), EngineError> {
    if commands.is_empty() {
        return Err(Invalid::EmptyProposal.into());
    }
    if !commands.iter().all(allowed_in_proposal) {
        return Err(Invalid::NotInProposal.into());
    }
    commands.iter().try_for_each(|command| apply_one(project, command))
}

/// The four measures of decision D5, for a scheduled project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Measures {
    /// The project's finish.
    pub finish: i64,
    /// Open leaves that end after their deadline.
    pub late: usize,
    /// Work above what people have, in working minutes, everyone together.
    pub over_minutes: i64,
    /// Open critical leaves.
    pub critical: usize,
}

impl Measures {
    pub fn of(project: &Project) -> Self {
        let open = || project.tasks.iter().filter(|task| !task.summary && task.percent_complete < 100.0);
        Self {
            finish: statistics(project).finish,
            late: open().filter(|task| task.deadline.is_some_and(|deadline| task.finish > deadline)).count(),
            over_minutes: over_minutes(project).values().sum(),
            critical: open().filter(|task| task.critical).count(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Finish,
    Late,
    Overallocation,
    Critical,
}

/// How a trial went (decision D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The goal got better and no measure got worse past its threshold.
    Improved,
    /// The goal got better, and some measure worse.
    TradeOff,
    /// The goal did not get better.
    Worse,
}

/// What a proposal would do, measured on a copy.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProposalTrial {
    /// What `ApplyProposal` would carry: the proposal's commands, then the leveling the
    /// trial computed when it asked for leveling.
    pub commands: Vec<Command>,
    /// Assignments the proposal adds (for showing who works on a moved task).
    pub assignments: Vec<TaskAssignment>,
    pub moves: Vec<PlanMove>,
    pub late: Vec<u32>,
    pub overallocated: Vec<u32>,
    pub before: Measures,
    pub after: Measures,
    /// The goal's own measure before and after: the task's finish, the person's
    /// overallocated minutes, or the project's finish. Lower is better, except for an
    /// estimate fix, where it is only shown.
    pub target_before: i64,
    pub target_after: i64,
    pub verdict: Verdict,
    /// The measures that got worse past their threshold.
    pub worse: Vec<Measure>,
}

fn target(project: &Project, goal: &Goal, measures: &Measures) -> Result<i64, EngineError> {
    match goal {
        Goal::Deadline { task } => project.task(*task).map(|task| task.finish).ok_or(EngineError::TaskNotFound),
        Goal::Overallocation { resource } => {
            project
                .resources
                .iter()
                .find(|item| item.uid == *resource)
                .ok_or(EngineError::ResourceNotFound)?;
            Ok(over_minutes(project).get(resource).copied().unwrap_or(0))
        }
        Goal::Finish => Ok(measures.finish),
        Goal::Estimate { task } => project.task(*task).map(|task| task.finish).ok_or(EngineError::TaskNotFound),
    }
}

/// The measures that got worse past their threshold from `before` to `after`.
pub fn worse_measures(project: &Project, before: &Measures, after: &Measures) -> Vec<Measure> {
    let slip = if after.finish > before.finish {
        project.calendar().working_between(before.finish, after.finish)
    } else {
        0
    };
    [
        (slip > FINISH_SLACK_MINUTES, Measure::Finish),
        (after.late > before.late, Measure::Late),
        (after.over_minutes > before.over_minutes + OVER_SLACK_MINUTES, Measure::Overallocation),
        (after.critical > before.critical, Measure::Critical),
    ]
    .into_iter()
    .filter_map(|(worse, measure)| worse.then_some(measure))
    .collect()
}

/// Run `proposal` on a copy of `project` (scheduled) and measure it. An error is the
/// engine refusing a command, in the same words `ApplyProposal` would use.
pub fn trial_proposal(project: &Project, proposal: &Proposal) -> Result<ProposalTrial, EngineError> {
    if proposal.commands.iter().any(|command| matches!(command, Command::ApplyLeveling { .. })) {
        return Err(Invalid::NotInProposal.into());
    }
    let mut commands = proposal.commands.clone();
    if proposal.level {
        let mut leveled_copy = project.clone();
        if !commands.is_empty() {
            apply_proposal(&mut leveled_copy, &commands)?;
            schedule(&mut leveled_copy)?;
        }
        commands.push(leveled(&leveled_copy)?);
    }
    // Build what `ApplyProposal` will build, from the project itself.
    let mut trial = project.clone();
    apply_proposal(&mut trial, &commands)?;
    schedule(&mut trial)?;

    let before = Measures::of(project);
    let after = Measures::of(&trial);
    let target_before = target(project, &proposal.goal, &before)?;
    let target_after = target(&trial, &proposal.goal, &after)?;
    let worse = worse_measures(project, &before, &after);
    // An estimate fix makes the plan truer rather than shorter: only the measures judge it.
    let better = matches!(proposal.goal, Goal::Estimate { .. }) || target_after < target_before;
    let verdict = match (better, worse.is_empty()) {
        (true, true) => Verdict::Improved,
        (true, false) => Verdict::TradeOff,
        (false, _) => Verdict::Worse,
    };
    let assignments = trial
        .assignments
        .iter()
        .filter(|item| !project.assignments.iter().any(|old| old.uid == item.uid))
        .map(|item| TaskAssignment {
            task_uid: item.task_uid,
            resource_uid: item.resource_uid,
            units: item.units,
        })
        .collect();
    let (moves, late, overallocated) = outcome(project, &trial);
    Ok(ProposalTrial {
        commands,
        assignments,
        moves,
        late,
        overallocated,
        before,
        after,
        target_before,
        target_after,
        verdict,
        worse,
    })
}
