import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";

import { assignmentMatrix, errorMessage } from "../../api";
import { formatUnits, parseUnitsOrPercent } from "../../assignmentUnits";
import { formatHours } from "../../format";
import { useMessages } from "../../i18n";
import type { AssignmentMatrix, MatrixCell, MatrixRow, Project, RuleTarget, SkillDef, SkillLevel } from "../../types";
import { matrixMessages } from "./AssignmentMatrixView.i18n";
import { clampToGrid, moveInGrid, type GridPos } from "./matrixGrid";

type Messages = (typeof matrixMessages)["vi"];

interface Props {
  project: Project;
  group: boolean;
  visibleTaskUids?: Set<number> | null;
  onAssign: (taskUid: number, resourceUid: number, units: number) => void;
  onUpdate: (assignmentUid: number, units: number) => void;
  onRemove: (assignmentUid: number) => void;
}

/** A value column as drawn: a person, or a role (role mode, or a collapsed role group). */
type ValueColumn =
  | { kind: "person"; uid: number; index: number }
  | { kind: "role"; key: string; roleIndex: number; index: number | null };

interface Edit {
  taskUid: number;
  resourceUid: number;
  cell: MatrixCell | null;
  pos: GridPos;
}

function targetLabel(
  target: RuleTarget,
  resources: Project["resources"],
  skills: SkillDef[],
  skillLevels: SkillLevel[],
  m: Messages,
): string {
  if (target.type === "resource") {
    const res = resources.find((r) => r.uid === target.uid);
    return res ? res.name : m.resourceFallback(target.uid);
  }
  const skill = skills.find((s) => s.id === target.id);
  const skillName = skill ? skill.name : m.skillFallback(target.id);
  if (target.min_level != null) {
    const level = skillLevels.find((l) => l.id === target.min_level);
    return `${skillName}:${level ? level.name : target.min_level}`;
  }
  return skillName;
}

