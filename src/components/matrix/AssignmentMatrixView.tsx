import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { assignmentMatrix } from "../../api";
import { formatUnits, parseUnits } from "../../assignmentUnits";
import { formatHours } from "../../format";
import { useMessages } from "../../i18n";
import type { AssignmentMatrix, MatrixCell, Project, RuleTarget, SkillDef, SkillLevel } from "../../types";
import { matrixMessages } from "./AssignmentMatrixView.i18n";

interface Props {
  project: Project;
  group: boolean;
  visibleTaskUids?: Set<number> | null;
  onAssign: (taskUid: number, resourceUid: number, units: number) => void;
  onUpdate: (assignmentUid: number, units: number) => void;
  onRemove: (assignmentUid: number) => void;
}

function targetLabel(
  target: RuleTarget,
  resources: Project["resources"],
  skills: SkillDef[],
  skillLevels: SkillLevel[] = [],
): string {
  if (target.type === "resource") {
    const res = resources.find((r) => r.uid === target.uid);
    return res ? res.name : `NL ${target.uid}`;
  }
  const skill = skills.find((s) => s.id === target.id);
  const skillName = skill ? skill.name : `KN ${target.id}`;
  if (target.min_level != null) {
    const level = skillLevels.find((l) => l.id === target.min_level);
    return level ? `${skillName}:${level.name}` : `${skillName}:${target.min_level}`;
  }
  return skillName;
}

function hoursText(work: number): string {
  const h = formatHours(work / 60);
  return h ? `${h}h` : "";
}

