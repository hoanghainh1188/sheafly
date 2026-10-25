use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

use crate::checkpoint::{self, Checkpoints};
use crate::command::Command;
use crate::document::Document;
use crate::error::EngineError;
use crate::evm::BurndownSnapshot;
use crate::model::Project;
use crate::recovery::{self, Damage, OpenReport, Recovery};

/// File format version. Bump it when a new `Command` variant or a table an older app must
/// read is added, so that app refuses the file with a clear message instead of failing
/// halfway through. The `checkpoint` table needs no bump: it is only a cache, and an app
/// that does not know it ignores it and replays the log. Version 2: actual dates drive the
/// schedule and remaining-duration/work edits keep progress (phase 13), so an older app
/// would replay the same commands to other dates. Version 3 added
/// `Command::RescheduleUncompleted` and the background flag on recurring series.
/// Version 4 added `Command::ApplyLeveling` (phase 16). Version 5 added `MoveSegment`,
/// `PasteTasks` and `FillDown` (phase 17). Version 6 added `SetEditableActualCosts` and
/// `SetTaskActualCost`, and version 7 `SetCalendarWorkWeek` and `ClearCalendarWorkWeek`
/// (phase 15); version 8 `SetAssignmentActualCost`; version 9 `SetCalendarBase` and the
/// base of `CreateCalendar`; version 10 `SetProjectCalendar`; version 11 the custom field
/// commands (`AddField`, `UpdateField`, `DeleteField`, `SetFieldValue`) and the custom
/// field of `FillDown` (phase 18); version 12 `SetLevelingOrder`, version 13
/// `SetResourceAvailability`, version 14 the skill and rule commands (`AddSkill`,
/// `RenameSkill`, `DeleteSkill`, `SetResourceSkills`, `SetAssignmentRules`) and
/// `AssignTasks`, version 15 `SetInterrupts` and the leveling splits of `ApplyLeveling` (an
/// older app would drop them and replay other dates), version 16 `SetWorkRange` and
/// `SetForecastSettings` (the finish forecast) and the effort range of `FillDown`, version 17
/// `SetResourceEmail`, `SetSource` and `ApplyPull` (tasks pulled from an external tracker).
/// `RecordPush` and the pushed days of a task's link (a push to the tracker) are version 18, and `ApplyPull`'s `resolutions` with the `base` of a task's link (three-way
/// sync, conflicts) version 19, and a source's `sync` settings (per-field direction and conflict policy,
/// constants) version 20, and the planned days (`PullTask::planned_*`, `PushChange::start_field`/`due_field`,
/// `PushedEntry::planned_*`) version 21, and the Jira tracker kind and a pulled assignee's name (`PullTask::assignee_name`) version 22, and
/// skill levels and resource groups (`SetSkillLevels`, `SetResourceGroup`, the levels of
/// `SetResourceSkills`, a rule's `min_level`) version 23, and the productivity factor
/// (`SetFocusFactor`, `SetResourceFocus`) version 24, and the cash flow data
/// (`SetCashFlowSettings`, `SetPayments`, `SetPaymentReceipts`) version 25. None of them changes how older
/// commands schedule.
pub const FORMAT_VERSION: u32 = 25;

/// Checkpoints a file keeps below the cursor, besides the one at the cursor. Each is
/// a whole scheduled project (about 1 MB for 1,000 tasks), so only a few nearby ones.
const FILE_CHECKPOINTS_BEFORE_CURSOR: usize = 3;

fn store_err(err: impl ToString) -> EngineError {
    EngineError::Store(err.to_string())
}

