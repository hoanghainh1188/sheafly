//! Reports built from a scheduled project (issue #168). Today: the Excel workbook.
//!
//! Nothing here computes a schedule. Every number comes from the engine: task fields,
//! `Project::usage` (work by day), `assignment_matrix` and `load_map`. This crate only adds
//! up days into months, rolls leaves up into their groups and lays the sheets out.
//!
//! Sheets, in order:
//! 1. Tasks & Schedule: the outline with dates, effort, costs, baseline 0 and the critical flag.
//! 2. Time-Phased Work: each task's work by month, with SUM formulas (totals count leaves only).
//! 3. Assignment Matrix: units per task and person, the rule target of unassigned leaves.
//! 4. Resource Loading: each person's work and capacity by month, overallocated months marked.

mod loading_sheet;
mod matrix_sheet;
mod months;
mod phased_sheet;
mod styles;
mod tasks_sheet;
mod text;

use chrono::Datelike;
use rust_xlsxwriter::{ExcelDateTime, Workbook, XlsxError};
use sheafly_engine::model::Project;
use sheafly_engine::timeutil::civil;
use sheafly_engine::Lang;

use months::Months;
use styles::Styles;
use text::Text;

type Result<T> = std::result::Result<T, String>;

/// What every sheet needs: the language's labels and the cell formats.
struct Ctx {
    text: &'static Text,
    styles: Styles,
}

/// The project as an `.xlsx` workbook. An error is a message in `lang`.
pub fn excel_workbook(project: &Project, lang: Lang) -> Result<Vec<u8>> {
    let ctx = Ctx {
        text: text::text(lang),
        styles: Styles::new(),
    };
    let months = Months::of(project).map_err(|err| err.message(lang))?;
    let mut workbook = Workbook::new();
    tasks_sheet::write(&mut workbook, project, &ctx)?;
    phased_sheet::write(&mut workbook, project, &ctx, &months)?;
    matrix_sheet::write(&mut workbook, project, &ctx)?;
    loading_sheet::write(&mut workbook, project, &ctx, &months, lang)?;
    workbook.save_to_buffer().map_err(xlsx)
}

fn xlsx(err: XlsxError) -> String {
    err.to_string()
}

/// A project instant (wall-clock time held as UTC) as an Excel date and time.
fn excel_time(ms: i64) -> Option<ExcelDateTime> {
    let (date, minute) = civil(ms);
    ExcelDateTime::from_ymd(date.year() as u16, date.month() as u8, date.day() as u8)
        .ok()?
        .and_hms(minute / 60, (minute % 60) as u8, 0)
        .ok()
}

/// A 0-based column as Excel letters (0 → A, 27 → AB).
fn column_letters(mut col: u32) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

/// A cached formula result, so readers that do not recalculate (Quick Look, phones,
/// other libraries) show the total instead of 0.
fn result_text(value: f64) -> String {
    let rounded = (value * 1e6).round() / 1e6;
    format!("{rounded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_letters_follow_excel() {
        let cases = [(0, "A"), (1, "B"), (25, "Z"), (26, "AA"), (27, "AB"), (51, "AZ"), (52, "BA"), (701, "ZZ"), (702, "AAA")];
        for (col, letters) in cases {
            assert_eq!(column_letters(col), letters);
        }
    }

    #[test]
    fn excel_time_keeps_the_wall_clock() {
        let ms = sheafly_engine::timeutil::compose(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), 8 * 60 + 30);
        let expected = ExcelDateTime::from_ymd(2026, 1, 5).unwrap().and_hms(8, 30, 0).unwrap();
        assert_eq!(excel_time(ms).unwrap().to_excel(), expected.to_excel());
    }
}
