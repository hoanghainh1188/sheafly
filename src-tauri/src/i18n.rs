//! Every user-facing string of the Tauri shell, in Vietnamese and English. Engine
//! messages come from the engine (`EngineError::message`, `OpenReport::message`).

use sheafly_engine::Lang;

use crate::settings::Theme;

/// Fixed text for one language.
pub struct Text {
    // Menu bar
    pub menu_file: &'static str,
    pub menu_edit: &'static str,
    pub menu_view: &'static str,
    pub menu_project: &'static str,
    pub menu_language: &'static str,
    pub menu_theme: &'static str,
    pub menu_help: &'static str,
    pub open_recent: &'static str,
    pub recent_clear: &'static str,
    pub recent_empty: &'static str,
    pub about: &'static str,
    pub hide: &'static str,
    pub hide_others: &'static str,
    pub show_all: &'static str,
    pub about_text: &'static str,
    pub copyright: &'static str,
    // Dialogs
    pub dialog_busy: &'static str,
    pub save: &'static str,
    pub dont_save: &'static str,
    pub cancel: &'static str,
    pub unsaved_title: &'static str,
    pub save_failed_title: &'static str,
    pub save_project_title: &'static str,
    pub open_project_title: &'static str,
    pub import_mspdi_title: &'static str,
    pub import_table_title: &'static str,
    pub table_filter: &'static str,
    pub plan_outdated: &'static str,
    pub export_mspdi_title: &'static str,
    pub export_mspdi_file: &'static str,
    pub export_pdf_title: &'static str,
    pub empty_pdf: &'static str,
    pub restore: &'static str,
    pub drop_autosave: &'static str,
    pub restore_title: &'static str,
    pub compact: &'static str,
    pub compact_title: &'static str,
    pub licenses_unavailable: &'static str,
    // Sheafly Pro
    pub license_not_removed: &'static str,
    pub plan_pro: &'static str,
    pub plan_business: &'static str,
    pub plan_trial: &'static str,
    pub licensed_to: &'static str,
    pub needs_pro: &'static str,
}

pub static VI: Text = Text {
    menu_file: "Tệp",
    menu_edit: "Sửa",
    menu_view: "Xem",
    menu_project: "Dự án",
    menu_language: "Ngôn ngữ",
    menu_theme: "Giao diện",
    menu_help: "Trợ giúp",
    open_recent: "Mở gần đây",
    recent_clear: "Xóa danh sách",
    recent_empty: "Chưa có tệp nào",
    about: "Giới thiệu Sheafly",
    hide: "Ẩn Sheafly",
    hide_others: "Ẩn ứng dụng khác",
    show_all: "Hiện tất cả",
    about_text: "Lập lịch dự án kiểu Microsoft Project, chạy trên máy, giao diện tiếng Việt và tiếng Anh.\n\n\
Đường găng, lịch làm việc, nguồn lực và cân bằng tải, theo dõi tiến độ, baseline, \
EVM, burn-down và đường cong S; năm view Gantt, Theo dõi, Nguồn lực, Sử dụng và \
Sơ đồ mạng; xuất PDF và trao đổi MSPDI với Microsoft Project.\n\n\
Engine lập lịch viết bằng Rust; giao diện Tauri 2 và React 19. \
Biểu tượng: Lucide (ISC). Phông PDF: Be Vietnam Pro (OFL). \
Giấy phép của mọi thành phần bên thứ ba: Trợ giúp → Giấy phép bên thứ ba.",
    copyright: "© 2026 Hoàng Hải Nguyễn · Giấy phép MIT",
    dialog_busy: "Đang mở một hộp thoại. Hãy đóng nó trước.",
    save: "Lưu",
    dont_save: "Không lưu",
    cancel: "Hủy",
    unsaved_title: "Thay đổi chưa lưu",
    save_failed_title: "Không lưu được dự án",
    save_project_title: "Lưu dự án",
    open_project_title: "Mở dự án",
    import_mspdi_title: "Nhập MSPDI",
    import_table_title: "Nhập bảng",
    table_filter: "Bảng (CSV, Excel)",
    plan_outdated: "Dự án đã thay đổi từ lúc xem trước. Hãy xem lại kế hoạch.",
    export_mspdi_title: "Xuất MSPDI",
    export_mspdi_file: "du-an.xml",
    export_pdf_title: "Xuất PDF",
    empty_pdf: "PDF trống",
    restore: "Khôi phục",
    drop_autosave: "Bỏ bản tự lưu",
    restore_title: "Khôi phục dự án",
    compact: "Gộp",
    compact_title: "Gộp lịch sử hoàn tác",
    licenses_unavailable: "Không mở được danh sách giấy phép bên thứ ba.",
    license_not_removed: "Không xóa được license trên máy này.",
    plan_pro: "Sheafly Pro",
    plan_business: "Sheafly Business",
    plan_trial: "Dùng thử Sheafly Pro",
    licensed_to: "cấp cho",
    needs_pro: "Cần Sheafly Pro để đổi năng suất hữu ích (Trợ giúp → License Sheafly Pro…).",
};