/// Write the document to `path` atomically: a fresh temp file beside the real file is
/// written in one transaction and then renamed over it, so a crash never leaves a
/// half-written project. A symlinked `path` updates the file it points to.
pub fn save(document: &Document, path: &str) -> Result<(), EngineError> {
    let target = fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let temp = create_temp(&target)?;
    let result = write_file(document, &temp)
        .and_then(|()| keep_permissions(&target, &temp))
        .and_then(|()| fs::rename(&temp, &target).map_err(save_err));
    match result {
        Ok(()) => {
            sync_parent(&target);
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

fn save_err(err: impl ToString) -> EngineError {
    EngineError::Save(err.to_string())
}

/// A new, uniquely named hidden file next to `target`. Never reuses an existing file.
fn create_temp(target: &Path) -> Result<PathBuf, EngineError> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "du-an".into());
    let mut last_error = None;
    for attempt in 0..100 {
        let candidate = dir.join(format!(".{name}.{}-{attempt}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
            Err(err) => {
                last_error = Some(err);
                break;
            }
        }
    }
    Err(last_error.map_or(EngineError::TempFile, save_err))
}

fn keep_permissions(target: &Path, temp: &Path) -> Result<(), EngineError> {
    match fs::metadata(target) {
        Ok(metadata) => fs::set_permissions(temp, metadata.permissions()).map_err(save_err),
        Err(_) => Ok(()),
    }
}

/// Best effort: make the rename itself durable. The data is already committed.
fn sync_parent(target: &Path) {
    if let Some(dir) = target.parent() {
        if let Ok(handle) = File::open(dir) {
            let _ = handle.sync_all();
        }
    }
}

fn write_file(document: &Document, path: &Path) -> Result<(), EngineError> {
    let mut connection = Connection::open(path).map_err(save_err)?;
    let tx = connection.transaction().map_err(save_err)?;
    tx.execute_batch(
        "CREATE TABLE meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE command_log (
            seq INTEGER PRIMARY KEY,
            payload TEXT NOT NULL
        );
        CREATE TABLE burndown (
            date INTEGER PRIMARY KEY,
            remaining_work INTEGER NOT NULL,
            ev REAL NOT NULL,
            ac REAL NOT NULL
        );
        CREATE TABLE checkpoint (
            applied INTEGER PRIMARY KEY,
            engine TEXT NOT NULL,
            project TEXT NOT NULL
        );",
    )
    .map_err(save_err)?;
    let engine = checkpoint::engine_tag();
    for (applied, project) in file_checkpoints(document) {
        let payload = serde_json::to_string(project).map_err(save_err)?;
        tx.execute(
            "INSERT INTO checkpoint(applied, engine, project) VALUES(?1, ?2, ?3)",
            params![applied as i64, engine, payload],
        )
        .map_err(save_err)?;
    }
    let base = serde_json::to_string(&document.base).map_err(save_err)?;
    let meta = [
        ("version", FORMAT_VERSION.to_string()),
        ("cursor", document.cursor.to_string()),
        ("base", base),
    ];
    for (key, value) in &meta {
        tx.execute("INSERT INTO meta(key, value) VALUES(?1, ?2)", params![key, value])
            .map_err(save_err)?;
    }
    for snapshot in &document.burndown {
        tx.execute(
            "INSERT INTO burndown(date, remaining_work, ev, ac) VALUES(?1, ?2, ?3, ?4)",
            params![snapshot.date, snapshot.remaining_work, snapshot.ev, snapshot.ac],
        )
        .map_err(save_err)?;
    }
    for (seq, command) in document.commands.iter().enumerate() {
        let payload = serde_json::to_string(command).map_err(save_err)?;
        tx.execute(
            "INSERT INTO command_log(seq, payload) VALUES(?1, ?2)",
            params![seq as i64, payload],
        )
        .map_err(save_err)?;
    }
    tx.commit().map_err(save_err)?;
    // Close before the rename so no handle still points at the temp file.
    connection.close().map_err(|(_, err)| save_err(err))
}

/// The open project at the cursor, so opening needs no replay, plus a few states just
/// below it, so the first undos after opening stay fast.
fn file_checkpoints(document: &Document) -> Vec<(usize, &Project)> {
    let mut chosen: Vec<(usize, &Project)> = document
        .checkpoints
        .iter()
        .rev()
        .filter(|(applied, _)| *applied < document.cursor)
        .take(FILE_CHECKPOINTS_BEFORE_CURSOR)
        .collect();
    if document.cursor > 0 {
        chosen.push((document.cursor, &document.project));
    }
    chosen
}

/// A checkpoint row as stored. The project is parsed only when it is used.
struct StoredCheckpoint {
    applied: usize,
    ours: bool,
    payload: String,
}

impl StoredCheckpoint {
    fn project(&self) -> Option<Project> {
        serde_json::from_str(&self.payload).ok()
    }
}

/// Every readable checkpoint row. A damaged table reads as none: the log replays instead.
fn stored_checkpoints(connection: &Connection) -> Vec<StoredCheckpoint> {
    read_stored_checkpoints(connection).unwrap_or_default()
}

fn read_stored_checkpoints(connection: &Connection) -> Result<Vec<StoredCheckpoint>, EngineError> {
    if !table_exists(connection, "checkpoint")? {
        return Ok(Vec::new());
    }
    let engine = checkpoint::engine_tag();
    let mut statement = connection
        .prepare("SELECT applied, engine, project FROM checkpoint ORDER BY applied")
        .map_err(store_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(store_err)?;
    let mut stored = Vec::new();
    for row in rows {
        let (applied, tag, payload) = row.map_err(store_err)?;
        if let Some(applied) = usize::try_from(applied).ok().filter(|applied| *applied > 0) {
            stored.push(StoredCheckpoint { applied, ours: tag == engine, payload });
        }
    }
    Ok(stored)
}

fn table_exists(connection: &Connection, name: &str) -> Result<bool, EngineError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |row| row.get(0),
        )
        .map_err(store_err)?;
    Ok(count > 0)
}

