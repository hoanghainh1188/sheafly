//! The workbook's labels in each language. Sheet names stay under Excel's 31 characters
//! and avoid `[]:*?/\`.

use sheafly_engine::Lang;

pub(crate) struct Text {
    pub(crate) tasks_sheet: &'static str,
    pub(crate) phased_sheet: &'static str,
    pub(crate) matrix_sheet: &'static str,
    pub(crate) loading_sheet: &'static str,
    pub(crate) task_columns: [&'static str; 16],
    pub(crate) task_name: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) total_hours: &'static str,
    pub(crate) unassigned_rule: &'static str,
    pub(crate) at_least: &'static str,
    pub(crate) resource: &'static str,
    pub(crate) role: &'static str,
    pub(crate) measure: &'static str,
    pub(crate) work_hours: &'static str,
    pub(crate) capacity_hours: &'static str,
    pub(crate) yes: &'static str,
    pub(crate) no: &'static str,
}

const VI: Text = Text {
    tasks_sheet: "Kế hoạch & Công việc",
    phased_sheet: "Phân bổ công",
    matrix_sheet: "Ma trận phân công",
    loading_sheet: "Tải nguồn lực",
    task_columns: [
        "ID",
        "Tên công việc",
        "Cấp",
        "Thời lượng (ngày)",
        "Công việc (h)",
        "Bắt đầu",
        "Kết thúc",
        "Tiền nhiệm",
        "Nguồn lực",
        "% Xong",
        "Chi phí",
        "Chi phí thực tế",
        "Baseline Bắt đầu",
        "Baseline Kết thúc",
        "Baseline Chi phí",
        "Đường găng",
    ],
    task_name: "Tên công việc",
    summary: "Nhóm",
    total_hours: "Tổng cộng (h)",
    unassigned_rule: "Chưa giao (quy tắc)",
    at_least: "từ cấp",
    resource: "Nguồn lực",
    role: "Vai trò",
    measure: "Chỉ số",
    work_hours: "Công việc (h)",
    capacity_hours: "Khả năng (h)",
    yes: "Có",
    no: "Không",
};

const EN: Text = Text {
    tasks_sheet: "Tasks & Schedule",
    phased_sheet: "Time-Phased Work",
    matrix_sheet: "Assignment Matrix",
    loading_sheet: "Resource Loading",
    task_columns: [
        "ID",
        "Task Name",
        "Level",
        "Duration (Days)",
        "Work (Hours)",
        "Start",
        "Finish",
        "Predecessors",
        "Resources",
        "% Complete",
        "Cost",
        "Actual Cost",
        "Baseline Start",
        "Baseline Finish",
        "Baseline Cost",
        "Critical",
    ],
    task_name: "Task Name",
    summary: "Group",
    total_hours: "Total (h)",
    unassigned_rule: "Unassigned (Rule)",
    at_least: "at least",
    resource: "Resource",
    role: "Role",
    measure: "Measure",
    work_hours: "Work (h)",
    capacity_hours: "Capacity (h)",
    yes: "Yes",
    no: "No",
};

pub(crate) fn text(lang: Lang) -> &'static Text {
    match lang {
        Lang::Vi => &VI,
        Lang::En => &EN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_names_are_valid_in_excel() {
        for t in [&VI, &EN] {
            for name in [t.tasks_sheet, t.phased_sheet, t.matrix_sheet, t.loading_sheet] {
                assert!(name.chars().count() <= 31, "{name}");
                assert!(!name.contains(['[', ']', ':', '*', '?', '/', '\\']), "{name}");
            }
        }
    }
}
