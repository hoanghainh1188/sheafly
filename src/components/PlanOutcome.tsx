import { useMemo, useState } from "react";

import { formatInstant } from "../format";
import { useMessages } from "../i18n";
import type { PlanPreview, Project } from "../types";
import { assignPreviewMessages } from "./AssignPreviewDialog.i18n";

/** What a plan or an AI proposal's trial would do (both come from the engine). */
type PlanResult = Pick<PlanPreview, "assignments" | "moves" | "late" | "overallocated">;

/** Moved tasks listed before the rest are counted. */
const LISTED_MOVES = 200;

/**
 * What a plan would do: the tasks whose dates move (old → new, filtered by person), the
 * ones that would end after their deadline, and the people leveling cannot relieve. The
 * engine computed it all (`preview_plan`); this only shows it.
 */
export function PlanOutcome({ project, plan }: { project: Project; plan: PlanResult }) {
  const p = useMessages(assignPreviewMessages);
  const [person, setPerson] = useState<number | null>(null);
  const tasks = useMemo(() => new Map(project.tasks.map((task) => [task.uid, task])), [project.tasks]);
  const names = useMemo(() => new Map(project.resources.map((item) => [item.uid, item.name])), [project.resources]);
  // Who works on each task once the plan's assignments are in.
  const people = useMemo(() => {
    const byTask = new Map<number, number[]>();
    const add = (task: number, resource: number) => {
      const list = byTask.get(task) ?? [];
      if (!list.includes(resource)) list.push(resource);
      byTask.set(task, list);
    };
    for (const item of project.assignments) add(item.task_uid, item.resource_uid);
    for (const item of plan.assignments) add(item.task_uid, item.resource_uid);
    return byTask;
  }, [project.assignments, plan.assignments]);
  const late = useMemo(() => new Set(plan.late), [plan.late]);
  const involved = useMemo(
    () => [...new Set(plan.moves.flatMap((move) => people.get(move.uid) ?? []))].sort((a, b) => (names.get(a) ?? "").localeCompare(names.get(b) ?? "")),
    [plan.moves, people, names],
  );
  // A person no longer in the plan's moves filters nothing.
  const filter = person != null && involved.includes(person) ? person : null;
  const shown = plan.moves.filter((move) => filter == null || (people.get(move.uid) ?? []).includes(filter));
  const moved = new Set(plan.moves.map((move) => move.uid));
  const lateUnmoved = plan.late.filter((uid) => !moved.has(uid));
  const label = (uid: number) => {
    const task = tasks.get(uid);
    return task ? `${task.id}. ${task.name}` : String(uid);
  };

  return (
    <section aria-label={p.outcome}>
      <h3 className="dialog-subhead">{p.outcome}</h3>
      {plan.overallocated.length > 0 && (
        <p className="bad">{p.stillOver(plan.overallocated.map((uid) => names.get(uid) ?? String(uid)).join(", "))}</p>
      )}
      {plan.moves.length > 0 && (
        <>
          {involved.length > 1 && (
            <label className="plan-filter">
              {p.filterPerson}{" "}
              <select
                aria-label={p.filterPerson}
                value={filter ?? ""}
                onChange={(event) => setPerson(event.target.value ? Number(event.target.value) : null)}
              >
                <option value="">{p.everyone}</option>
                {involved.map((uid) => (
                  <option key={uid} value={uid}>
                    {names.get(uid) ?? uid}
                  </option>
                ))}
              </select>
            </label>
          )}
          <div className="suggestion-wrap">
            <table className="suggestions" aria-label={p.moves}>
              <thead>
                <tr>
                  <th scope="col">{p.task}</th>
                  <th scope="col">{p.person}</th>
                  <th scope="col">{p.start}</th>
                  <th scope="col">{p.finishColumn}</th>
                </tr>
              </thead>
              <tbody>
                {shown.slice(0, LISTED_MOVES).map((move) => (
                  <tr key={move.uid}>
                    <th scope="row">{label(move.uid)}</th>
                    <td>{(people.get(move.uid) ?? []).map((uid) => names.get(uid) ?? uid).join(", ")}</td>
                    <td className="num">{`${formatInstant(move.start_before)} → ${formatInstant(move.start)}`}</td>
                    <td className="num">
                      {`${formatInstant(move.finish_before)} → ${formatInstant(move.finish)}`}
                      {late.has(move.uid) && <span className="bad plan-late"> {p.lateMark}</span>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {shown.length > LISTED_MOVES && <p className="meta">{p.moreMoves(shown.length - LISTED_MOVES)}</p>}
        </>
      )}
      {lateUnmoved.length > 0 && <p className="meta">{p.lateUnmoved(lateUnmoved.map(label).join(", "))}</p>}
    </section>
  );
}