/// Refuse files written by a newer app. Files from before the version row open as version 1.
fn check_version(connection: &Connection) -> Result<(), EngineError> {
    let stored: Option<String> = connection
        .query_row("SELECT value FROM meta WHERE key = 'version'", [], |row| row.get(0))
        .optional()
        .map_err(store_err)?;
    let Some(stored) = stored else {
        return Ok(());
    };
    let version: u32 = stored
        .trim()
        .parse()
        .map_err(|_| EngineError::BadFormatVersion(stored.clone()))?;
    if version > FORMAT_VERSION {
        return Err(EngineError::NewerFormat(version));
    }
    Ok(())
}

/// A file opened for editing, with what the app should tell the user about it.
pub struct Opened {
    pub document: Document,
    pub report: OpenReport,
}

/// The commands that parse, in order, up to the first one that does not.
struct Log {
    commands: Vec<Command>,
    /// Rows in the log, including any after the damage.
    total: usize,
    damage: Option<(usize, Damage)>,
}

fn read_log(connection: &Connection) -> Result<Log, EngineError> {
    let mut statement = connection
        .prepare("SELECT payload FROM command_log ORDER BY seq")
        .map_err(store_err)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(store_err)?;
    let mut log = Log { commands: Vec::new(), total: 0, damage: None };
    for row in rows {
        let position = log.total;
        log.total += 1;
        if log.damage.is_some() {
            continue;
        }
        let parsed = row
            .map_err(|err| Damage::Failed(store_err(err)))
            .and_then(|payload| {
                serde_json::from_str::<Command>(&payload).map_err(|err| {
                    // Only a command name this build does not know points at a newer app.
                    if err.to_string().contains("unknown variant") {
                        Damage::Newer
                    } else {
                        Damage::Unreadable
                    }
                })
            });
        match parsed {
            Ok(command) => log.commands.push(command),
            Err(damage) => log.damage = Some((position, damage)),
        }
    }
    Ok(log)
}

