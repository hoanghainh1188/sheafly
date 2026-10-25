//! Resource assignment matrix: which person or role is assigned to which task.
//! A read, never logged.
//!
//! Rows: tasks in outline order (summaries roll up).
//! Columns: people grouped under their role, or roles (when `group` is true).
//! A column "Chưa giao" for leaves with no one, showing the target of the first matching
//! assignment rule (skill and minimum level, or the person).
//! Cells: units and work of the assignment, as computed by the engine; a role column shows
//! the sum of its people. A summary's cell adds an assignment of its own (allowed, e.g.
//! after indenting a task under an assigned one) to its children's. Totals per row and
//! column count each assignment once.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Assignment, Project};
use crate::skills::{rule_for, RuleTarget};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixCell {
    /// Assignment UID when the row's task itself is assigned to this resource (a leaf, or
    /// a summary holding an assignment of its own; its cell also sums its children).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_uid: Option<u32>,
    /// Units assigned, or sum of units for a role or summary roll-up.
    pub units: f64,
    /// Work in working minutes.
    pub work: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixColumn {
    /// In person mode: the resource UID as string (`"{uid}"`).
    /// In role mode: the role name.
    pub id: String,
    pub label: String,
    /// The role / group name this column belongs to.
    pub group: String,
    /// Resource UID if this is a person column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_uid: Option<u32>,
    /// Total work in working minutes for this column: every task's own assignments.
    pub total_work: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixRole {
    pub name: String,
    /// Resource UIDs belonging to this role, in project order.
    pub resource_uids: Vec<u32>,
    /// Total work in working minutes for this role across the project.
    pub total_work: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnassignedTarget {
    pub rule_index: usize,
    pub target: RuleTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixRow {
    pub task_uid: u32,
    pub task_id: u32,
    pub task_name: String,
    pub outline_level: u32,
    pub summary: bool,
    /// Cells corresponding 1:1 to `columns`.
    pub cells: Vec<Option<MatrixCell>>,
    /// Role cells corresponding 1:1 to `roles` (sums of each role's members).
    pub role_cells: Vec<Option<MatrixCell>>,
    /// For an unassigned leaf task with no resources assigned:
    /// target of the first matching assignment rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unassigned: Option<UnassignedTarget>,
    /// Row total work in working minutes across all columns.
    pub total_work: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssignmentMatrix {
    pub columns: Vec<MatrixColumn>,
    pub roles: Vec<MatrixRole>,
    pub rows: Vec<MatrixRow>,
    pub total_work: i64,
}

/// Compute the assignment matrix.
/// When `group` is true, columns are distinct roles (RACI-style).
/// When `group` is false, columns are people grouped under their role.
#[allow(clippy::needless_range_loop)]
pub fn assignment_matrix(project: &Project, group: bool) -> AssignmentMatrix {
    // 1. Collect distinct non-empty roles sorted alphabetically, then empty role at the end.
    let mut role_names: Vec<String> = project
        .resources
        .iter()
        .map(|r| r.group.clone())
        .filter(|g| !g.is_empty())
        .collect();
    role_names.sort();
    role_names.dedup();
    if project.resources.iter().any(|r| r.group.is_empty()) {
        role_names.push(String::new());
    }

    // 2. Build roles list.
    let mut roles: Vec<MatrixRole> = role_names
        .iter()
        .map(|name| {
            let uids: Vec<u32> = project
                .resources
                .iter()
                .filter(|r| r.group == *name)
                .map(|r| r.uid)
                .collect();
            MatrixRole {
                name: name.clone(),
                resource_uids: uids,
                total_work: 0,
            }
        })
        .collect();

    // 3. Build columns.
    let mut columns: Vec<MatrixColumn> = if group {
        roles
            .iter()
            .map(|r| MatrixColumn {
                id: r.name.clone(),
                label: r.name.clone(),
                group: r.name.clone(),
                resource_uid: None,
                total_work: 0,
            })
            .collect()
    } else {
        let mut cols = Vec::new();
        for role in &roles {
            for &uid in &role.resource_uids {
                if let Some(resource) = project.resources.iter().find(|r| r.uid == uid) {
                    cols.push(MatrixColumn {
                        id: uid.to_string(),
                        label: resource.name.clone(),
                        group: resource.group.clone(),
                        resource_uid: Some(uid),
                        total_work: 0,
                    });
                }
            }
        }
        cols
    };

    // 4. Index assignments by (task_uid, resource_uid).
    let mut assignment_map: HashMap<(u32, u32), &Assignment> =
        HashMap::with_capacity(project.assignments.len());
    let mut task_assignment_count: HashMap<u32, usize> =
        HashMap::with_capacity(project.tasks.len());
    for a in &project.assignments {
        assignment_map.insert((a.task_uid, a.resource_uid), a);
        *task_assignment_count.entry(a.task_uid).or_insert(0) += 1;
    }

    // 5. Pre-allocate cell grids: task_index -> column_index -> Option<MatrixCell>.
    let task_count = project.tasks.len();
    let col_count = columns.len();
    let role_count = roles.len();
    let mut grid: Vec<Vec<Option<MatrixCell>>> = vec![vec![None; col_count]; task_count];
    let mut role_grid: Vec<Vec<Option<MatrixCell>>> = vec![vec![None; role_count]; task_count];

    // Each task's own assignments, summaries included: a summary can hold one of its own
    // (an assigned task that later got children, or an MSPDI file).
    for (i, task) in project.tasks.iter().enumerate() {
        for (r, role) in roles.iter().enumerate() {
            let mut sum_units = 0.0;
            let mut sum_work = 0i64;
            let mut has_any = false;
            for &res_uid in &role.resource_uids {
                if let Some(a) = assignment_map.get(&(task.uid, res_uid)) {
                    sum_units += a.units;
                    sum_work += a.work;
                    has_any = true;
                }
            }
            if has_any {
                role_grid[i][r] = Some(MatrixCell {
                    assignment_uid: None,
                    units: sum_units,
                    work: sum_work,
                });
            }
        }
        if group {
            grid[i] = role_grid[i].clone();
        } else {
            for (c, col) in columns.iter().enumerate() {
                let Some(res_uid) = col.resource_uid else {
                    continue;
                };
                if let Some(a) = assignment_map.get(&(task.uid, res_uid)) {
                    grid[i][c] = Some(MatrixCell {
                        assignment_uid: Some(a.uid),
                        units: a.units,
                        work: a.work,
                    });
                }
            }
        }
    }

    // 6. Column totals and grand total: every task's own assignments, counted once.
    for (c, col) in columns.iter_mut().enumerate() {
        col.total_work = grid
            .iter()
            .filter_map(|cells| cells[c].as_ref())
            .map(|cell| cell.work)
            .sum();
    }
    for (r, role) in roles.iter_mut().enumerate() {
        role.total_work = role_grid
            .iter()
            .filter_map(|cells| cells[r].as_ref())
            .map(|cell| cell.work)
            .sum();
    }
    let total_work: i64 = role_grid
        .iter()
        .flatten()
        .filter_map(|cell| cell.as_ref())
        .map(|cell| cell.work)
        .sum();

    // 7. Roll up summaries bottom-up: a summary's own assignment plus its direct children,
    // which already hold theirs. A summary's own assignment keeps its uid on the cell.
    for i in (0..task_count).rev() {
        if !project.tasks[i].summary {
            continue;
        }
        let level = project.tasks[i].outline_level;
        let children: Vec<usize> = (i + 1..task_count)
            .take_while(|&j| project.tasks[j].outline_level > level)
            .filter(|&j| project.tasks[j].outline_level == level + 1)
            .collect();
        for r in 0..role_count {
            role_grid[i][r] = roll_up(
                role_grid[i][r].take(),
                children.iter().map(|&j| &role_grid[j][r]),
            );
        }
        if group {
            grid[i] = role_grid[i].clone();
        } else {
            for c in 0..col_count {
                grid[i][c] = roll_up(grid[i][c].take(), children.iter().map(|&j| &grid[j][c]));
            }
        }
    }

    // 8. Build rows, row totals, and unassigned targets.
    let mut rows = Vec::with_capacity(task_count);
    for (i, task) in project.tasks.iter().enumerate() {
        let cells = std::mem::take(&mut grid[i]);
        let role_cells = std::mem::take(&mut role_grid[i]);
        let total_work: i64 = cells
            .iter()
            .filter_map(|c| c.as_ref())
            .map(|c| c.work)
            .sum();

        let unassigned =
            if !task.summary && task_assignment_count.get(&task.uid).copied().unwrap_or(0) == 0 {
                rule_for(project, task).map(|idx| UnassignedTarget {
                    rule_index: idx,
                    target: project.assignment_rules[idx].target,
                })
            } else {
                None
            };

        rows.push(MatrixRow {
            task_uid: task.uid,
            task_id: task.id,
            task_name: task.name.clone(),
            outline_level: task.outline_level,
            summary: task.summary,
            cells,
            role_cells,
            unassigned,
            total_work,
        });
    }

    AssignmentMatrix {
        columns,
        roles,
        rows,
        total_work,
    }
}

/// A summary's cell: its own assignment (keeping its uid) plus its children's cells.
fn roll_up<'a>(
    own: Option<MatrixCell>,
    children: impl Iterator<Item = &'a Option<MatrixCell>>,
) -> Option<MatrixCell> {
    children.flatten().fold(own, |acc, cell| {
        let mut sum = acc.unwrap_or(MatrixCell {
            assignment_uid: None,
            units: 0.0,
            work: 0,
        });
        sum.units += cell.units;
        sum.work += cell.work;
        Some(sum)
    })
}
