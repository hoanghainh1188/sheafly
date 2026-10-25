import { distinctValues, formatFieldValue, valueKey, valueOf } from "./fieldValues";
import { pick } from "./i18n";
import { outsideSpan } from "./spanFit";
import type { Assignment, FieldDef, Project, Resource, Task } from "./types";
import { viewRowsMessages } from "./viewRows.i18n";

/** A custom field, by id, as a filter or group mode. */
export type FieldMode = `field:${number}`;
/** A criterion of the schedule health check, by number: its tasks. */
export type HealthMode = `health:${number}`;
export type FilterMode = "none" | "critical" | "resource" | "late" | "interrupts" | "span" | FieldMode | HealthMode;
export type GroupMode = "none" | "resource" | "summary" | FieldMode;
export type HighlightMode = "none" | "critical" | "late" | "interrupts" | "span";

/** The criterion number a health mode names, or null for another mode. */
export function healthCriterionOf(mode: string): number | null {
  if (!mode.startsWith("health:")) return null;
  const number = Number(mode.slice("health:".length));
  return Number.isInteger(number) && number >= 1 && number <= 14 ? number : null;
}

export function fieldMode(field: FieldDef): FieldMode {
  return `field:${field.id}`;
}

/** The custom field a mode names, or null for another mode or a field that is gone. */
export function fieldOfMode(project: Project, mode: string): FieldDef | null {
  if (!mode.startsWith("field:")) return null;
  const id = Number(mode.slice("field:".length));
  return project.fields.find((field) => field.id === id) ?? null;
}

export interface TaskViewRow {
  kind: "task";
  key: string;
  task: Task;
  highlighted: boolean;
}

export interface GroupViewRow {
  kind: "group";
  key: string;
  label: string;
}

export type ViewRow = TaskViewRow | GroupViewRow;

/**
 * Filter hides non-matching rows but keeps a summary when any descendant matches.
 * Highlight only tints rows that remain visible. Group then reorders those rows.
 * None of this rewrites WBS codes or the saved task order. Group headers it names itself
 * are in the current interface language. A custom field filter keeps tasks whose value
 * has `fieldValue` as its key (see `valueKey`; "" keeps tasks with no value), and a
 * mode naming a field that no longer exists does nothing. "span" filters or highlights
 * the tasks that do not fit before `spanEnd` (see `outsideSpan`); without an end it does
 * nothing. A health mode keeps the tasks in `healthTasks` (the criterion's, from the last
 * report); without them it does nothing.
 */
export function buildViewRows(
  project: Project,
  filter: FilterMode,
  resourceUid: number | null,
  group: GroupMode,
  highlight: HighlightMode,
  fieldValue = "",
  spanEnd: number | null = null,
  healthTasks: ReadonlySet<number> | null = null,
): ViewRow[] {
  const visible = visibleTasks(project, filter, resourceUid, fieldValue, spanEnd, healthTasks);
  const lit = highlighter(highlight, spanEnd);
  if (group === "resource") return groupByResource(project, visible, lit);
  if (group === "summary") return groupBySummary(project, visible, lit);
  const field = fieldOfMode(project, group);
  if (field) return groupByField(field, visible, lit);
  return visible.map((task) => taskRow(`task-${task.uid}`, task, lit));
}

/** Whether a row is tinted. */
type Lit = (task: Task) => boolean;

function visibleTasks(
  project: Project,
  filter: FilterMode,
  resourceUid: number | null,
  fieldValue: string,
  spanEnd: number | null,
  healthTasks: ReadonlySet<number> | null,
): Task[] {
  const field = fieldOfMode(project, filter);
  const health = filter.startsWith("health:");
  if (filter === "none" || (filter.startsWith("field:") && !field)) return project.tasks;
  if (filter === "span" && spanEnd == null) return project.tasks;
  if (health && healthTasks == null) return project.tasks;
  const byUid = new Map(project.tasks.map((task) => [task.uid, task]));
  const direct = new Set<number>();
  const assignedTo = filter === "resource" ? tasksAssignedTo(project.assignments, resourceUid) : new Set<number>();
  for (const task of project.tasks) {
    // A summary with no value matches nothing: it shows above its matching tasks only.
    const matches = field
      ? valueKey(valueOf(task, field)) === fieldValue && !(fieldValue === "" && task.summary)
      : filter === "span"
        ? spanEnd != null && outsideSpan(task, spanEnd)
        : health
          ? (healthTasks?.has(task.uid) ?? false)
          : matchesFilter(task, assignedTo, filter, resourceUid);
    if (matches) direct.add(task.uid);
  }
  const keep = new Set(direct);
  for (const uid of direct) {
    let parent = byUid.get(uid)?.parent_uid ?? null;
    while (parent != null) {
      const ancestor = byUid.get(parent);
      if (!ancestor) break;
      if (ancestor.summary) keep.add(ancestor.uid);
      parent = ancestor.parent_uid;
    }
  }
  return project.tasks.filter((task) => keep.has(task.uid));
}

function matchesFilter(
  task: Task,
  assignedTo: ReadonlySet<number>,
  filter: FilterMode,
  resourceUid: number | null,
): boolean {
  if (filter === "critical") return task.critical;
  if (filter === "late") return task.late;
  if (filter === "interrupts") return task.interrupts;
  if (filter === "resource") {
    if (resourceUid == null) return false;
    return assignedTo.has(task.uid);
  }
  return true;
}

function highlighter(highlight: HighlightMode, spanEnd: number | null): Lit {
  if (highlight === "critical") return (task) => task.critical;
  if (highlight === "late") return (task) => task.late;
  if (highlight === "interrupts") return (task) => task.interrupts;
  if (highlight === "span" && spanEnd != null) return (task) => outsideSpan(task, spanEnd);
  return () => false;
}

