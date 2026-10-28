use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::checkpoint::Checkpoints;
use crate::command::{self, Command};
use crate::error::EngineError;
use crate::evm::{self, BurndownSnapshot, ProjectMetrics};
use crate::model::{Project, Task};
use crate::schedule::{mark_late, schedule};
use crate::timeutil::{civil, compose};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub project: Project,
    pub path: Option<String>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Commands applied and not undone (`Document::cursor`). The UI records it when a
    /// dialog opens, so Hủy can undo back to exactly that point.
    #[serde(default)]
    pub history_cursor: usize,
}

/// Outcome of replaying part of the log.
#[derive(Debug)]
pub struct Replayed {
    /// The state after the first `applied` commands.
    pub project: Project,
    pub applied: usize,
    /// Why command `applied` (0-based) could not be replayed, if one failed.
    pub error: Option<EngineError>,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub base: Project,
    pub project: Project,
    pub commands: Vec<Command>,
    pub cursor: usize,
    pub path: Option<String>,
    pub dirty: bool,
    /// Burn-down samples, oldest first. Kept outside the command log so undo does not drop them.
    pub burndown: Vec<BurndownSnapshot>,
    /// Scheduled states part-way through `commands`. Valid only while `base` and the
    /// commands before each state stay as they are.
    pub checkpoints: Checkpoints,
}

impl Document {
    /// A new project in Vietnamese ("Dự án mới", "Công việc 1", calendar "Chuẩn").
    pub fn new_default() -> Self {
        Self::new_in(Lang::Vi)
    }

    /// A new project whose default names (project, first task, calendar) are in `lang`.
    /// The names are data: they go into `base` and stay as they are wherever the file is
    /// opened later, so replay never depends on the language.
    pub fn new_in(lang: Lang) -> Self {
        let mut project = Project::new(lang.pick3("Dự án mới", "New project", "新規プロジェクト"), default_start(now_millis()));
        if let Some(calendar) = project.calendars.first_mut() {
            calendar.name = lang.pick3("Chuẩn", "Standard", "標準").into();
        }
        let uid = project.alloc_task_uid();
        let mut task = Task::new(uid, lang.pick3("Công việc 1", "Task 1", "タスク 1"));
        task.start = project.start;
        project.tasks.push(task);
        Self::from_project(project)
    }

    pub fn from_project(mut project: Project) -> Self {
        if project.tasks.is_empty() {
            let uid = project.alloc_task_uid();
            let mut task = Task::new(uid, "Công việc 1");
            task.start = project.start;
            project.tasks.push(task);
        }
        schedule(&mut project).ok();
        Self {
            base: project.clone(),
            project,
            commands: Vec::new(),
            cursor: 0,
            path: None,
            dirty: false,
            burndown: Vec::new(),
            checkpoints: Checkpoints::default(),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            path: self.path.clone(),
            dirty: self.dirty,
            can_undo: self.cursor > 0,
            can_redo: self.cursor < self.commands.len(),
            history_cursor: self.cursor,
        }
    }

    pub fn apply(&mut self, command: Command) -> Result<Snapshot, EngineError> {
        // Log what leveling decided, not the request, so replay is cheap and stays put
        // if the heuristic changes.
        let command = match command {
            Command::LevelResources => command::leveled(&self.project)?,
            other => other,
        };
        if self.cursor < self.commands.len() {
            self.commands.truncate(self.cursor);
            self.checkpoints.drop_after(self.cursor);
        }
        let mut trial = self.project.clone();
        command::apply_one(&mut trial, &command)?;
        schedule(&mut trial)?;
        let status_changed = trial.status_date != self.project.status_date;
        self.commands.push(command);
        self.cursor = self.commands.len();
        self.project = trial;
        self.checkpoints.record(self.cursor, &self.project);
        self.dirty = true;
        if let Some(status) = self.project.status_date.filter(|_| status_changed) {
            self.record_burndown(status);
        }
        Ok(self.snapshot())
    }