pub static EN: Text = Text {
    menu_file: "File",
    menu_edit: "Edit",
    menu_view: "View",
    menu_project: "Project",
    menu_language: "Language",
    menu_theme: "Appearance",
    menu_help: "Help",
    open_recent: "Open Recent",
    recent_clear: "Clear Menu",
    recent_empty: "No Recent Files",
    about: "About Sheafly",
    hide: "Hide Sheafly",
    hide_others: "Hide Others",
    show_all: "Show All",
    about_text: "Project scheduling in the style of Microsoft Project, running on your computer.\n\n\
Critical path, calendars, resources and leveling, progress tracking, baselines, \
EVM, burn-down and S-curves; five views: Gantt Chart, Tracking Gantt, Resource Sheet, \
Task Usage and Network Diagram; PDF export and MSPDI exchange with Microsoft Project.\n\n\
Scheduling engine written in Rust; interface built with Tauri 2 and React 19. \
Icons: Lucide (ISC). PDF font: Be Vietnam Pro (OFL). \
Licenses of all third-party components: Help → Third-Party Licenses.",
    copyright: "© 2026 Hoàng Hải Nguyễn · MIT License",
    dialog_busy: "A dialog is already open. Close it first.",
    save: "Save",
    dont_save: "Don’t Save",
    cancel: "Cancel",
    unsaved_title: "Unsaved Changes",
    save_failed_title: "Could Not Save the Project",
    save_project_title: "Save Project",
    open_project_title: "Open Project",
    import_mspdi_title: "Import MSPDI",
    import_table_title: "Import a table",
    table_filter: "Table (CSV, Excel)",
    plan_outdated: "The project changed since the preview. Review the plan again.",
    export_mspdi_title: "Export MSPDI",
    export_mspdi_file: "project.xml",
    export_pdf_title: "Export PDF",
    empty_pdf: "The PDF is empty",
    restore: "Restore",
    drop_autosave: "Discard Autosave",
    restore_title: "Restore Project",
    compact: "Compact",
    compact_title: "Compact Undo History",
    licenses_unavailable: "The third-party licenses could not be opened.",
    license_not_removed: "The license could not be removed from this computer.",
    plan_pro: "Sheafly Pro",
    plan_business: "Sheafly Business",
    plan_trial: "Sheafly Pro trial",
    licensed_to: "licensed to",
    needs_pro: "Changing the useful productivity needs Sheafly Pro (Help → Sheafly Pro License…).",
};

pub fn text(lang: Lang) -> &'static Text {
    lang.pick(&VI, &EN)
}

/// The About panel's licence line: "Sheafly Pro, licensed to Lan".
pub fn licensed_to(text: &Text, plan: sheafly_license::Plan, name: &str) -> String {
    let plan = match plan {
        sheafly_license::Plan::Pro => text.plan_pro,
        sheafly_license::Plan::Business => text.plan_business,
        sheafly_license::Plan::Trial => text.plan_trial,
    };
    format!("{plan}, {} {name}", text.licensed_to)
}