export function AssignmentMatrixView({
  project,
  group,
  visibleTaskUids,
  onAssign,
  onUpdate,
  onRemove,
}: Props) {
  const m = useMessages(matrixMessages);
  const [data, setData] = useState<AssignmentMatrix | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [editing, setEditing] = useState<{ taskUid: number; resourceUid: number } | null>(null);
  const [editText, setEditText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Fetch matrix from engine when project or group mode changes
  useEffect(() => {
    let active = true;
    void assignmentMatrix(group).then((res) => {
      if (active) setData(res);
    });
    return () => {
      active = false;
    };
  }, [project, group]);

  useEffect(() => {
    if (editing) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing]);

  const toggleCollapse = (roleName: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(roleName)) next.delete(roleName);
      else next.add(roleName);
      return next;
    });
  };

  const rows = useMemo(() => {
    if (!data) return [];
    if (!visibleTaskUids) return data.rows;
    return data.rows.filter((r) => visibleTaskUids.has(r.task_uid));
  }, [data, visibleTaskUids]);

  if (!data) {
    return <div className="matrix-view loading"><p className="meta">{m.empty}</p></div>;
  }

  // Count unassigned leaf tasks
  const unassignedCount = rows.filter((r) => !r.summary && r.cells.every((c) => !c)).length;

  const commitEdit = (taskUid: number, resourceUid: number, currentCell: MatrixCell | null) => {
    const trimmed = editText.trim();
    if (trimmed === "" || trimmed === "0") {
      if (currentCell?.assignment_uid) {
        onRemove(currentCell.assignment_uid);
      }
    } else {
      const units = parseUnits(trimmed);
      if (units != null && units > 0) {
        if (currentCell?.assignment_uid) {
          if (units !== currentCell.units) {
            onUpdate(currentCell.assignment_uid, units);
          }
        } else {
          onAssign(taskUid, resourceUid, units);
        }
      }
    }
    setEditing(null);
  };

  const onEditKeyDown = (
    event: KeyboardEvent<HTMLInputElement>,
    taskUid: number,
    resourceUid: number,
    currentCell: MatrixCell | null,
  ) => {
    if (event.key === "Enter") {
      event.preventDefault();
      commitEdit(taskUid, resourceUid, currentCell);
    } else if (event.key === "Escape") {
      event.preventDefault();
      setEditing(null);
    }
  };

  return (
    <div className="matrix-view" role="region" aria-label={m.task}>
      <table className="matrix-table" role="grid">
        <thead>
          {/* Header Row 1 */}
          <tr>
            <th rowSpan={group ? 1 : 2} className="matrix-col-id">{m.id}</th>
            <th rowSpan={group ? 1 : 2} className="matrix-col-name">{m.task}</th>
            <th rowSpan={group ? 1 : 2} className="matrix-col-unassigned" title={m.unassigned}>
              {m.unassigned}
            </th>
            {group ? (
              // In Role mode, each column is a role
              data.columns.map((col) => (
                <th key={col.id} className="matrix-col-role" title={m.roleOf(col.label || m.noRole)}>
                  {col.label || m.noRole}
                </th>
              ))
            ) : (
              // In Person mode, super-headers for roles
              data.roles.map((role) => {
                const isCollapsed = collapsed.has(role.name);
                const colSpan = isCollapsed ? 1 : role.resource_uids.length;
                if (colSpan === 0) return null;
                return (
                  <th
                    key={role.name}
                    colSpan={colSpan}
                    className="matrix-role-group"
                  >
                    <div className="matrix-role-header">
                      <span className="matrix-role-title">
                        {role.name ? m.roleOf(role.name) : m.noRole} ({role.resource_uids.length})
                      </span>
                      <button
                        type="button"
                        className="matrix-collapse-btn"
                        onClick={() => toggleCollapse(role.name)}
                        aria-label={isCollapsed ? m.expand : m.collapse}
                      >
                        {isCollapsed ? `+ ${m.expand}` : `− ${m.collapse}`}
                      </button>
                    </div>
                  </th>
                );
              })
            )}
            <th rowSpan={group ? 1 : 2} className="matrix-col-total">{m.total}</th>
          </tr>

          {/* Header Row 2 (Person mode sub-headers) */}
          {!group && (
            <tr>
              {data.roles.flatMap((role) => {
                const isCollapsed = collapsed.has(role.name);
                if (isCollapsed) {
                  return [
                    <th key={`${role.name}-collapsed`} className="matrix-col-person collapsed" title={role.name || m.noRole}>
                      {role.name || m.noRole}
                    </th>,
                  ];
                }
                return role.resource_uids.map((uid) => {
                  const res = project.resources.find((r) => r.uid === uid);
                  const name = res ? res.name : `NL ${uid}`;
                  return (
                    <th key={uid} className="matrix-col-person" title={name}>
                      {name}
                    </th>
                  );
                });
              })}
            </tr>
          )}
        </thead>

        <tbody>
          {rows.map((row) => {
            const indentStyle = { paddingLeft: `${row.outline_level * 16 + 8}px` };
            let colIdx = 0;

            return (
              <tr
                key={row.task_uid}
                className={`matrix-row${row.summary ? " summary" : ""}`}
              >
                <td className="matrix-cell-id">{row.task_id}</td>
                <td className="matrix-cell-name" style={indentStyle}>
                  {row.summary ? <strong>{row.task_name}</strong> : row.task_name}
                </td>
                <td className="matrix-cell-unassigned">
                  {!row.summary && row.unassigned && (
                    <span className="matrix-rule-tag" title={m.unassignedRule}>
                      {targetLabel(row.unassigned.target, project.resources, project.skills, project.skill_levels)}
                    </span>
                  )}
                  {!row.summary && !row.unassigned && row.cells.every((c) => !c) && (
                    <span className="matrix-empty-rule">{m.noRule}</span>
                  )}
                </td>

                {/* Cells */}
                {group ? (
                  // Role mode cells
                  row.cells.map((cell, c) => (
                    <td key={data.columns[c].id} className="matrix-cell-value role-mode">
                      {cell ? (
                        <div className="matrix-cell-content">
                          <span className="matrix-units">{formatUnits(cell.units)}u</span>
                          <span className="matrix-work">{hoursText(cell.work)}</span>
                        </div>
                      ) : (
                        <span className="matrix-blank">—</span>
                      )}
                    </td>
                  ))
                ) : (
                  // Person mode cells
                  data.roles.flatMap((role, rIdx) => {
                    const isCollapsed = collapsed.has(role.name);
                    if (isCollapsed) {
                      const roleCell = row.role_cells[rIdx];
                      colIdx += role.resource_uids.length;
                      return [
                        <td key={`${role.name}-collapsed`} className="matrix-cell-value collapsed">
                          {roleCell ? (
                            <div className="matrix-cell-content">
                              <span className="matrix-units">{formatUnits(roleCell.units)}u</span>
                              <span className="matrix-work">{hoursText(roleCell.work)}</span>
                            </div>
                          ) : (
                            <span className="matrix-blank">—</span>
                          )}
                        </td>,
                      ];
                    }

                    return role.resource_uids.map((uid) => {
                      const currentColIdx = colIdx++;
                      const cell = row.cells[currentColIdx];
                      const isEditing = editing?.taskUid === row.task_uid && editing?.resourceUid === uid;

                      if (row.summary) {
                        return (
                          <td key={uid} className="matrix-cell-value summary" title={m.summaryReadOnly}>
                            {cell ? (
                              <span className="matrix-summary-work">{hoursText(cell.work)}</span>
                            ) : (
                              <span className="matrix-blank">—</span>
                            )}
                          </td>
                        );
                      }

                      return (
                        <td
                          key={uid}
                          className={`matrix-cell-value editable${cell ? " assigned" : ""}`}
                          onClick={() => {
                            if (!isEditing) {
                              setEditing({ taskUid: row.task_uid, resourceUid: uid });
                              setEditText(cell ? formatUnits(cell.units) : "");
                            }
                          }}
                        >
                          {isEditing ? (
                            <input
                              ref={inputRef}
                              type="text"
                              role="textbox"
                              aria-label={m.unitsTip}
                              className="matrix-edit-input"
                              value={editText}
                              onChange={(e) => setEditText(e.target.value)}
                              onBlur={() => commitEdit(row.task_uid, uid, cell)}
                              onKeyDown={(e) => onEditKeyDown(e, row.task_uid, uid, cell)}
                            />
                          ) : cell ? (
                            <div className="matrix-cell-content">
                              <span className="matrix-units">{formatUnits(cell.units)}</span>
                              <span className="matrix-work">({hoursText(cell.work)})</span>
                            </div>
                          ) : (
                            <span className="matrix-blank">+</span>
                          )}
                        </td>
                      );
                    });
                  })
                )}

                <td className="matrix-cell-total">
                  {row.total_work > 0 ? <strong>{hoursText(row.total_work)}</strong> : "—"}
                </td>
              </tr>
            );
          })}
        </tbody>

        <tfoot>
          <tr className="matrix-foot-row">
            <td colSpan={2} className="matrix-foot-label">{m.total}</td>
            <td className="matrix-foot-unassigned">
              {unassignedCount > 0 ? m.unassignedCount(unassignedCount) : "—"}
            </td>
            {group ? (
              data.columns.map((col) => (
                <td key={col.id} className="matrix-foot-total">
                  {col.total_work > 0 ? hoursText(col.total_work) : "—"}
                </td>
              ))
            ) : (
              data.roles.flatMap((role) => {
                const isCollapsed = collapsed.has(role.name);
                if (isCollapsed) {
                  return [
                    <td key={`${role.name}-collapsed`} className="matrix-foot-total collapsed">
                      {role.total_work > 0 ? hoursText(role.total_work) : "—"}
                    </td>,
                  ];
                }
                return role.resource_uids.map((uid) => {
                  const col = data.columns.find((c) => c.resource_uid === uid);
                  const total = col ? col.total_work : 0;
                  return (
                    <td key={uid} className="matrix-foot-total">
                      {total > 0 ? hoursText(total) : "—"}
                    </td>
                  );
                });
              })
            )}
            <td className="matrix-foot-grand-total">
              {data.total_work > 0 ? hoursText(data.total_work) : "—"}
            </td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}
