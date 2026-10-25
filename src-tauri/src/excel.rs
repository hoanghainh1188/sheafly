//! Excel workbook exporter for Sheafly.
//!
//! Generates a multi-sheet corporate workbook (.xlsx) containing:
//! 1. Tasks & Schedule: Hierarchy, outline indents, dates, costs, baselines, and critical path.
//! 2. Time-Phased Work: Distributed planned work across weeks/months with SUM formulas.
//! 3. Assignment Matrix: RACI-style resource assignments per task.
//! 4. Resource Loading: Monthly workload vs capacity highlighting overallocations.

use std::collections::HashMap;

use chrono::{Datelike, DateTime, Timelike, Utc};
use rust_xlsxwriter::{
    Color, ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook,
};
use sheafly_engine::assignment_matrix::assignment_matrix;
use sheafly_engine::curves::{curves, CurveBucket, CurveFilter, CurveMeasure};
use sheafly_engine::load_map::{load_map, Bucket, LoadGroup};
use sheafly_engine::model::Project;
use sheafly_engine::Lang;

/// Converts project timestamp milliseconds to an ExcelDateTime.
fn instant_to_excel_date(ms: i64) -> Option<ExcelDateTime> {
    let dt = DateTime::<Utc>::from_timestamp_millis(ms)?;
    ExcelDateTime::from_ymd(dt.year() as u16, dt.month() as u8, dt.day() as u8)
        .ok()?
        .and_hms(dt.hour() as u16, dt.minute() as u8, dt.second() as u8)
        .ok()
}

/// Converts a 0-indexed column number to Excel column letters (e.g. 0 -> "A", 27 -> "AB").
fn col_to_letter(mut col: u32) -> String {
    let mut s = String::new();
    loop {
        let rem = (col % 26) as u8;
        s.push((b'A' + rem) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s.chars().rev().collect()
}

/// Palette and styling tokens for the Excel export.
struct ExcelStyles {
    header_fmt: Format,
    summary_row_fmt: Format,
    summary_date_fmt: Format,
    summary_num_fmt: Format,
    summary_curr_fmt: Format,
    task_name_fmt: Format,
    date_fmt: Format,
    num_fmt: Format,
    percent_fmt: Format,
    currency_fmt: Format,
    center_fmt: Format,
    overallocated_fmt: Format,
    total_label_fmt: Format,
    total_formula_fmt: Format,
}

impl ExcelStyles {
    fn new(lang: Lang) -> Self {
        let currency_pattern = match lang {
            Lang::Vi => "#,##0 \"₫\"",
            Lang::En => "$#,##0",
        };

        let header_fmt = Format::new()
            .set_bold()
            .set_font_color(Color::RGB(0xFFFFFF))
            .set_background_color(Color::RGB(0x1E3A8A)) // Navy Blue
            .set_align(FormatAlign::Center)
            .set_align(FormatAlign::VerticalCenter)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xCBD5E1));

        let summary_row_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xF1F5F9)) // Slate 100
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let summary_date_fmt = summary_row_fmt.clone().set_num_format("yyyy-mm-dd hh:mm");
        let summary_num_fmt = summary_row_fmt.clone().set_num_format("0.0");
        let summary_curr_fmt = summary_row_fmt.clone().set_num_format(currency_pattern);

        let task_name_fmt = Format::new()
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let date_fmt = Format::new()
            .set_num_format("yyyy-mm-dd hh:mm")
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let num_fmt = Format::new()
            .set_num_format("0.0")
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let percent_fmt = Format::new()
            .set_num_format("0.0%")
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let currency_fmt = Format::new()
            .set_num_format(currency_pattern)
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let center_fmt = Format::new()
            .set_align(FormatAlign::Center)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xE2E8F0));

        let overallocated_fmt = Format::new()
            .set_bold()
            .set_font_color(Color::RGB(0xB91C1C)) // Red 700
            .set_background_color(Color::RGB(0xFEE2E2)) // Red 100
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0xFCA5A5));

        let total_label_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0xE2E8F0))
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0x94A3B8));

        let total_formula_fmt = Format::new()
            .set_bold()
            .set_num_format("0.0")
            .set_background_color(Color::RGB(0xE2E8F0))
            .set_align(FormatAlign::Right)
            .set_border(FormatBorder::Thin)
            .set_border_color(Color::RGB(0x94A3B8));

        Self {
            header_fmt,
            summary_row_fmt,
            summary_date_fmt,
            summary_num_fmt,
            summary_curr_fmt,
            task_name_fmt,
            date_fmt,
            num_fmt,
            percent_fmt,
            currency_fmt,
            center_fmt,
            overallocated_fmt,
            total_label_fmt,
            total_formula_fmt,
        }
    }
}

