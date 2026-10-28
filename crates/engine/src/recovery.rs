//! What opening a file found besides the project: damage in the command log, and tasks
//! whose dates differ from what the engine that saved the file computed.
//!
//! A damaged log never stops a file from opening. The app opens an unsaved copy at the
//! last state it can rebuild and leaves the file itself alone.

use chrono::Datelike;

use crate::error::EngineError;
use crate::lang::Lang;
use crate::model::Project;
use crate::timeutil::civil;

/// How many moved tasks the message names before summing up the rest.
const MOVED_LISTED: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub enum Damage {
    /// The stored command does not parse.
    Unreadable,
    /// The command is one this build does not know, likely from a newer app.
    Newer,
    /// The command parses but fails to apply, or its row cannot be read.
    Failed(EngineError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recovery {
    pub damage: Damage,
    /// 0-based position of the first command that could not be used.
    pub at: usize,
    /// Commands in the file's log, including the damaged ones.
    pub total: usize,
    /// Commands the file had applied when it was saved (its cursor).
    pub saved_cursor: usize,
    /// The copy shows the state after this many commands.
    pub shown_after: usize,
    /// Whether undo still reaches back through the commands before the damage.
    pub history_kept: bool,
    /// Whether the app opened an unsaved copy. Damage only in the redo commands after
    /// the cursor leaves the saved state whole, so the file itself opens.
    pub copy: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MovedTask {
    pub uid: u32,
    pub name: String,
    pub before: (i64, i64),
    pub after: (i64, i64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenReport {
    pub recovery: Option<Recovery>,
    /// Tasks this engine schedules on other dates than the engine that saved the file.
    pub moved: Vec<MovedTask>,
}

impl OpenReport {
    /// The warning to show after opening, in `lang`, if there is anything to say.
    pub fn message(&self, lang: Lang) -> Option<String> {
        let parts: Vec<String> = self
            .recovery
            .iter()
            .map(|recovery| match lang {
                Lang::Vi => recovery_message_vi(recovery),
                Lang::En | Lang::Ja => recovery_message_en(recovery),
            })
            .chain((!self.moved.is_empty()).then(|| moved_message(&self.moved, lang)))
            .collect();
        (!parts.is_empty()).then(|| parts.join("\n\n"))
    }
}

/// Leaf tasks whose start or finish differs between `before` and `after`, in outline order.
pub fn moved_tasks(before: &Project, after: &Project) -> Vec<MovedTask> {
    after
        .tasks
        .iter()
        .filter(|task| !task.summary)
        .filter_map(|task| {
            let old = before.task(task.uid)?;
            let (old_dates, new_dates) = ((old.start, old.finish), (task.start, task.finish));
            (old_dates != new_dates).then(|| MovedTask {
                uid: task.uid,
                name: task.name.clone(),
                before: old_dates,
                after: new_dates,
            })
        })
        .collect()
}

fn recovery_message_en(recovery: &Recovery) -> String {
    let position = format!("step {} of {}", recovery.at + 1, recovery.total);
    let cause = match &recovery.damage {
        Damage::Unreadable => format!("The file is damaged at {position}."),
        Damage::Newer => {
            format!("The file has {position} from a newer version of Sheafly.")
        }
        Damage::Failed(error) => format!(
            "Could not replay {position}: {}.",
            error.message(Lang::En)
        ),
    };
    if !recovery.copy {
        return format!(
            "{cause} The saved state opened in full; the redo steps from step {} on cannot \
             be used and will be dropped when you save.",
            recovery.at + 1
        );
    }
    let state = if recovery.shown_after == recovery.saved_cursor {
        "A copy opened at exactly the saved state.".to_string()
    } else {
        format!(
            "A copy opened at the state after step {} (the file was saved after step {}).",
            recovery.shown_after, recovery.saved_cursor
        )
    };
    let history = if recovery.history_kept {
        ""
    } else {
        " The copy has no undo history."
    };
    format!(
        "{cause} {state}{history} The original file is unchanged; save the copy as a new file."
    )
}

fn recovery_message_vi(recovery: &Recovery) -> String {
    let position = format!("thao tác {}/{}", recovery.at + 1, recovery.total);
    let cause = match &recovery.damage {
        Damage::Unreadable => format!("Tệp bị hỏng ở {position}."),
        Damage::Newer => {
            format!("Tệp có {position} do phiên bản mới hơn của Sheafly tạo ra.")
        }
        Damage::Failed(error) => format!("Không áp dụng lại được {position}: {error}."),
    };
    if !recovery.copy {
        return format!(
            "{cause} Trạng thái đã lưu mở đầy đủ; các thao tác làm lại từ thao tác {} trở đi \
             không dùng được và sẽ bị bỏ khi lưu.",
            recovery.at + 1
        );
    }
    let state = if recovery.shown_after == recovery.saved_cursor {
        "Đã mở một bản sao đúng ở trạng thái đã lưu.".to_string()
    } else {
        format!(
            "Đã mở một bản sao ở trạng thái sau thao tác {} (lúc lưu là {}).",
            recovery.shown_after, recovery.saved_cursor
        )
    };
    let history = if recovery.history_kept {
        ""
    } else {
        " Bản sao không có lịch sử hoàn tác."
    };
    format!("{cause} {state}{history} Tệp gốc giữ nguyên; hãy lưu bản sao thành tệp mới.")
}

fn moved_message(moved: &[MovedTask], lang: Lang) -> String {
    let count = moved.len();
    let heading = match lang {
        Lang::Vi => format!(
            "Phiên bản Sheafly này tính lịch khác phiên bản đã lưu tệp: {count} công việc \
             đổi ngày. Hãy kiểm tra trước khi lưu."
        ),
        Lang::En => format!(
            "This version of Sheafly schedules the project differently from the version \
             that saved the file: {count} {} moved. Check them before you save.",
            if count == 1 { "task" } else { "tasks" }
        ),
        Lang::Ja => format!(
            "このバージョンの Sheafly では、ファイルを保存したバージョンと異なるスケジュール計算が行われます: {count} 個のタスクの日付が変更されました。保存する前に確認してください。"
        ),
    };
    let mut lines = vec![heading];
    lines.extend(moved.iter().take(MOVED_LISTED).map(|task| {
        format!(
            "• {}: {} → {}",
            task.name,
            span(task.before, lang),
            span(task.after, lang)
        )
    }));
    if count > MOVED_LISTED {
        let rest = count - MOVED_LISTED;
        lines.push(match lang {
            Lang::Vi => format!("… và {rest} công việc khác."),
            Lang::En => format!("… and {rest} more {}.", if rest == 1 { "task" } else { "tasks" }),
            Lang::Ja => format!("… 他 {rest} 件のタスク。"),
        });
    }
    lines.join("\n")
}

pub(crate) fn span((start, finish): (i64, i64), lang: Lang) -> String {
    format!("{}–{}", day(start, lang), day(finish, lang))
}

/// A date as each language writes it: 05/01/2026, Jan 5, 2026, or 2026-01-05.
fn day(ms: i64, lang: Lang) -> String {
    let (date, _) = civil(ms);
    match lang {
        Lang::Vi => format!("{:02}/{:02}/{}", date.day(), date.month(), date.year()),
        Lang::En => date.format("%b %-d, %Y").to_string(),
        Lang::Ja => format!("{}-{:02}-{:02}", date.year(), date.month(), date.day()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeutil::timestamp;

    fn moved(count: usize) -> Vec<MovedTask> {
        (0..count)
            .map(|index| MovedTask {
                uid: index as u32 + 1,
                name: format!("Việc {}", index + 1),
                before: (timestamp(2026, 1, 5, 8, 0), timestamp(2026, 1, 6, 17, 0)),
                after: (timestamp(2026, 1, 7, 8, 0), timestamp(2026, 1, 8, 17, 0)),
            })
            .collect()
    }

    #[test]
    fn a_clean_open_has_no_message() {
        assert_eq!(OpenReport::default().message(Lang::Vi), None);
    }

    #[test]
    fn moved_tasks_are_listed_with_old_and_new_dates() {
        let report = OpenReport {
            recovery: None,
            moved: moved(12),
        };
        let message = report.message(Lang::Vi).unwrap();
        assert!(message.contains("12 công việc đổi ngày"), "{message}");
        assert!(
            message.contains("• Việc 1: 05/01/2026–06/01/2026 → 07/01/2026–08/01/2026"),
            "{message}"
        );
        assert!(message.contains("Việc 10"), "{message}");
        assert!(!message.contains("Việc 11:"), "{message}");
        assert!(message.contains("… và 2 công việc khác."), "{message}");
    }

    #[test]
    fn recovery_explains_the_cause_and_the_copy() {
        let recovery = Recovery {
            damage: Damage::Newer,
            at: 4,
            total: 9,
            saved_cursor: 9,
            shown_after: 4,
            history_kept: true,
            copy: true,
        };
        let message = OpenReport {
            recovery: Some(recovery),
            moved: Vec::new(),
        }
        .message(Lang::Vi)
        .unwrap();
        assert!(
            message.contains("thao tác 5/9 do phiên bản mới hơn"),
            "{message}"
        );
        assert!(
            message.contains("sau thao tác 4 (lúc lưu là 9)"),
            "{message}"
        );
        assert!(message.contains("Tệp gốc giữ nguyên"), "{message}");
        assert!(!message.contains("không có lịch sử"), "{message}");
    }

    #[test]
    fn moved_tasks_in_english_use_english_dates() {
        let report = OpenReport {
            recovery: None,
            moved: moved(11),
        };
        let message = report.message(Lang::En).unwrap();
        assert!(
            message.starts_with(
                "This version of Sheafly schedules the project differently from the \
                 version that saved the file: 11 tasks moved. Check them before you save."
            ),
            "{message}"
        );
        assert!(
            message.contains("• Việc 1: Jan 5, 2026–Jan 6, 2026 → Jan 7, 2026–Jan 8, 2026"),
            "{message}"
        );
        assert!(message.ends_with("… and 1 more task."), "{message}");
    }

    #[test]
    fn a_failed_step_in_english_names_the_engine_error_in_english() {
        let recovery = Recovery {
            damage: Damage::Failed(EngineError::TaskNotFound),
            at: 2,
            total: 5,
            saved_cursor: 5,
            shown_after: 2,
            history_kept: false,
            copy: true,
        };
        let report = OpenReport {
            recovery: Some(recovery),
            moved: Vec::new(),
        };
        assert_eq!(
            report.message(Lang::En).unwrap(),
            "Could not replay step 3 of 5: Task not found. A copy opened at the state after \
             step 2 (the file was saved after step 5). The copy has no undo history. The \
             original file is unchanged; save the copy as a new file."
        );
        let vietnamese = report.message(Lang::Vi).unwrap();
        assert!(
            vietnamese.contains("Không áp dụng lại được thao tác 3/5: Không tìm thấy công việc."),
            "{vietnamese}"
        );
    }

    #[test]
    fn redo_damage_in_english_says_the_file_opened() {
        let recovery = Recovery {
            damage: Damage::Unreadable,
            at: 6,
            total: 8,
            saved_cursor: 6,
            shown_after: 6,
            history_kept: true,
            copy: false,
        };
        let report = OpenReport {
            recovery: Some(recovery),
            moved: Vec::new(),
        };
        assert_eq!(
            report.message(Lang::En).unwrap(),
            "The file is damaged at step 7 of 8. The saved state opened in full; the redo \
             steps from step 7 on cannot be used and will be dropped when you save."
        );
    }
}
