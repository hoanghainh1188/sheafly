//! Sheet 1: the task outline with dates, effort, costs, baseline 0 and the critical flag.

use std::collections::HashMap;

use rust_xlsxwriter::{Workbook, Worksheet};
use sheafly_engine::model::{Project, Task};

use crate::{excel_time, xlsx, Ctx, Result};

/// Minutes in a day of duration, as the app shows durations (`MINUTES_PER_DAY` in `format.ts`).
const MINUTES_PER_DAY: f64 = 480.0;

pub(crate) fn write(workbook: &mut Workbook, project: &Project, ctx: &Ctx) -> Result<()> {
    let sheet = workbook.add_worksheet();
    sheet.set_name(ctx.text.tasks_sheet).map_err(xlsx)?;
    for (col, header) in ctx.text.task_columns.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, *header, &ctx.styles.header).map_err(xlsx)?;
    }
    let people = people_by_task(project);
    for (index, task) in project.tasks.iter().enumerate() {
        let names = people.get(&task.uid).map(|names| names.join(", ")).unwrap_or_default();
        write_row(sheet, (index + 1) as u32, task, &names, ctx)?;
    }
    let last_col = (ctx.text.task_columns.len() - 1) as u16;
    sheet.set_freeze_panes(1, 2).map_err(xlsx)?;
    sheet.autofilter(0, 0, project.tasks.len() as u32, last_col).map_err(xlsx)?;
    sheet.autofit();
    Ok(())
}

fn people_by_task(project: &Project) -> HashMap<u32, Vec<&str>> {
    let names: HashMap<u32, &str> = project.resources.iter().map(|r| (r.uid, r.name.as_str())).collect();
    let mut people: HashMap<u32, Vec<&str>> = HashMap::new();
    for assignment in &project.assignments {
        if let Some(&name) = names.get(&assignment.resource_uid) {
            people.entry(assignment.task_uid).or_default().push(name);
        }
    }
    people
}

fn write_row(sheet: &mut Worksheet, row: u32, task: &Task, people: &str, ctx: &Ctx) -> Result<()> {
    let s = &ctx.styles;
    let group = task.summary;
    let indent = task.outline_level.saturating_sub(1).min(15) as u8;
    sheet.write_number_with_format(row, 0, task.id as f64, s.center.of(group)).map_err(xlsx)?;
    let name = s.text.of(group).clone().set_indent(indent);
    sheet.write_string_with_format(row, 1, &task.name, &name).map_err(xlsx)?;
    sheet.write_number_with_format(row, 2, task.outline_level as f64, s.center.of(group)).map_err(xlsx)?;
    let days = task.duration as f64 / MINUTES_PER_DAY;
    sheet.write_number_with_format(row, 3, days, s.hours.of(group)).map_err(xlsx)?;
    sheet.write_number_with_format(row, 4, task.work as f64 / 60.0, s.hours.of(group)).map_err(xlsx)?;
    write_time(sheet, row, 5, task.start, s.date.of(group))?;
    write_time(sheet, row, 6, task.finish, s.date.of(group))?;
    sheet.write_string_with_format(row, 7, &task.predecessors, s.center.of(group)).map_err(xlsx)?;
    sheet.write_string_with_format(row, 8, people, s.text.of(group)).map_err(xlsx)?;
    sheet.write_number_with_format(row, 9, task.percent_complete / 100.0, s.percent.of(group)).map_err(xlsx)?;
    sheet.write_number_with_format(row, 10, task.cost, s.money.of(group)).map_err(xlsx)?;
    sheet.write_number_with_format(row, 11, task.actual_cost, s.money.of(group)).map_err(xlsx)?;
    if let Some(baseline) = task.baselines.iter().find(|b| b.number == 0) {
        write_time(sheet, row, 12, baseline.start, s.date.of(group))?;
        write_time(sheet, row, 13, baseline.finish, s.date.of(group))?;
        sheet.write_number_with_format(row, 14, baseline.cost, s.money.of(group)).map_err(xlsx)?;
    }
    let critical = if task.critical { ctx.text.yes } else { ctx.text.no };
    sheet.write_string_with_format(row, 15, critical, s.center.of(group)).map_err(xlsx)?;
    Ok(())
}

fn write_time(sheet: &mut Worksheet, row: u32, col: u16, ms: i64, format: &rust_xlsxwriter::Format) -> Result<()> {
    if let Some(time) = excel_time(ms) {
        sheet.write_datetime_with_format(row, col, &time, format).map_err(xlsx)?;
    }
    Ok(())
}