function valueColumns(data: AssignmentMatrix, group: boolean, collapsed: Set<string>): ValueColumn[] {
  if (group) {
    return data.columns.map((col, c) => ({ kind: "role", key: col.id, roleIndex: c, index: c }));
  }
  let index = 0;
  return data.roles.flatMap((role, roleIndex): ValueColumn[] => {
    if (collapsed.has(role.name)) {
      index += role.resource_uids.length;
      return role.resource_uids.length > 0 ? [{ kind: "role", key: `${role.name}-collapsed`, roleIndex, index: null }] : [];
    }
    return role.resource_uids.map((uid) => ({ kind: "person", uid, index: index++ }));
  });
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
  // The matrix with the mode it was read for: one of the other mode is never drawn, so a
  // click right after switching cannot land on a column of the old layout.
  const [loaded, setLoaded] = useState<{ group: boolean; matrix: AssignmentMatrix } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [editing, setEditing] = useState<Edit | null>(null);
  const [editText, setEditText] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [active, setActive] = useState<GridPos>({ row: 0, col: 0 });
  const editRef = useRef<Edit | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const tableRef = useRef<HTMLTableElement>(null);

  useEffect(() => {
    let current = true;
    setError(null);
    assignmentMatrix(group)
      .then((matrix) => {
        if (current) setLoaded({ group, matrix });
      })
      .catch((err) => {
        if (current) setError(errorMessage(err));
      });
    return () => {
      current = false;
    };
  }, [project, group]);

  useEffect(() => {
    editRef.current = null;
    setEditing(null);
  }, [group]);

  useEffect(() => {
    if (editing) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [editing]);

  const data = loaded?.group === group ? loaded.matrix : null;

  const rows = useMemo(() => {
    if (!data) return [];
    if (!visibleTaskUids) return data.rows;
    return data.rows.filter((r) => visibleTaskUids.has(r.task_uid));
  }, [data, visibleTaskUids]);

  const columns = useMemo(() => (data ? valueColumns(data, group, collapsed) : []), [data, group, collapsed]);
  const focusAt = clampToGrid(active, rows.length, columns.length);

  if (!data) {
    return (
      <div className="matrix-view loading" aria-busy={!error}>
        {error ? (
          <p className="bad" role="alert">{m.loadFailed(error)}</p>
        ) : (
          <p className="meta" role="status">{m.loading}</p>
        )}
      </div>
    );
  }

  const unitsText = (units: number) => m.unitsShort(formatUnits(units));
  const hoursText = (work: number) => {
    const h = formatHours(work / 60);
    return h ? m.hours(h) : "";
  };

  const toggleCollapse = (roleName: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(roleName)) next.delete(roleName);
      else next.add(roleName);
      return next;
    });
  };

  const focusCell = (pos: GridPos) => {
    setActive(pos);
    tableRef.current?.querySelector<HTMLElement>(`[data-cell="${pos.row}:${pos.col}"]`)?.focus();
  };

  const startEdit = (row: MatrixRow, uid: number, cell: MatrixCell | null, pos: GridPos) => {
    const edit = { taskUid: row.task_uid, resourceUid: uid, cell, pos };
    editRef.current = edit;
    setActive(pos);
    setEditing(edit);
    setEditText(cell ? formatUnits(cell.units) : "");
    setInvalid(false);
  };

  const endEdit = (refocus: boolean) => {
    const edit = editRef.current;
    editRef.current = null;
    setEditing(null);
    setInvalid(false);
    if (refocus && edit) focusCell(edit.pos);
  };

  /** Sends the typed units; text that does not read keeps the field open and marked. */
  const commitEdit = (refocus: boolean) => {
    const edit = editRef.current;
    if (!edit) return; // already committed or cancelled (blur while the input unmounts)
    const trimmed = editText.trim();
    const units = trimmed === "" ? 0 : parseUnitsOrPercent(trimmed);
    if (units == null) {
      setInvalid(true);
      return;
    }
    const assignment = edit.cell?.assignment_uid;
    if (units === 0) {
      if (assignment != null) onRemove(assignment);
    } else if (assignment != null) {
      if (units !== edit.cell?.units) onUpdate(assignment, units);
    } else {
      onAssign(edit.taskUid, edit.resourceUid, units);
    }
    endEdit(refocus);
  };

  const onEditKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      commitEdit(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      endEdit(true);
    }
  };

  const cellAt = (row: MatrixRow, column: ValueColumn): MatrixCell | null =>
    column.kind === "person"
      ? (row.cells[column.index] ?? null)
      : ((column.index != null ? row.cells[column.index] : row.role_cells[column.roleIndex]) ?? null);

  const onCellKeyDown = (event: KeyboardEvent<HTMLTableCellElement>, row: MatrixRow, column: ValueColumn, pos: GridPos) => {
    if (event.target !== event.currentTarget) return;
    const next = moveInGrid(pos, event.key, rows.length, columns.length, event.ctrlKey || event.metaKey);
    if (next) {
      event.preventDefault();
      focusCell(next);
      return;
    }
    const cell = cellAt(row, column);
    const editable = column.kind === "person" && !row.summary;
    if ((event.key === "Enter" || event.key === "F2") && editable) {
      event.preventDefault();
      startEdit(row, column.uid, cell, pos);
    } else if ((event.key === "Delete" || event.key === "Backspace") && column.kind === "person" && cell?.assignment_uid != null) {
      event.preventDefault();
      onRemove(cell.assignment_uid);
    }
  };

  const renderValue = (row: MatrixRow, column: ValueColumn, rowIndex: number, colIndex: number) => {
    const pos = { row: rowIndex, col: colIndex };
    const cell = cellAt(row, column);
    const key = column.kind === "person" ? column.uid : column.key;
    const common = {
      role: "gridcell",
      "data-cell": `${rowIndex}:${colIndex}`,
      tabIndex: focusAt.row === rowIndex && focusAt.col === colIndex ? 0 : -1,
      onFocus: () => setActive(pos),
      onKeyDown: (event: KeyboardEvent<HTMLTableCellElement>) => onCellKeyDown(event, row, column, pos),
    };

    if (column.kind === "role") {
      return (
        <td key={key} {...common} aria-readonly className={`matrix-cell-value${group ? " role-mode" : " collapsed"}`}>
          {cell ? (
            <div className="matrix-cell-content">
              <span className="matrix-units">{unitsText(cell.units)}</span>
              <span className="matrix-work">{hoursText(cell.work)}</span>
            </div>
          ) : (
            <span className="matrix-blank">—</span>
          )}
        </td>
      );
    }

    if (row.summary) {
      const own = cell?.assignment_uid;
      return (
        <td key={key} {...common} aria-readonly className="matrix-cell-value summary" title={own != null ? m.summaryOwnTip : m.summaryReadOnly}>
          {cell ? <span className="matrix-summary-work">{hoursText(cell.work)}</span> : <span className="matrix-blank">—</span>}
          {own != null && (
            <button
              type="button"
              tabIndex={-1}
              className="matrix-remove-btn"
              aria-label={m.removeSummary(row.task_name)}
              onClick={() => onRemove(own)}
            >
              ×
            </button>
          )}
        </td>
      );
    }

    const isEditing = editing?.taskUid === row.task_uid && editing.resourceUid === column.uid;
    return (
      <td
        key={key}
        {...common}
        className={`matrix-cell-value editable${cell ? " assigned" : ""}`}
        onClick={() => {
          if (!isEditing) startEdit(row, column.uid, cell, pos);
        }}
      >
        {isEditing ? (
          <>
            <input
              ref={inputRef}
              type="text"
              aria-label={m.unitsTip}
              aria-invalid={invalid}
              title={invalid ? m.badUnits : undefined}
              className="matrix-edit-input"
              value={editText}
              onChange={(e) => {
                setEditText(e.target.value);
                setInvalid(false);
              }}
              onBlur={() => commitEdit(false)}
              onKeyDown={onEditKeyDown}
            />
            {invalid && (
              <span className="matrix-edit-error" role="alert">
                {m.badUnits}
              </span>
            )}
          </>
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
  };

  const unassignedCount = rows.filter((r) => !r.summary && r.cells.every((c) => !c)).length;

  return (
    <div className="matrix-view" role="region" aria-label={m.title}>
      {error && (
        <p className="bad" role="alert">
          {m.loadFailed(error)}
        </p>
      )}
      {rows.length === 0 && <p className="meta">{m.empty}</p>}
      <table className="matrix-table" role="grid" ref={tableRef} aria-label={m.title}>
        <thead>
          <tr>
            <th rowSpan={group ? 1 : 2} className="matrix-col-id">{m.id}</th>
            <th rowSpan={group ? 1 : 2} className="matrix-col-name">{m.task}</th>
            <th rowSpan={group ? 1 : 2} className="matrix-col-unassigned" title={m.unassigned}>
              {m.unassigned}
            </th>
            {group
              ? data.columns.map((col) => (
                  <th key={col.id} className="matrix-col-role" title={m.roleOf(col.label || m.noRole)}>
                    {col.label || m.noRole}
                  </th>
                ))
              : data.roles.map((role) => {
                  const isCollapsed = collapsed.has(role.name);
                  const colSpan = isCollapsed ? 1 : role.resource_uids.length;
                  if (role.resource_uids.length === 0) return null;
                  return (
                    <th key={role.name} colSpan={colSpan} className="matrix-role-group">
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
                })}
            <th rowSpan={group ? 1 : 2} className="matrix-col-total">{m.total}</th>
          </tr>

          {!group && (
            <tr>
              {data.roles.flatMap((role) => {
                if (collapsed.has(role.name)) {
                  return role.resource_uids.length > 0
                    ? [
                        <th key={`${role.name}-collapsed`} className="matrix-col-person collapsed" title={role.name || m.noRole}>
                          {role.name || m.noRole}
                        </th>,
                      ]
                    : [];
                }
                return role.resource_uids.map((uid) => {
                  const res = project.resources.find((r) => r.uid === uid);
                  const name = res ? res.name : m.resourceFallback(uid);
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
          {rows.map((row, rowIndex) => (
            <tr key={row.task_uid} className={`matrix-row${row.summary ? " summary" : ""}`}>
              <td role="gridcell" className="matrix-cell-id">{row.task_id}</td>
              <td role="rowheader" className="matrix-cell-name" style={{ paddingLeft: `${row.outline_level * 16 + 8}px` }}>
                {row.summary ? <strong>{row.task_name}</strong> : row.task_name}
              </td>
              <td role="gridcell" className="matrix-cell-unassigned">
                {!row.summary && row.unassigned && (
                  <span className="matrix-rule-tag" title={m.unassignedRule}>
                    {targetLabel(row.unassigned.target, project.resources, project.skills, project.skill_levels ?? [], m)}
                  </span>
                )}
                {!row.summary && !row.unassigned && row.cells.every((c) => !c) && (
                  <span className="matrix-empty-rule">{m.noRule}</span>
                )}
              </td>
              {columns.map((column, colIndex) => renderValue(row, column, rowIndex, colIndex))}
              <td role="gridcell" className="matrix-cell-total">
                {row.total_work > 0 ? <strong>{hoursText(row.total_work)}</strong> : "—"}
              </td>
            </tr>
          ))}
        </tbody>

        <tfoot>
          <tr className="matrix-foot-row">
            <td colSpan={2} className="matrix-foot-label">{m.total}</td>
            <td className="matrix-foot-unassigned">
              {unassignedCount > 0 ? m.unassignedCount(unassignedCount) : "—"}
            </td>
            {columns.map((column) => {
              const total =
                column.kind === "person"
                  ? (data.columns[column.index]?.total_work ?? 0)
                  : group
                    ? (data.columns[column.roleIndex]?.total_work ?? 0)
                    : (data.roles[column.roleIndex]?.total_work ?? 0);
              return (
                <td
                  key={column.kind === "person" ? column.uid : column.key}
                  className={`matrix-foot-total${column.kind === "role" && !group ? " collapsed" : ""}`}
                >
                  {total > 0 ? hoursText(total) : "—"}
                </td>
              );
            })}
            <td className="matrix-foot-grand-total">
              {data.total_work > 0 ? hoursText(data.total_work) : "—"}
            </td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}