fn build_workbook(project: &Project, lang: Lang) -> Result<Workbook, String> {
    let mut workbook = Workbook::new();
    let styles = ExcelStyles::new(lang);

    // 1. Tasks & Schedule Sheet
    write_tasks_sheet(&mut workbook, project, &styles, lang)?;

    // 2. Time-Phased Work Sheet
    write_time_phased_sheet(&mut workbook, project, &styles, lang)?;

    // 3. Assignment Matrix Sheet
    write_assignment_matrix_sheet(&mut workbook, project, &styles, lang)?;

    // 4. Resource Loading Sheet
    write_resource_loading_sheet(&mut workbook, project, &styles, lang)?;

    Ok(workbook)
}

/// Exports the given project to a multi-sheet Excel file at `path`.
pub fn export_excel_workbook(project: &Project, path: &str, lang: Lang) -> Result<(), String> {
    let mut workbook = build_workbook(project, lang)?;
    workbook.save(path).map_err(|err| err.to_string())
}

/// Exports the given project to a multi-sheet Excel byte buffer.
pub fn export_excel_bytes(project: &Project, lang: Lang) -> Result<Vec<u8>, String> {
    let mut workbook = build_workbook(project, lang)?;
    workbook.save_to_buffer().map_err(|err| err.to_string())
}

