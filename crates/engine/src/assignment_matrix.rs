//! Resource assignment matrix: which person or role is assigned to which task.
//! A read, never logged.
//!
//! Rows: tasks in outline order (summaries roll up).
//! Columns: people grouped under their role, or roles (when `group` is true).
//! A column "Chưa giao" for leaves with no one, showing the target of the first matching
//! assignment rule (skill and minimum level, or the person).
//! Cells: units and work of the assignment, as computed by the engine; a role column shows
//! the sum of its people. Totals per row and column.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{Assignment, Project};
use crate::skills::{rule_for, RuleTarget};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixCell {
    /// Assignment UID if this is a leaf task directly assigned to this resource.
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
    /// Total work in working minutes for this column across all leaf tasks.
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

    // Populate leaf task cells.
    for (i, task) in project.tasks.iter().enumerate() {
        if task.summary {
            continue;
        }
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
                let res_uid = col.resource_uid.unwrap();
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

    // 6. Roll up summaries bottom-up.
    for i in (0..task_count).rev() {
        if !project.tasks[i].summary {
            continue;
        }
        let level = project.tasks[i].outline_level;

        // Roll up role_grid
        for r in 0..role_count {
            let mut sum_units = 0.0;
            let mut sum_work = 0i64;
            let mut has_any = false;
            for j in i + 1..task_count {
                if project.tasks[j].outline_level <= level {
                    break;
                }
                if project.tasks[j].outline_level == level + 1 {
                    if let Some(cell) = &role_grid[j][r] {
                        sum_units += cell.units;
                        sum_work += cell.work;
                        has_any = true;
                    }
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
            for c in 0..col_count {
                let mut sum_units = 0.0;
                let mut sum_work = 0i64;
                let mut has_any = false;
                for j in i + 1..task_count {
                    if project.tasks[j].outline_level <= level {
                        break;
                    }
                    if project.tasks[j].outline_level == level + 1 {
                        if let Some(cell) = &grid[j][c] {
                            sum_units += cell.units;
                            sum_work += cell.work;
                            has_any = true;
                        }
                    }
                }
                if has_any {
                    grid[i][c] = Some(MatrixCell {
                        assignment_uid: None,
                        units: sum_units,
                        work: sum_work,
                    });
                }
            }
        }
    }

    // 7. Build rows, row totals, and unassigned targets.
    let mut rows = Vec::with_capacity(task_count);
    for (i, task) in project.tasks.iter().enumerate() {
        let cells = std::mem::take(&mut grid[i]);
        let role_cells = std::mem::take(&mut role_grid[i]);
        let total_work: i64 = cells.iter().filter_map(|c| c.as_ref()).map(|c| c.work).sum();

        let unassigned = if !task.summary
            && task_assignment_count.get(&task.uid).copied().unwrap_or(0) == 0
        {
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

    // 8. Column totals and grand total across all leaf tasks.
    let leaf_indices: Vec<usize> = project
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.summary)
        .map(|(i, _)| i)
        .collect();

    for (c, col) in columns.iter_mut().enumerate() {
        col.total_work = leaf_indices
            .iter()
            .filter_map(|&i| rows[i].cells[c].as_ref())
            .map(|cell| cell.work)
            .sum();
    }

    for role in &mut roles {
        role.total_work = leaf_indices
            .iter()
            .map(|&i| {
                let task_uid = project.tasks[i].uid;
                role.resource_uids
                    .iter()
                    .filter_map(|&res_uid| assignment_map.get(&(task_uid, res_uid)))
                    .map(|a| a.work)
                    .sum::<i64>()
            })
            .sum();
    }

    let total_work: i64 = leaf_indices.iter().map(|&i| rows[i].total_work).sum();

    AssignmentMatrix {
        columns,
        roles,
        rows,
        total_work,
    }
}
