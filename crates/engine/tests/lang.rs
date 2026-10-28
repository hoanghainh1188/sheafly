//! Messages in each language, and the default names of a new project. Data already in a
//! project never changes with the language.

use std::fs;

use rusqlite::Connection;

use sheafly_engine::command::Command;
use sheafly_engine::store::{open, save, FORMAT_VERSION};
use sheafly_engine::timeutil::timestamp;
use sheafly_engine::{Document, EngineError, Invalid, Lang, Project};

fn doc() -> Document {
    Document::from_project(Project::new("Ngôn ngữ", timestamp(2026, 1, 5, 8, 0)))
}

fn refused(command: Command) -> EngineError {
    doc().apply(command).unwrap_err()
}

#[test]
fn edit_errors_read_naturally_in_english() {
    let cases = [
        (
            refused(Command::SetPredecessors {
                uid: 1,
                text: "x?".into(),
            }),
            "Invalid predecessor: x?",
            "Tiền nhiệm không hợp lệ: x?",
        ),
        (
            refused(Command::RescheduleUncompleted),
            "The status date is not set",
            "Chưa đặt ngày trạng thái",
        ),
        (
            refused(Command::RenameCalendar {
                calendar_id: 99,
                name: "Ca đêm".into(),
            }),
            "That calendar does not exist",
            "Không có lịch này",
        ),
        (
            refused(Command::SplitTask {
                uid: 1,
                at: timestamp(2026, 1, 5, 9, 0),
                gap: 0,
            }),
            "The split length must be greater than 0",
            "Khoảng ngắt phải lớn hơn 0",
        ),
        (
            refused(Command::RenameTask {
                uid: 42,
                name: "Không có".into(),
            }),
            "Task not found",
            "Không tìm thấy công việc",
        ),
    ];
    for (error, en, vi) in cases {
        assert_eq!(error.message(Lang::En), en);
        assert_eq!(error.message(Lang::Vi), vi);
        assert_eq!(error.to_string(), vi, "Display stays Vietnamese");
    }
}

#[test]
fn file_errors_keep_the_system_detail() {
    let error = EngineError::NewerFormat(FORMAT_VERSION + 1);
    assert_eq!(
        error.message(Lang::En),
        format!(
            "This file was created by a newer version of Sheafly (format {}). Please \
             update the app.",
            FORMAT_VERSION + 1
        )
    );
    assert_eq!(
        EngineError::Store("disk I/O error".into()).message(Lang::En),
        "Could not read the file: disk I/O error"
    );
    assert_eq!(
        EngineError::Invalid(Invalid::OvertimeNeedsAssignment).message(Lang::En),
        "Assign a resource before entering overtime work"
    );
}

#[test]
fn a_damaged_file_is_explained_in_english() {
    let mut doc = doc();
    for index in 0..4 {
        doc.apply(Command::AddTask {
            name: format!("Việc {index}"),
            after_uid: None,
        })
        .unwrap();
    }
    let dir = std::env::temp_dir().join(format!("sheafly-lang-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("project.pdproj").to_string_lossy().to_string();
    save(&doc, &path).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "DELETE FROM checkpoint; UPDATE command_log SET payload = '{\"type\":\"teleport\"}' \
             WHERE seq = 2;",
        )
        .unwrap();

    let opened = open(&path).unwrap();
    let english = opened.report.message(Lang::En).unwrap();
    assert!(
        english.starts_with("The file has step 3 of 4 from a newer version of Sheafly."),
        "{english}"
    );
    assert!(
        english
            .contains("A copy opened at the state after step 2 (the file was saved after step 4)."),
        "{english}"
    );
    assert!(
        english.ends_with("The original file is unchanged; save the copy as a new file."),
        "{english}"
    );
    let vietnamese = opened.report.message(Lang::Vi).unwrap();
    assert!(vietnamese.contains("thao tác 3/4"), "{vietnamese}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn new_projects_take_default_names_in_their_language() {
    let english = Document::new_in(Lang::En);
    assert_eq!(english.project.name, "New project");
    assert_eq!(english.project.tasks[0].name, "Task 1");
    assert_eq!(english.project.calendars[0].name, "Standard");
    // The names are data in the base project, so undoing back to it keeps them.
    assert_eq!(english.base.tasks[0].name, "Task 1");

    let vietnamese = Document::new_default();
    assert_eq!(vietnamese.project.name, "Dự án mới");
    assert_eq!(vietnamese.project.tasks[0].name, "Công việc 1");
    assert_eq!(vietnamese.project.calendars[0].name, "Chuẩn");

    let japanese = Document::new_in(Lang::Ja);
    assert_eq!(japanese.project.name, "新規プロジェクト");
    assert_eq!(japanese.project.tasks[0].name, "タスク 1");
    assert_eq!(japanese.project.calendars[0].name, "標準");
    assert_eq!(japanese.base.tasks[0].name, "タスク 1");
}

#[test]
fn errors_in_japanese() {
    let cases = [
        (
            refused(Command::SetPredecessors {
                uid: 1,
                text: "x?".into(),
            }),
            "先行タスクが無効です: x?",
        ),
        (
            refused(Command::RescheduleUncompleted),
            "状況報告日が設定されていません",
        ),
        (
            refused(Command::RenameTask {
                uid: 42,
                name: "None".into(),
            }),
            "タスクが見つかりません",
        ),
    ];
    for (error, ja) in cases {
        assert_eq!(error.message(Lang::Ja), ja);
    }
}