/// One entry of a menu built from a table.
pub enum Entry {
    Item {
        id: &'static str,
        vi: &'static str,
        en: &'static str,
        accelerator: Option<&'static str>,
    },
    /// A system item (Cut, Copy, Paste, Select All) that edits the focused text field.
    /// The webview turns Copy and Paste outside a field into the page's `copy` and
    /// `paste` events, which copy and paste the selected tasks.
    Native {
        kind: NativeItem,
        vi: &'static str,
        en: &'static str,
    },
    Separator,
    /// A submenu of related items; `id` only names it for its icon and the tests.
    Submenu {
        id: &'static str,
        vi: &'static str,
        en: &'static str,
        entries: &'static [Entry],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeItem {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

const fn native(kind: NativeItem, vi: &'static str, en: &'static str) -> Entry {
    Entry::Native { kind, vi, en }
}

const fn item(
    id: &'static str,
    vi: &'static str,
    en: &'static str,
    accelerator: Option<&'static str>,
) -> Entry {
    Entry::Item {
        id,
        vi,
        en,
        accelerator,
    }
}

const fn group(id: &'static str, vi: &'static str, en: &'static str, entries: &'static [Entry]) -> Entry {
    Entry::Submenu { id, vi, en, entries }
}

/// The custom quit item. Not the predefined Quit: it would exit without asking about
/// unsaved changes.
pub const QUIT: Entry = item(
    "quit",
    "Thoát Sheafly",
    "Quit Sheafly",
    Some("CmdOrCtrl+Q"),
);

pub const FILE_MENU: &[Entry] = &[
    item("new", "Dự án mới", "New Project", Some("CmdOrCtrl+N")),
    item("open", "Mở…", "Open…", Some("CmdOrCtrl+O")),
    item("save", "Lưu", "Save", Some("CmdOrCtrl+S")),
    item(
        "save-as",
        "Lưu thành…",
        "Save As…",
        Some("CmdOrCtrl+Shift+S"),
    ),
    Entry::Separator,
    group(
        "import",
        "Nhập",
        "Import",
        &[
            item("import-xml", "Nhập MSPDI…", "Import MSPDI…", None),
            item("import-table", "Nhập bảng (CSV, Excel)…", "Import Table (CSV, Excel)…", None),
            item("pull-tracker", "Kéo từ task tracker…", "Pull from Task Tracker…", None),
        ],
    ),
    group(
        "export",
        "Xuất",
        "Export",
        &[
            item("export-xml", "Xuất MSPDI…", "Export MSPDI…", None),
            item("export-pdf", "Xuất PDF", "Export PDF", None),
            item("push-tracker", "Đẩy ngày về task tracker…", "Push Dates to Task Tracker…", None),
        ],
    ),
];

/// The Help menu. License opens the frontend's license dialog; the licenses item opens the
/// file the installer ships (`THIRD_PARTY_LICENSES.txt`) in the system's text viewer.
pub const HELP_MENU: &[Entry] = &[
    item("license", "License Sheafly Pro…", "Sheafly Pro License…", None),
    item("third-party-licenses", "Giấy phép bên thứ ba", "Third-Party Licenses", None),
];

pub const EDIT_MENU: &[Entry] = &[
    item("undo", "Hoàn tác", "Undo", Some("CmdOrCtrl+Z")),
    item("redo", "Làm lại", "Redo", Some("CmdOrCtrl+Shift+Z")),
    Entry::Separator,
    native(NativeItem::Cut, "Cắt", "Cut"),
    native(NativeItem::Copy, "Sao chép", "Copy"),
    native(NativeItem::Paste, "Dán", "Paste"),
    native(NativeItem::SelectAll, "Chọn tất cả", "Select All"),
    Entry::Separator,
    item(
        "copy-task",
        "Sao chép công việc",
        "Copy Task",
        Some("CmdOrCtrl+Alt+C"),
    ),
    item(
        "paste-task",
        "Dán công việc",
        "Paste Task",
        Some("CmdOrCtrl+Alt+V"),
    ),
    item("fill-down", "Điền xuống", "Fill Down", Some("CmdOrCtrl+D")),
    Entry::Separator,
    item("indent", "Thụt vào", "Indent", Some("CmdOrCtrl+]")),
    item("outdent", "Thụt ra", "Outdent", Some("CmdOrCtrl+[")),
    item(
        "delete",
        "Xóa công việc",
        "Delete Task",
        Some("CmdOrCtrl+Backspace"),
    ),
    Entry::Separator,
    group(
        "task-tools",
        "Công việc",
        "Task",
        &[
            item("recurring", "Công việc lặp…", "Recurring Task…", None),
            item("split-task", "Tách công việc…", "Split Task…", None),
        ],
    ),
];

/// The View menu, before the language submenu.
pub const VIEW_MENU: &[Entry] = &[
    item("view-gantt", "Gantt", "Gantt Chart", None),
    item("view-tracking", "Gantt theo dõi", "Tracking Gantt", None),
    item("view-resources", "Bảng nguồn lực", "Resource Sheet", None),
    item("view-matrix", "Ma trận phân công", "Assignment Matrix", None),
    item("view-usage", "Sử dụng công việc", "Task Usage", None),
    item("view-network", "Sơ đồ mạng", "Network Diagram", None),
    item("view-load", "Tải nguồn lực", "Resource Load", None),
    item("view-forecast", "Dự báo ngày kết thúc", "Finish Forecast", None),
    item(
        "view-evm",
        "Chỉ số EVM và burn-down",
        "Earned Value and Burn-down",
        None,
    ),
    Entry::Separator,
    group(
        "timescale",
        "Thang thời gian",
        "Timescale",
        &[
            item("scale-day", "Thang ngày", "Timescale: Days", None),
            item("scale-week", "Thang tuần", "Timescale: Weeks", None),
            item("scale-month", "Thang tháng", "Timescale: Months", None),
            Entry::Separator,
            item("zoom-in", "Phóng to thời gian", "Zoom In Timescale", Some("CmdOrCtrl+=")),
            item("zoom-out", "Thu nhỏ thời gian", "Zoom Out Timescale", Some("CmdOrCtrl+-")),
            item("zoom-reset", "Cỡ thời gian gốc", "Actual Timescale Size", Some("CmdOrCtrl+0")),
        ],
    ),
];

pub const PROJECT_MENU: &[Entry] = &[
    group(
        "project-setup",
        "Thiết lập dự án",
        "Project Setup",
        &[
            item("project-info", "Thông tin dự án…", "Project Information…", None),
            item("custom-fields", "Trường tùy chỉnh…", "Custom Fields…", None),
            item("working-time", "Lịch làm việc…", "Working Time…", None),
            item("tracker-settings", "Đồng bộ task tracker…", "Task Tracker Sync…", None),
        ],
    ),
    Entry::Separator,
    group(
        "leveling",
        "Cân bằng",
        "Leveling",
        &[
            item("level", "Cân bằng nguồn lực", "Level Resources", None),
            item("clear-level", "Xóa độ trễ cân bằng", "Clear Leveling", None),
            Entry::Separator,
            item("leveling-order", "Thứ tự cân bằng…", "Leveling Order…", None),
        ],
    ),
    group(
        "assigning",
        "Giao việc",
        "Assigning",
        &[
            item("assign-by-rules", "Giao việc theo quy tắc…", "Assign by Rules…", None),
            item("assignment-rules", "Quy tắc giao việc…", "Assignment Rules…", None),
        ],
    ),
    group(
        "tracking",
        "Theo dõi",
        "Tracking",
        &[
            item("baseline", "Đường cơ sở…", "Baseline…", None),
            item(
                "reschedule-uncompleted",
                "Dời phần chưa xong sau ngày trạng thái",
                "Reschedule Uncompleted Work After the Status Date",
                None,
            ),
        ],
    ),
    Entry::Separator,
    item(
        "compact-history",
        "Gộp lịch sử hoàn tác…",
        "Compact Undo History…",
        None,
    ),
];

/// The language choices: menu id, language, and its name in its own language. The
/// names are the same in both menus.
pub const LANGUAGES: [(&str, Lang, &str); 2] = [
    ("lang-vi", Lang::Vi, "Tiếng Việt"),
    ("lang-en", Lang::En, "English"),
];

/// The theme choices: menu id, theme, and the label in Vietnamese and English.
pub const THEMES: [(&str, Theme, &str, &str); 3] = [
    ("theme-system", Theme::System, "Theo hệ thống", "System"),
    ("theme-light", Theme::Light, "Sáng", "Light"),
    ("theme-dark", Theme::Dark, "Tối", "Dark"),
];

/// "Save changes?" for the project called `name`.
pub fn unsaved_message(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Vi => format!("Dự án “{name}” có thay đổi chưa lưu. Lưu trước khi tiếp tục?"),
        Lang::En => format!(
            "The project “{name}” has unsaved changes. Do you want to save them before \
             continuing?"
        ),
    }
}

/// The offer to restore autosaved work. `file` is the name of the file it came from.
pub fn autosave_offer(lang: Lang, project: &str, file: Option<&str>) -> String {
    match lang {
        Lang::Vi => {
            let source = file
                .map(|file| format!(" của tệp “{file}”"))
                .unwrap_or_default();
            format!(
                "Sheafly đã đóng khi dự án “{project}”{source} còn thay đổi chưa lưu. \
                 Khôi phục bản tự lưu?"
            )
        }
        Lang::En => {
            let source = file
                .map(|file| format!(" (file “{file}”)"))
                .unwrap_or_default();
            format!(
                "Sheafly closed while the project “{project}”{source} had unsaved \
                 changes. Restore the autosaved version?"
            )
        }
    }
}

pub fn restore_failed(lang: Lang, detail: &str) -> String {
    match lang {
        Lang::Vi => format!("Không khôi phục được bản tự lưu: {detail}"),
        Lang::En => format!("Could not restore the autosaved version: {detail}"),
    }
}

/// The undo history is short enough to keep whole.
pub fn compact_not_needed(lang: Lang, undoable: usize, kept: usize) -> String {
    match lang {
        Lang::Vi => format!(
            "Lịch sử hoàn tác mới có {undoable} thao tác, chưa cần gộp (giữ {kept} thao tác \
             gần nhất)."
        ),
        Lang::En => format!(
            "The undo history has only {undoable} {}, so there is nothing to compact yet \
             (the last {kept} steps are always kept).",
            if undoable == 1 { "step" } else { "steps" }
        ),
    }
}

pub fn compact_question(lang: Lang, folded: usize, kept: usize) -> String {
    match lang {
        Lang::Vi => format!(
            "Gộp {folded} thao tác cũ vào trạng thái gốc và giữ {kept} thao tác gần nhất để \
             hoàn tác? Không hoàn tác được việc gộp. Tệp nhỏ lại sau lần lưu tới."
        ),
        Lang::En => format!(
            "Fold {folded} older {} into the starting state and keep the last {kept} steps \
             to undo? Compacting cannot be undone. The file gets smaller the next time you \
             save.",
            if folded == 1 { "step" } else { "steps" }
        ),
    }
}

pub fn unknown_theme(lang: Lang, code: &str) -> String {
    match lang {
        Lang::Vi => format!("Giao diện không được hỗ trợ: {code}"),
        Lang::En => format!("Unsupported appearance: {code}"),
    }
}

pub fn unknown_language(lang: Lang, code: &str) -> String {
    match lang {
        Lang::Vi => format!("Ngôn ngữ không được hỗ trợ: {code}"),
        Lang::En => format!("Unsupported language: {code}"),
    }
}

pub fn settings_not_saved(lang: Lang, detail: &str) -> String {
    match lang {
        Lang::Vi => format!("Không lưu được thiết lập: {detail}"),
        Lang::En => format!("Could not save the settings: {detail}"),
    }
}

/// A file from Open Recent (or opened from the system) that is no longer there.
pub fn file_missing(lang: Lang, path: &str) -> String {
    match lang {
        Lang::Vi => format!("Không tìm thấy tệp “{path}”. Tệp đã được bỏ khỏi danh sách gần đây."),
        Lang::En => format!("“{path}” could not be found. It has been removed from Open Recent."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_entries() -> impl Iterator<Item = &'static Entry> {
        fn flatten(entries: &'static [Entry], out: &mut Vec<&'static Entry>) {
            for entry in entries {
                out.push(entry);
                if let Entry::Submenu { entries, .. } = entry {
                    flatten(entries, out);
                }
            }
        }
        let mut out = Vec::new();
        for menu in [FILE_MENU, EDIT_MENU, VIEW_MENU, PROJECT_MENU, HELP_MENU] {
            flatten(menu, &mut out);
        }
        out.push(&QUIT);
        out.into_iter()
    }

    #[test]
    fn every_menu_item_has_both_languages_and_a_unique_id() {
        let mut ids = std::collections::HashSet::new();
        for entry in all_entries() {
            if let Entry::Item { id, vi, en, .. } | Entry::Submenu { id, vi, en, .. } = entry {
                assert!(!vi.trim().is_empty(), "{id} has no Vietnamese label");
                assert!(!en.trim().is_empty(), "{id} has no English label");
                assert!(ids.insert(*id), "duplicate menu id {id}");
            }
        }
        for (id, _, name) in LANGUAGES {
            assert!(!name.is_empty());
            assert!(ids.insert(id), "duplicate menu id {id}");
        }
        for (id, _, vi, en) in THEMES {
            assert!(!vi.is_empty() && !en.is_empty() && vi != en, "{id}");
            assert!(ids.insert(id), "duplicate menu id {id}");
        }
    }

    #[test]
    fn english_labels_are_translated() {
        // Every label is translated; the Vietnamese "Gantt" is "Gantt Chart" in English.
        for entry in all_entries() {
            if let Entry::Native { vi, en, .. } = entry {
                assert_ne!(vi, en);
            }
            if let Entry::Item { id, vi, en, .. } = entry {
                assert_ne!(vi, en, "{id} is not translated");
                assert!(
                    en.chars().all(|ch| ch.is_ascii() || "…’".contains(ch)),
                    "{id}: {en}"
                );
            }
        }
    }

    #[test]
    fn quit_keeps_its_accelerator() {
        let quit = match QUIT {
            Entry::Item { accelerator, .. } => accelerator,
            Entry::Native { .. } | Entry::Separator | Entry::Submenu { .. } => None,
        };
        assert_eq!(quit, Some("CmdOrCtrl+Q"));
    }

    #[test]
    fn language_names_are_written_in_their_own_language() {
        assert_eq!(LANGUAGES[0], ("lang-vi", Lang::Vi, "Tiếng Việt"));
        assert_eq!(LANGUAGES[1], ("lang-en", Lang::En, "English"));
    }

    #[test]
    fn fixed_text_differs_between_languages() {
        let pairs = [
            (VI.menu_file, EN.menu_file),
            (VI.menu_view, EN.menu_view),
            (VI.menu_language, EN.menu_language),
            (VI.menu_theme, EN.menu_theme),
            (VI.save, EN.save),
            (VI.dont_save, EN.dont_save),
            (VI.cancel, EN.cancel),
            (VI.dialog_busy, EN.dialog_busy),
            (VI.about_text, EN.about_text),
            (VI.copyright, EN.copyright),
            (VI.export_mspdi_file, EN.export_mspdi_file),
        ];
        for (vi, en) in pairs {
            assert!(!vi.is_empty() && !en.is_empty());
            assert_ne!(vi, en);
        }
        assert!(EN.about_text.starts_with(
            "Project scheduling in the style of Microsoft Project, running on your computer."
        ));
        assert_eq!(
            (EN.save, EN.dont_save, EN.cancel),
            ("Save", "Don’t Save", "Cancel")
        );
    }

    #[test]
    fn formatted_messages_name_their_subject() {
        assert_eq!(
            unsaved_message(Lang::En, "Bridge"),
            "The project “Bridge” has unsaved changes. Do you want to save them before \
             continuing?"
        );
        assert!(
            autosave_offer(Lang::En, "Bridge", Some("bridge.pdproj"))
                .contains("(file “bridge.pdproj”)")
        );
        assert!(autosave_offer(Lang::Vi, "Cầu", None).contains("dự án “Cầu” còn"));
        assert!(compact_not_needed(Lang::En, 1, 100).contains("only 1 step,"));
        assert!(compact_question(Lang::En, 20, 100).starts_with("Fold 20 older steps"));
        assert_eq!(licensed_to(&EN, sheafly_license::Plan::Pro, "Lan"), "Sheafly Pro, licensed to Lan");
        assert_eq!(licensed_to(&VI, sheafly_license::Plan::Trial, "Lan"), "Dùng thử Sheafly Pro, cấp cho Lan");
    }
}
