//! Sheet 2: each task's work by month.
//!
//! A leaf's hours are its row of `Project::usage` (the engine spreads work over the task's
//! working time: calendar, splits, holidays), added up by month. A group shows the sum of
//! the leaves under it. The bottom totals count leaves only (`SUMIF` on the Group column),
//! so a group never counts its work twice.

use std::collections::HashMap;

use rust_xlsxwriter::{Formula, Workbook};
use sheafly_engine::model::Project;

use crate::months::Months;
use crate::{column_letters, result_text, xlsx, Ctx, Result};

/// Columns before the months: ID, name, group.
const FIRST_MONTH: u16 = 3;
const GROUP_COL: u32 = 2;

pub(crate) fn write(workbook: &mut Workbook, project: &Project, ctx: &Ctx, months: &Months) -> Result<()> {
    let s = &ctx.styles;
    let t = ctx.text;
    let sheet = workbook.add_worksheet();
    sheet.set_name(t.phased_sheet).map_err(xlsx)?;
    let total_col = FIRST_MONTH + months.len() as u16;
    for (col, header) in ["ID", t.task_name, t.summary].into_iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, header, &s.header).map_err(xlsx)?;
    }
    for (index, key) in months.keys.iter().enumerate() {
        sheet.write_string_with_format(0, FIRST_MONTH + index as u16, key, &s.header).map_err(xlsx)?;
    }
    sheet.write_string_with_format(0, total_col, t.total_hours, &s.header).map_err(xlsx)?;

    let hours = hours_by_month(project, months);
    let first_letters = column_letters(FIRST_MONTH as u32);
    let last_letters = column_letters(total_col as u32 - 1);
    for (index, task) in project.tasks.iter().enumerate() {
        let row = (index + 1) as u32;
        let group = task.summary;
        sheet.write_number_with_format(row, 0, task.id as f64, s.center.of(group)).map_err(xlsx)?;
        let indent = task.outline_level.saturating_sub(1).min(15) as u8;
        let name = s.text.of(group).clone().set_indent(indent);
        sheet.write_string_with_format(row, 1, &task.name, &name).map_err(xlsx)?;
        let flag = if group { t.yes } else { t.no };
        sheet.write_string_with_format(row, GROUP_COL as u16, flag, s.center.of(group)).map_err(xlsx)?;
        for (month, value) in hours[index].iter().enumerate() {
            sheet.write_number_with_format(row, FIRST_MONTH + month as u16, *value, s.hours.of(group)).map_err(xlsx)?;
        }
        if months.len() > 0 {
            let n = row + 1;
            let formula = Formula::new(format!("=SUM({first_letters}{n}:{last_letters}{n})"))
                .set_result(result_text(hours[index].iter().sum()));
            sheet.write_formula_with_format(row, total_col, formula, &s.total_hours).map_err(xlsx)?;
        }
    }
    if !project.tasks.is_empty() {
        write_totals(sheet, project, ctx, &hours, total_col)?;
    }
    sheet.set_freeze_panes(1, FIRST_MONTH).map_err(xlsx)?;
    sheet.autofit();
    Ok(())
}

fn write_totals(
    sheet: &mut rust_xlsxwriter::Worksheet,
    project: &Project,
    ctx: &Ctx,
    hours: &[Vec<f64>],
    total_col: u16,
) -> Result<()> {
    let s = &ctx.styles;
    let row = project.tasks.len() as u32 + 1;
    let last = project.tasks.len() + 1;
    let group_letters = column_letters(GROUP_COL);
    sheet.write_string_with_format(row, 1, ctx.text.total_hours, &s.total_label).map_err(xlsx)?;
    let leaves: Vec<usize> = (0..project.tasks.len()).filter(|&i| !project.tasks[i].summary).collect();
    let months = total_col - FIRST_MONTH;
    for offset in 0..=months {
        let col = FIRST_MONTH + offset;
        let letters = column_letters(col as u32);
        let sum: f64 = if offset < months {
            leaves.iter().map(|&i| hours[i][offset as usize]).sum()
        } else {
            leaves.iter().map(|&i| hours[i].iter().sum::<f64>()).sum()
        };
        let formula = Formula::new(format!(
            "=SUMIF(${group_letters}$2:${group_letters}${last},\"{}\",{letters}2:{letters}{last})",
            ctx.text.no
        ))
        .set_result(result_text(sum));
        sheet.write_formula_with_format(row, col, formula, &s.total_hours).map_err(xlsx)?;
    }
    Ok(())
}

/// Hours by month for every task, in `project.tasks` order. Groups hold the sum of the
/// leaves below them.
fn hours_by_month(project: &Project, months: &Months) -> Vec<Vec<f64>> {
    let position: HashMap<u32, usize> = project.tasks.iter().enumerate().map(|(i, t)| (t.uid, i)).collect();
    let mut hours = vec![vec![0.0; months.len()]; project.tasks.len()];
    for row in project.usage.iter().filter(|row| row.assignment_uid.is_none()) {
        let Some(&index) = position.get(&row.task_uid) else { continue };
        for day in &row.days {
            if let Some(month) = months.of_day(&day.date) {
                hours[index][month] += day.hours;
            }
        }
    }
    let mut groups: Vec<usize> = Vec::new();
    for (index, task) in project.tasks.iter().enumerate() {
        while groups.last().is_some_and(|&g| project.tasks[g].outline_level >= task.outline_level) {
            groups.pop();
        }
        if task.summary {
            groups.push(index);
        } else {
            let own = hours[index].clone();
            for &group in &groups {
                for (month, value) in own.iter().enumerate() {
                    hours[group][month] += value;
                }
            }
        }
    }
    hours
}