/// Open a file. A damaged command log opens as an unsaved copy at the last state that
/// can be rebuilt; only an unreadable header or a newer format is an error.
pub fn open(path: &str) -> Result<Opened, EngineError> {
    let connection = Connection::open(path).map_err(store_err)?;
    check_version(&connection)?;
    let base_json: String = connection
        .query_row("SELECT value FROM meta WHERE key = 'base'", [], |row| row.get(0))
        .map_err(store_err)?;
    let cursor_text: String = connection
        .query_row("SELECT value FROM meta WHERE key = 'cursor'", [], |row| row.get(0))
        .map_err(store_err)?;
    let base: Project = serde_json::from_str(&base_json).map_err(store_err)?;
    let log = read_log(&connection)?;
    // A damaged burn-down table or cursor loses that much, not the whole project.
    let burndown = load_burndown(&connection).unwrap_or_default();
    let stored = stored_checkpoints(&connection);

    let saved_cursor = cursor_text.trim().parse::<usize>().unwrap_or(log.total).min(log.total);
    // Checkpoints from this engine are trusted as replays of the commands before them;
    // that holds as long as `ENGINE_REVISION` is bumped whenever results can change.
    let mut checkpoints = Checkpoints::default();
    for row in stored.iter().filter(|row| row.ours && row.applied <= log.commands.len()) {
        if let Some(project) = row.project() {
            checkpoints.insert(row.applied, project);
        }
    }
    let mut document = Document {
        project: base.clone(),
        base,
        cursor: saved_cursor.min(log.commands.len()),
        commands: log.commands,
        path: Some(path.to_string()),
        dirty: false,
        burndown,
        checkpoints,
    };
    let replayed = document.replay_to(document.cursor);
    let replay_failed = replayed.error.is_some();
    let damage = match &replayed.error {
        Some(error) => Some((replayed.applied, Damage::Failed(error.clone()))),
        None => log.damage,
    };
    let Some((at, damage)) = damage else {
        document.project = replayed.project;
        let moved = stored
            .iter()
            .find(|row| !row.ours && row.applied == document.cursor)
            .and_then(StoredCheckpoint::project)
            .map(|before| recovery::moved_tasks(&before, &document.project))
            .unwrap_or_default();
        return Ok(Opened { document, report: OpenReport { recovery: None, moved } });
    };
    if !replay_failed && at >= saved_cursor {
        // Only redo commands are lost; the saved state and its history are whole.
        document.project = replayed.project;
        let recovery = Recovery {
            damage,
            at,
            total: log.total,
            saved_cursor,
            shown_after: saved_cursor,
            history_kept: true,
            copy: false,
        };
        return Ok(Opened {
            document,
            report: OpenReport { recovery: Some(recovery), moved: Vec::new() },
        });
    }

    let good = replayed.applied;
    // A checkpoint this engine saved past the damage still shows what the user last saw.
    let later = stored
        .iter()
        .rev()
        .filter(|row| row.ours && row.applied > good && row.applied <= saved_cursor)
        .find_map(|row| row.project().map(|project| (row.applied, project)));
    let history_kept = later.is_none();
    let shown_after = match later {
        Some((applied, project)) => {
            document.base = project.clone();
            document.project = project;
            document.commands.clear();
            document.cursor = 0;
            document.checkpoints.clear();
            applied
        }
        None => {
            document.project = replayed.project;
            document.commands.truncate(good);
            document.cursor = good;
            document.checkpoints.drop_after(good);
            good
        }
    };
    // Saving must not write over the file that still holds the rest of the log.
    document.path = None;
    document.dirty = true;
    let recovery = Recovery {
        damage,
        at,
        total: log.total,
        saved_cursor,
        shown_after,
        history_kept,
        copy: true,
    };
    Ok(Opened {
        document,
        report: OpenReport { recovery: Some(recovery), moved: Vec::new() },
    })
}

/// Open a file without the report. See `open`.
pub fn load(path: &str) -> Result<Document, EngineError> {
    open(path).map(|opened| opened.document)
}

/// User save: sample burn-down for today (or the status date day), then write the file.
pub fn save_with_snapshot(document: &mut Document, path: &str, now: i64) -> Result<(), EngineError> {
    document.record_burndown(now);
    save(document, path)
}

fn load_burndown(connection: &Connection) -> Result<Vec<BurndownSnapshot>, EngineError> {
    // Files written before phase 10 have no burn-down table.
    if !table_exists(connection, "burndown")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare("SELECT date, remaining_work, ev, ac FROM burndown ORDER BY date")
        .map_err(|err| EngineError::Store(err.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok(BurndownSnapshot {
                date: row.get(0)?,
                remaining_work: row.get(1)?,
                ev: row.get(2)?,
                ac: row.get(3)?,
            })
        })
        .map_err(|err| EngineError::Store(err.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| EngineError::Store(err.to_string()))
}
