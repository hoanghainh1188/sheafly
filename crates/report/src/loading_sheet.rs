//! Sheet 4: each person's work and capacity by month, from the engine's `load_map`.
//!
//! Two numeric rows per person (work, capacity), so the sheet sums and pivots. A work
//! cell is marked when the engine found the person overallocated on any day of that
//! month (`Project::overallocations`), or when the month has work and no capacity.

use std::collections::HashSet;

use rust_xlsxwriter::{Formula, Workbook, Worksheet};
use sheafly_engine::load_map::{load_map, Bucket, LoadGroup, LoadRow};
use sheafly_engine::model::Project;
use sheafly_engine::Lang;

use crate::months::Months;
use crate::{column_letters, result_text, xlsx, Ctx, Result};

/// Columns before the months: resource, role, measure.
const FIRST_MONTH: u16 = 3;

pub(crate) fn write(workbook: &mut Workbook, project: &Project, ctx: &Ctx, months: &Months, lang: Lang) -> Result<()> {
    let s = &ctx.styles;
    let t = ctx.text;
    let load = load_map(project, months.first, months.last, Bucket::Month, LoadGroup::Person, None)
        .map_err(|err| err.message(lang))?;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t.loading_sheet).map_err(xlsx)?;
    for (col, header) in [t.resource, t.role, t.measure].into_iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, header, &s.header).map_err(xlsx)?;
    }
    // The map's buckets are the same months: both start on the 1st of the first month.
    let total_col = FIRST_MONTH + load.buckets.len() as u16;
    for (index, bucket) in load.buckets.iter().enumerate() {
        let label = bucket.get(..7).unwrap_or(bucket);
        sheet.write_string_with_format(0, FIRST_MONTH + index as u16, label, &s.header).map_err(xlsx)?;
    }
    sheet.write_string_with_format(0, total_col, t.total_hours, &s.header).map_err(xlsx)?;

    let over: HashSet<(u32, usize)> = project
        .overallocations
        .iter()
        .filter_map(|o| Some((o.resource_uid, months.of_day(&o.date)?)))
        .collect();
    for (index, row) in load.rows.iter().enumerate() {
        let r = 1 + 2 * index as u32;
        write_person(sheet, project, ctx, row, r, total_col, &over)?;
    }
    sheet.set_freeze_panes(1, FIRST_MONTH).map_err(xlsx)?;
    let last = 2 * load.rows.len() as u32;
    sheet.autofilter(0, 0, last, total_col).map_err(xlsx)?;
    sheet.autofit();
    Ok(())
}

fn write_person(
    sheet: &mut Worksheet,
    project: &Project,
    ctx: &Ctx,
    row: &LoadRow,
    r: u32,
    total_col: u16,
    over: &HashSet<(u32, usize)>,
) -> Result<()> {
    let s = &ctx.styles;
    let role = row
        .resource_uid
        .and_then(|uid| project.resources.iter().find(|res| res.uid == uid))
        .map(|res| res.group.as_str())
        .unwrap_or("");
    let first = column_letters(FIRST_MONTH as u32);
    let last = column_letters(total_col as u32 - 1);
    for (offset, measure) in [ctx.text.work_hours, ctx.text.capacity_hours].into_iter().enumerate() {
        let line = r + offset as u32;
        sheet.write_string_with_format(line, 0, &row.label, &s.text.task).map_err(xlsx)?;
        sheet.write_string_with_format(line, 1, role, &s.center.task).map_err(xlsx)?;
        sheet.write_string_with_format(line, 2, measure, &s.text.task).map_err(xlsx)?;
        let mut sum = 0.0;
        for (month, cell) in row.cells.iter().enumerate() {
            let minutes = if offset == 0 { cell.work } else { cell.capacity };
            let hours = minutes as f64 / 60.0;
            sum += hours;
            let flagged = row.resource_uid.is_some_and(|uid| over.contains(&(uid, month)));
            let marked = offset == 0 && (flagged || (cell.work > 0 && cell.capacity == 0));
            let format = if marked { &s.over } else { &s.hours.task };
            sheet.write_number_with_format(line, FIRST_MONTH + month as u16, hours, format).map_err(xlsx)?;
        }
        if !row.cells.is_empty() {
            let n = line + 1;
            let formula = Formula::new(format!("=SUM({first}{n}:{last}{n})")).set_result(result_text(sum));
            sheet.write_formula_with_format(line, total_col, formula, &s.total_hours).map_err(xlsx)?;
        }
    }
    Ok(())
}
