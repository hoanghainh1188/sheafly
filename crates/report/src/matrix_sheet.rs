//! Sheet 3: units per task and person, from the engine's `assignment_matrix` (person mode).
//! An unassigned leaf names the target of the first rule that matches it.

use rust_xlsxwriter::Workbook;
use sheafly_engine::assignment_matrix::assignment_matrix;
use sheafly_engine::model::Project;
use sheafly_engine::skills::RuleTarget;

use crate::{xlsx, Ctx, Result};

pub(crate) fn write(workbook: &mut Workbook, project: &Project, ctx: &Ctx) -> Result<()> {
    let s = &ctx.styles;
    let sheet = workbook.add_worksheet();
    sheet.set_name(ctx.text.matrix_sheet).map_err(xlsx)?;
    let matrix = assignment_matrix(project, false);
    sheet.write_string_with_format(0, 0, "ID", &s.header).map_err(xlsx)?;
    sheet.write_string_with_format(0, 1, ctx.text.task_name, &s.header).map_err(xlsx)?;
    for (index, column) in matrix.columns.iter().enumerate() {
        let label = if column.group.trim().is_empty() {
            column.label.clone()
        } else {
            format!("{}\n({})", column.label, column.group)
        };
        sheet.write_string_with_format(0, 2 + index as u16, &label, &s.header).map_err(xlsx)?;
    }
    let rule_col = 2 + matrix.columns.len() as u16;
    sheet.write_string_with_format(0, rule_col, ctx.text.unassigned_rule, &s.header).map_err(xlsx)?;

    for (index, row) in matrix.rows.iter().enumerate() {
        let r = (index + 1) as u32;
        let group = row.summary;
        sheet.write_number_with_format(r, 0, row.task_id as f64, s.center.of(group)).map_err(xlsx)?;
        let indent = row.outline_level.saturating_sub(1).min(15) as u8;
        let name = s.text.of(group).clone().set_indent(indent);
        sheet.write_string_with_format(r, 1, &row.task_name, &name).map_err(xlsx)?;
        for (col, cell) in row.cells.iter().enumerate() {
            if let Some(cell) = cell {
                sheet.write_number_with_format(r, 2 + col as u16, cell.units, s.percent.of(group)).map_err(xlsx)?;
            }
        }
        if let Some(unassigned) = &row.unassigned {
            let target = target_text(project, &unassigned.target, ctx);
            sheet.write_string_with_format(r, rule_col, &target, s.text.of(group)).map_err(xlsx)?;
        }
    }
    sheet.set_freeze_panes(1, 2).map_err(xlsx)?;
    sheet.autofit();
    Ok(())
}

/// A rule's target as the app shows it: a person's name, or a skill with its minimum level.
fn target_text(project: &Project, target: &RuleTarget, ctx: &Ctx) -> String {
    match target {
        RuleTarget::Resource { uid } => project
            .resources
            .iter()
            .find(|r| r.uid == *uid)
            .map(|r| r.name.clone())
            .unwrap_or_default(),
        RuleTarget::Skill { id, min_level } => {
            let skill = project.skills.iter().find(|k| k.id == *id).map(|k| k.name.as_str()).unwrap_or_default();
            let level = min_level.and_then(|level| project.skill_levels.iter().find(|l| l.id == level));
            match level {
                Some(level) => format!("{skill} ({} {})", ctx.text.at_least, level.name),
                None => skill.to_string(),
            }
        }
    }
}