/// Writes Sheet 1: Tasks & Schedule.
fn write_tasks_sheet(
    workbook: &mut Workbook,
    project: &Project,
    styles: &ExcelStyles,
    lang: Lang,
) -> Result<(), String> {
    let sheet_name = match lang {
        Lang::Vi => "Kế hoạch & Công việc",
        Lang::En => "Tasks & Schedule",
    };
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(sheet_name)
        .map_err(|err| err.to_string())?;

    let headers = match lang {
        Lang::Vi => vec![
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
        Lang::En => vec![
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
    };

    // Write header row
    for (col_idx, &header) in headers.iter().enumerate() {
        worksheet
            .write_string_with_format(0, col_idx as u16, header, &styles.header_fmt)
            .map_err(|err| err.to_string())?;
    }

    // Map resource UIDs to names for quick lookup
    let resource_map: HashMap<u32, &str> = project
        .resources
        .iter()
        .map(|r| (r.uid, r.name.as_str()))
        .collect();

    // Map task UIDs to assigned resource string
    let mut task_resources: HashMap<u32, Vec<&str>> = HashMap::new();
    for a in &project.assignments {
        if let Some(&name) = resource_map.get(&a.resource_uid) {
            task_resources.entry(a.task_uid).or_default().push(name);
        }
    }

    // Write task rows
    for (row_idx, task) in project.tasks.iter().enumerate() {
        let row = (row_idx + 1) as u32;
        let is_summary = task.summary;

        // 0: ID
        worksheet
            .write_number_with_format(row, 0, task.id as f64, &styles.center_fmt)
            .map_err(|err| err.to_string())?;

        // 1: Name with indent
        let indent = (task.outline_level.saturating_sub(1)) as u8;
        let name_fmt = if is_summary {
            styles.summary_row_fmt.clone().set_indent(indent)
        } else {
            styles.task_name_fmt.clone().set_indent(indent)
        };
        worksheet
            .write_string_with_format(row, 1, &task.name, &name_fmt)
            .map_err(|err| err.to_string())?;

        // 2: Level
        worksheet
            .write_number_with_format(row, 2, task.outline_level as f64, &styles.center_fmt)
            .map_err(|err| err.to_string())?;

        // 3: Duration (days)
        let dur_days = task.duration as f64 / 480.0;
        let dur_fmt = if is_summary {
            &styles.summary_num_fmt
        } else {
            &styles.num_fmt
        };
        worksheet
            .write_number_with_format(row, 3, dur_days, dur_fmt)
            .map_err(|err| err.to_string())?;

        // 4: Work (hours)
        let work_hours = task.work as f64 / 60.0;
        let work_fmt = if is_summary {
            &styles.summary_num_fmt
        } else {
            &styles.num_fmt
        };
        worksheet
            .write_number_with_format(row, 4, work_hours, work_fmt)
            .map_err(|err| err.to_string())?;

        // 5: Start Date
        if let Some(start_dt) = instant_to_excel_date(task.start) {
            let s_fmt = if is_summary {
                &styles.summary_date_fmt
            } else {
                &styles.date_fmt
            };
            worksheet
                .write_datetime_with_format(row, 5, &start_dt, s_fmt)
                .map_err(|err| err.to_string())?;
        }

        // 6: Finish Date
        if let Some(finish_dt) = instant_to_excel_date(task.finish) {
            let f_fmt = if is_summary {
                &styles.summary_date_fmt
            } else {
                &styles.date_fmt
            };
            worksheet
                .write_datetime_with_format(row, 6, &finish_dt, f_fmt)
                .map_err(|err| err.to_string())?;
        }

        // 7: Predecessors
        worksheet
            .write_string_with_format(row, 7, &task.predecessors, &styles.center_fmt)
            .map_err(|err| err.to_string())?;

        // 8: Resources
        let res_str = task_resources
            .get(&task.uid)
            .map(|names| names.join(", "))
            .unwrap_or_default();
        worksheet
            .write_string_with_format(row, 8, &res_str, &styles.task_name_fmt)
            .map_err(|err| err.to_string())?;

        // 9: % Complete
        let pct = task.percent_complete / 100.0;
        worksheet
            .write_number_with_format(row, 9, pct, &styles.percent_fmt)
            .map_err(|err| err.to_string())?;

        // 10: Cost
        let cost_fmt = if is_summary {
            &styles.summary_curr_fmt
        } else {
            &styles.currency_fmt
        };
        worksheet
            .write_number_with_format(row, 10, task.cost, cost_fmt)
            .map_err(|err| err.to_string())?;

        // 11: Actual Cost
        worksheet
            .write_number_with_format(row, 11, task.actual_cost, cost_fmt)
            .map_err(|err| err.to_string())?;

        // Baseline columns (Baseline 0)
        let baseline = task.baselines.iter().find(|b| b.number == 0);
        if let Some(b) = baseline {
            if let Some(b_start) = instant_to_excel_date(b.start) {
                worksheet
                    .write_datetime_with_format(row, 12, &b_start, &styles.date_fmt)
                    .map_err(|err| err.to_string())?;
            }
            if let Some(b_finish) = instant_to_excel_date(b.finish) {
                worksheet
                    .write_datetime_with_format(row, 13, &b_finish, &styles.date_fmt)
                    .map_err(|err| err.to_string())?;
            }
            worksheet
                .write_number_with_format(row, 14, b.cost, &styles.currency_fmt)
                .map_err(|err| err.to_string())?;
        }

        // 15: Critical
        let is_crit_str = if task.critical {
            match lang {
                Lang::Vi => "Có",
                Lang::En => "Yes",
            }
        } else {
            match lang {
                Lang::Vi => "Không",
                Lang::En => "No",
            }
        };
        worksheet
            .write_string_with_format(row, 15, is_crit_str, &styles.center_fmt)
            .map_err(|err| err.to_string())?;
    }

    // Sheet layout options
    worksheet.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;
    let last_col = (headers.len() - 1) as u16;
    let last_row = project.tasks.len() as u32;
    worksheet
        .autofilter(0, 0, last_row, last_col)
        .map_err(|e| e.to_string())?;
    worksheet.autofit();

    Ok(())
}

/// Writes Sheet 2: Time-Phased Work across weekly or monthly buckets.
fn write_time_phased_sheet(
    workbook: &mut Workbook,
    project: &Project,
    styles: &ExcelStyles,
    lang: Lang,
) -> Result<(), String> {
    let sheet_name = match lang {
        Lang::Vi => "Phân bổ công",
        Lang::En => "Time-Phased Work",
    };
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(sheet_name)
        .map_err(|err| err.to_string())?;

    // Generate monthly curve data for work
    let rep = curves(
        project,
        CurveMeasure::Work,
        CurveBucket::Month,
        CurveFilter::Project,
        None,
    )
    .map_err(|e| e.to_string())?;
    let points = &rep.points;

    // Headers: ID, Task Name, [Month 1, Month 2, ...], Total
    let id_header = "ID";
    let name_header = match lang {
        Lang::Vi => "Tên công việc",
        Lang::En => "Task Name",
    };
    let total_header = match lang {
        Lang::Vi => "Tổng cộng (h)",
        Lang::En => "Total (h)",
    };

    worksheet
        .write_string_with_format(0, 0, id_header, &styles.header_fmt)
        .map_err(|e| e.to_string())?;
    worksheet
        .write_string_with_format(0, 1, name_header, &styles.header_fmt)
        .map_err(|e| e.to_string())?;

    for (p_idx, p) in points.iter().enumerate() {
        let col = (2 + p_idx) as u16;
        worksheet
            .write_string_with_format(0, col, &p.label, &styles.header_fmt)
            .map_err(|e| e.to_string())?;
    }

    let total_col = (2 + points.len()) as u16;
    worksheet
        .write_string_with_format(0, total_col, total_header, &styles.header_fmt)
        .map_err(|e| e.to_string())?;

    // Fill task rows
    for (row_idx, task) in project.tasks.iter().enumerate() {
        let row = (row_idx + 1) as u32;

        worksheet
            .write_number_with_format(row, 0, task.id as f64, &styles.center_fmt)
            .map_err(|e| e.to_string())?;

        let indent = (task.outline_level.saturating_sub(1)) as u8;
        let name_fmt = if task.summary {
            styles.summary_row_fmt.clone().set_indent(indent)
        } else {
            styles.task_name_fmt.clone().set_indent(indent)
        };
        worksheet
            .write_string_with_format(row, 1, &task.name, &name_fmt)
            .map_err(|e| e.to_string())?;

        // Distribute leaf task work evenly over scheduled period points that overlap
        let task_work_h = task.work as f64 / 60.0;
        let matching_cols: Vec<u16> = points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.date >= task.start && p.date <= task.finish + 30 * 86_400_000)
            .map(|(i, _)| (2 + i) as u16)
            .collect();

        let hours_per_col = if !matching_cols.is_empty() {
            task_work_h / matching_cols.len() as f64
        } else {
            0.0
        };

        for col_idx in 0..points.len() {
            let col = (2 + col_idx) as u16;
            let val = if matching_cols.contains(&col) {
                hours_per_col
            } else {
                0.0
            };
            worksheet
                .write_number_with_format(row, col, val, &styles.num_fmt)
                .map_err(|e| e.to_string())?;
        }

        // Row Total Formula: =SUM(C{row}:{last_col}{row})
        if !points.is_empty() {
            let first_col_letter = col_to_letter(2);
            let last_point_letter = col_to_letter((1 + points.len()) as u32);
            let row_num = row + 1;
            let formula = format!("=SUM({first_col_letter}{row_num}:{last_point_letter}{row_num})");
            worksheet
                .write_formula_with_format(row, total_col, formula.as_str(), &styles.total_formula_fmt)
                .map_err(|e| e.to_string())?;
        }
    }

    // Bottom Total Row with SUM formulas
    if !project.tasks.is_empty() {
        let total_row = (project.tasks.len() + 1) as u32;
        worksheet
            .write_string_with_format(total_row, 1, total_header, &styles.total_label_fmt)
            .map_err(|e| e.to_string())?;

        let first_row_num = 2;
        let last_row_num = project.tasks.len() + 1;

        for col_idx in 0..=points.len() {
            let col = (2 + col_idx) as u16;
            let col_let = col_to_letter(col as u32);
            let formula = format!("=SUM({col_let}{first_row_num}:{col_let}{last_row_num})");
            worksheet
                .write_formula_with_format(total_row, col, formula.as_str(), &styles.total_formula_fmt)
                .map_err(|e| e.to_string())?;
        }
    }

    worksheet.set_freeze_panes(1, 2).map_err(|e| e.to_string())?;
    worksheet.autofit();

    Ok(())
}

/// Writes Sheet 3: Assignment Matrix.
fn write_assignment_matrix_sheet(
    workbook: &mut Workbook,
    project: &Project,
    styles: &ExcelStyles,
    lang: Lang,
) -> Result<(), String> {
    let sheet_name = match lang {
        Lang::Vi => "Ma trận phân công",
        Lang::En => "Assignment Matrix",
    };
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(sheet_name)
        .map_err(|err| err.to_string())?;

    let matrix = assignment_matrix(project, false);

    worksheet
        .write_string_with_format(0, 0, "ID", &styles.header_fmt)
        .map_err(|e| e.to_string())?;
    worksheet
        .write_string_with_format(
            0,
            1,
            match lang {
                Lang::Vi => "Tên công việc",
                Lang::En => "Task Name",
            },
            &styles.header_fmt,
        )
        .map_err(|e| e.to_string())?;

    for (c_idx, col) in matrix.columns.iter().enumerate() {
        let col_num = (2 + c_idx) as u16;
        let label = format!("{}\n({})", col.label, col.group);
        worksheet
            .write_string_with_format(0, col_num, &label, &styles.header_fmt)
            .map_err(|e| e.to_string())?;
    }

    // Unassigned column
    let unassigned_col = (2 + matrix.columns.len()) as u16;
    worksheet
        .write_string_with_format(
            0,
            unassigned_col,
            match lang {
                Lang::Vi => "Chưa giao (quy tắc)",
                Lang::En => "Unassigned (Rule)",
            },
            &styles.header_fmt,
        )
        .map_err(|e| e.to_string())?;

    // Fill matrix rows
    for (r_idx, row) in matrix.rows.iter().enumerate() {
        let row_num = (r_idx + 1) as u32;

        worksheet
            .write_number_with_format(row_num, 0, row.task_id as f64, &styles.center_fmt)
            .map_err(|e| e.to_string())?;

        let indent = (row.outline_level.saturating_sub(1)) as u8;
        let name_fmt = if row.summary {
            styles.summary_row_fmt.clone().set_indent(indent)
        } else {
            styles.task_name_fmt.clone().set_indent(indent)
        };
        worksheet
            .write_string_with_format(row_num, 1, &row.task_name, &name_fmt)
            .map_err(|e| e.to_string())?;

        for (c_idx, maybe_cell) in row.cells.iter().enumerate() {
            let col_num = (2 + c_idx) as u16;
            if let Some(cell) = maybe_cell {
                worksheet
                    .write_number_with_format(row_num, col_num, cell.units, &styles.percent_fmt)
                    .map_err(|e| e.to_string())?;
            }
        }

        if row.unassigned.is_some() {
            worksheet
                .write_string_with_format(row_num, unassigned_col, "●", &styles.center_fmt)
                .map_err(|e| e.to_string())?;
        }
    }

    worksheet.set_freeze_panes(1, 2).map_err(|e| e.to_string())?;
    worksheet.autofit();

    Ok(())
}

/// Writes Sheet 4: Resource Loading (Monthly Demand vs Capacity).
fn write_resource_loading_sheet(
    workbook: &mut Workbook,
    project: &Project,
    styles: &ExcelStyles,
    lang: Lang,
) -> Result<(), String> {
    let sheet_name = match lang {
        Lang::Vi => "Tải nguồn lực",
        Lang::En => "Resource Loading",
    };
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(sheet_name)
        .map_err(|err| err.to_string())?;

    // Query load map for months across project span
    let project_finish = project
        .tasks
        .iter()
        .map(|t| t.finish)
        .max()
        .unwrap_or(project.start);
    let start_date = DateTime::<Utc>::from_timestamp_millis(project.start)
        .map(|dt| dt.date_naive())
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let finish_date = DateTime::<Utc>::from_timestamp_millis(project_finish)
        .map(|dt| dt.date_naive())
        .unwrap_or(start_date);

    let load = load_map(
        project,
        start_date,
        finish_date,
        Bucket::Month,
        LoadGroup::Person,
        None,
    )
    .map_err(|e| e.to_string())?;

    worksheet
        .write_string_with_format(
            0,
            0,
            match lang {
                Lang::Vi => "Nguồn lực",
                Lang::En => "Resource",
            },
            &styles.header_fmt,
        )
        .map_err(|e| e.to_string())?;

    worksheet
        .write_string_with_format(
            0,
            1,
            match lang {
                Lang::Vi => "Vai trò",
                Lang::En => "Role",
            },
            &styles.header_fmt,
        )
        .map_err(|e| e.to_string())?;

    for (b_idx, bucket_label) in load.buckets.iter().enumerate() {
        let col = (2 + b_idx) as u16;
        worksheet
            .write_string_with_format(0, col, bucket_label, &styles.header_fmt)
            .map_err(|e| e.to_string())?;
    }

    // Resource role map
    let role_map: HashMap<u32, &str> = project
        .resources
        .iter()
        .map(|r| (r.uid, r.group.as_str()))
        .collect();

    for (r_idx, row) in load.rows.iter().enumerate() {
        let row_num = (r_idx + 1) as u32;

        worksheet
            .write_string_with_format(row_num, 0, &row.label, &styles.task_name_fmt)
            .map_err(|e| e.to_string())?;

        let role = role_map
            .get(&row.resource_uid.unwrap_or(0))
            .copied()
            .unwrap_or("");
        worksheet
            .write_string_with_format(row_num, 1, role, &styles.center_fmt)
            .map_err(|e| e.to_string())?;

        for (c_idx, cell) in row.cells.iter().enumerate() {
            let col = (2 + c_idx) as u16;
            let work_h = cell.work as f64 / 60.0;
            let cap_h = cell.capacity as f64 / 60.0;

            let text = if cap_h > 0.0 {
                format!("{:.1}h / {:.1}h", work_h, cap_h)
            } else {
                format!("{:.1}h", work_h)
            };

            let is_over = work_h > cap_h && cap_h > 0.0;
            let fmt = if is_over {
                &styles.overallocated_fmt
            } else {
                &styles.center_fmt
            };

            worksheet
                .write_string_with_format(row_num, col, &text, fmt)
                .map_err(|e| e.to_string())?;
        }
    }

    worksheet.set_freeze_panes(1, 2).map_err(|e| e.to_string())?;
    worksheet.autofit();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sheafly_engine::model::ResourceKind;
    use sheafly_engine::{Command, Document};

    #[test]
    fn test_col_to_letter() {
        assert_eq!(col_to_letter(0), "A");
        assert_eq!(col_to_letter(1), "B");
        assert_eq!(col_to_letter(25), "Z");
        assert_eq!(col_to_letter(26), "AA");
        assert_eq!(col_to_letter(27), "AB");
        assert_eq!(col_to_letter(51), "AZ");
        assert_eq!(col_to_letter(52), "BA");
    }

    #[test]
    fn test_export_empty_project() {
        let doc = Document::new_default();
        let bytes_vi = export_excel_bytes(&doc.project, Lang::Vi).expect("Export VI failed");
        assert!(bytes_vi.len() > 1000);
        // Valid zip archive starts with PK\x03\x04
        assert_eq!(&bytes_vi[0..4], b"PK\x03\x04");

        let bytes_en = export_excel_bytes(&doc.project, Lang::En).expect("Export EN failed");
        assert!(bytes_en.len() > 1000);
        assert_eq!(&bytes_en[0..4], b"PK\x03\x04");
    }

    #[test]
    fn test_export_with_hierarchy_resources_and_baselines() {
        let mut doc = Document::new_in(Lang::En);

        // Add resources
        doc.apply(Command::AddResource {
            name: "Alice Engineer".into(),
            kind: ResourceKind::Person,
        })
        .unwrap();
        let alice_uid = doc.project.resources.last().unwrap().uid;

        doc.apply(Command::AddResource {
            name: "Bob Manager".into(),
            kind: ResourceKind::Person,
        })
        .unwrap();
        let bob_uid = doc.project.resources.last().unwrap().uid;

        // Add summary task and subtask
        doc.apply(Command::AddTask {
            name: "Phase 1: Design".into(),
            after_uid: None,
        })
        .unwrap();
        let phase_uid = doc.project.tasks.last().unwrap().uid;

        doc.apply(Command::AddTask {
            name: "Architecture Blueprint".into(),
            after_uid: Some(phase_uid),
        })
        .unwrap();
        let arch_uid = doc.project.tasks.last().unwrap().uid;

        // Indent Architecture Blueprint under Phase 1
        doc.apply(Command::Indent { uid: arch_uid }).unwrap();

        // Update task properties: duration 10 days (4800 min)
        doc.apply(Command::SetDuration {
            uid: arch_uid,
            minutes: 4800,
        })
        .unwrap();
        doc.apply(Command::SetPercentComplete {
            uid: arch_uid,
            percent: 45.0,
        })
        .unwrap();

        // Assign Alice to Architecture Blueprint
        doc.apply(Command::AssignResource {
            task_uid: arch_uid,
            resource_uid: alice_uid,
            units: 1.0,
        })
        .unwrap();

        // Assign Bob
        doc.apply(Command::AssignResource {
            task_uid: arch_uid,
            resource_uid: bob_uid,
            units: 0.5,
        })
        .unwrap();

        // Set baseline
        doc.apply(Command::SetBaseline { number: 0 }).unwrap();

        // Test export in memory
        let bytes = export_excel_bytes(&doc.project, Lang::Vi).expect("Export with data failed");
        assert!(bytes.len() > 5000);
        assert_eq!(&bytes[0..4], b"PK\x03\x04");

        // Test export to temp file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir
            .join(format!("sheafly_test_{}.xlsx", std::process::id()))
            .to_string_lossy()
            .to_string();

        export_excel_workbook(&doc.project, &file_path, Lang::En).expect("Save to file failed");
        assert!(std::path::Path::new(&file_path).exists());
        let meta = std::fs::metadata(&file_path).unwrap();
        assert!(meta.len() > 5000);
        let _ = std::fs::remove_file(file_path);
    }
}