    /// Sample burn-down for the status date day, or for `now` when there is no status date.
    pub fn record_burndown(&mut self, now: i64) {
        let date = self.project.status_date.unwrap_or(now);
        let snapshot = evm::take_snapshot(&self.project, date);
        evm::record_snapshot(&mut self.burndown, snapshot);
    }

    pub fn metrics(&self) -> ProjectMetrics {
        evm::metrics(&self.project, &self.burndown)
    }

    pub fn undo(&mut self) -> Result<Snapshot, EngineError> {
        if self.cursor == 0 {
            return Ok(self.snapshot());
        }
        self.project = self.project_at(self.cursor - 1)?;
        self.cursor -= 1;
        self.dirty = true;
        Ok(self.snapshot())
    }

    pub fn redo(&mut self) -> Result<Snapshot, EngineError> {
        if self.cursor >= self.commands.len() {
            return Ok(self.snapshot());
        }
        self.project = self.project_at(self.cursor + 1)?;
        self.cursor += 1;
        self.dirty = true;
        Ok(self.snapshot())
    }

    pub fn replace_project(&mut self, project: Project, path: Option<String>) -> Result<Snapshot, EngineError> {
        let mut project = project;
        schedule(&mut project)?;
        self.base = project.clone();
        self.project = project;
        self.commands.clear();
        self.cursor = 0;
        self.path = path;
        self.dirty = false;
        self.burndown.clear();
        self.checkpoints.clear();
        Ok(self.snapshot())
    }

    /// Fold all but the last `keep` undo steps into `base`, so the log and the file stop
    /// growing. Redo steps stay. Returns how many commands were folded. Cannot be undone.
    pub fn compact_history(&mut self, keep: usize) -> Result<usize, EngineError> {
        let start = self.cursor.saturating_sub(keep);
        if start == 0 {
            return Ok(0);
        }
        self.base = self.project_at(start)?;
        self.commands.drain(..start);
        self.cursor -= start;
        self.checkpoints.rebase(start);
        self.dirty = true;
        Ok(start)
    }

    /// The scheduled project after the first `applied` commands, replayed from the
    /// nearest checkpoint. States passed on the way are kept for the next undo.
    pub fn project_at(&mut self, applied: usize) -> Result<Project, EngineError> {
        let replayed = self.replay_to(applied);
        match replayed.error {
            Some(err) => Err(err),
            None => Ok(replayed.project),
        }
    }

    /// Like `project_at`, but a command that fails keeps the state before it, so an
    /// opened file can still show everything up to the damage.
    pub fn replay_to(&mut self, applied: usize) -> Replayed {
        let applied = applied.min(self.commands.len());
        let (from, mut project) = match self.checkpoints.nearest(applied) {
            Some((from, project)) => (from, project.clone()),
            None => (0, self.base.clone()),
        };
        if from == 0 && applied == 0 {
            // A saved base with no replayed commands still needs the derived late flag.
            mark_late(&mut project);
        }
        for step in from..applied {
            let result = command::apply_one(&mut project, &self.commands[step])
                .and_then(|()| schedule(&mut project));
            if let Err(error) = result {
                // The failed command may have half-changed `project`. Rebuild the state
                // before it; those commands just replayed, so this cannot fail again.
                let before = self.replay_to(step);
                return Replayed { error: Some(error), ..before };
            }
            self.checkpoints.record(step + 1, &project);
        }
        Replayed { project, applied, error: None }
    }
}

pub fn replay(base: &Project, commands: &[Command]) -> Result<Project, EngineError> {
    let mut project = base.clone();
    for command in commands {
        command::apply_one(&mut project, command)?;
        schedule(&mut project)?;
    }
    // A saved base with no replayed commands still needs the derived late flag.
    if commands.is_empty() {
        mark_late(&mut project);
    }
    Ok(project)
}

pub fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// 08:00 on the current working day, or the next working morning.
pub fn default_start(now: i64) -> i64 {
    let (date, _minute) = civil(now);
    let morning = compose(date, 8 * 60);
    let calendar = crate::calendar::Calendar::standard();
    if calendar.ranges_on(date).is_empty() {
        calendar.next_work_start(morning)
    } else {
        morning
    }
}