function taskRow(key: string, task: Task, lit: Lit): TaskViewRow {
  return { kind: "task", key, task, highlighted: lit(task) };
}

function groupByResource(project: Project, tasks: Task[], lit: Lit): ViewRow[] {
  const buckets = new Map<number, Task[]>();
  const unassigned: Task[] = [];
  const assignees = assigneesByTask(project.resources, project.assignments);
  for (const task of tasks) {
    const ids = assignees.get(task.uid) ?? [];
    if (ids.length === 0) {
      unassigned.push(task);
      continue;
    }
    for (const id of ids) {
      const list = buckets.get(id) ?? [];
      list.push(task);
      buckets.set(id, list);
    }
  }
  const rows: ViewRow[] = [];
  for (const resource of project.resources) {
    const list = buckets.get(resource.uid);
    if (!list || list.length === 0) continue;
    rows.push({
      kind: "group",
      key: `group-${resource.uid}`,
      label: resource.name.trim() || pick(viewRowsMessages).resourceFallback(resource.uid),
    });
    for (const task of list) {
      rows.push(taskRow(`group-${resource.uid}-task-${task.uid}`, task, lit));
    }
  }
  if (unassigned.length > 0) {
    rows.push({ kind: "group", key: "group-unassigned", label: pick(viewRowsMessages).unassigned });
    for (const task of unassigned) {
      rows.push(taskRow(`group-unassigned-task-${task.uid}`, task, lit));
    }
  }
  return rows;
}

/**
 * Leaf tasks sit under their nearest summary ancestor; summaries are headers only.
 * Groups follow the summary's outline position, and top-level leaves go last.
 */
function groupBySummary(project: Project, tasks: Task[], lit: Lit): ViewRow[] {
  const byUid = new Map(project.tasks.map((task) => [task.uid, task]));
  const buckets = new Map<number, Task[]>();
  const orphans: Task[] = [];
  for (const task of tasks) {
    if (task.summary) continue;
    const owner = nearestSummary(task, byUid);
    if (!owner) {
      orphans.push(task);
      continue;
    }
    const list = buckets.get(owner.uid) ?? [];
    list.push(task);
    buckets.set(owner.uid, list);
  }
  const rows: ViewRow[] = [];
  for (const summary of project.tasks) {
    const list = buckets.get(summary.uid);
    if (!list || list.length === 0) continue;
    rows.push({ kind: "group", key: `group-summary-${summary.uid}`, label: summaryLabel(summary) });
    for (const task of list) {
      rows.push(taskRow(`group-summary-${summary.uid}-task-${task.uid}`, task, lit));
    }
  }
  if (orphans.length > 0) {
    rows.push({ kind: "group", key: "group-no-summary", label: pick(viewRowsMessages).noSummary });
    for (const task of orphans) {
      rows.push(taskRow(`group-no-summary-task-${task.uid}`, task, lit));
    }
  }
  return rows;
}

/**
 * One group per value tasks have, in the order `distinctValues` gives (a list field's own
 * order), then the tasks with no value. A summary with no value is left out: its tasks
 * are listed on their own.
 */
function groupByField(field: FieldDef, tasks: Task[], lit: Lit): ViewRow[] {
  const buckets = new Map<string, Task[]>();
  for (const task of tasks) {
    const key = valueKey(valueOf(task, field));
    if (key === "" && task.summary) continue;
    const list = buckets.get(key) ?? [];
    list.push(task);
    buckets.set(key, list);
  }
  const rows: ViewRow[] = [];
  // Row keys use the group's place, not its value: a value like "x-task-3" must not
  // make a key another row already has.
  const push = (key: string, label: string, place: string) => {
    const list = buckets.get(key);
    if (!list || list.length === 0) return;
    rows.push({ kind: "group", key: `group-field-${place}`, label });
    for (const task of list) rows.push(taskRow(`group-field-${place}-task-${task.uid}`, task, lit));
  };
  distinctValues(field, tasks).forEach((value, index) => push(valueKey(value), formatFieldValue(value), String(index)));
  push("", pick(viewRowsMessages).noValue(field.name), "none");
  return rows;
}

function nearestSummary(task: Task, byUid: Map<number, Task>): Task | null {
  let parent = task.parent_uid;
  while (parent != null) {
    const ancestor = byUid.get(parent);
    if (!ancestor) return null;
    if (ancestor.summary) return ancestor;
    parent = ancestor.parent_uid;
  }
  return null;
}

function summaryLabel(summary: Task): string {
  const name = summary.name.trim() || pick(viewRowsMessages).taskFallback(summary.uid);
  return summary.wbs ? `${summary.wbs} ${name}` : name;
}

/** Each task's assignees, once each and only those the project has, in one pass. */
function assigneesByTask(resources: Resource[], assignments: Assignment[]): Map<number, number[]> {
  const known = new Set(resources.map((resource) => resource.uid));
  const byTask = new Map<number, number[]>();
  for (const item of assignments) {
    if (!known.has(item.resource_uid)) continue;
    const ids = byTask.get(item.task_uid) ?? [];
    if (!ids.includes(item.resource_uid)) ids.push(item.resource_uid);
    byTask.set(item.task_uid, ids);
  }
  return byTask;
}

/** The tasks assigned to `resourceUid`, for the resource filter. */
function tasksAssignedTo(assignments: Assignment[], resourceUid: number | null): Set<number> {
  return new Set(assignments.filter((item) => item.resource_uid === resourceUid).map((item) => item.task_uid));
}
