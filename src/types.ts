export type ConstraintType =
  | "asap"
  | "alap"
  | "snet"
  | "snlt"
  | "fnet"
  | "fnlt"
  | "mso"
  | "mfo";

export type DepKind = "fs" | "ss" | "ff" | "sf";
export type TaskType = "fixed_units" | "fixed_duration" | "fixed_work";
export type Accrual = "start" | "prorated" | "end";
export type ResourceKind = "person" | "machine";

export interface TimeRange {
  start_minute: number;
  end_minute: number;
}

/** Hours that replace the default week on every day of `[from, to]` ("YYYY-MM-DD"). */
export interface WorkWeek {
  name: string;
  from: string;
  to: string;
  week: TimeRange[][];
}

/** What a calendar based on calendar `id` sets itself; the rest comes from the base. */
export interface CalendarBase {
  id: number;
  /** A weekday set on this calendar, or null to take the base's. */
  week: (TimeRange[] | null)[];
  exceptions: Record<string, TimeRange[]>;
  work_weeks: WorkWeek[];
}

export interface Calendar {
  id: number;
  name: string;
  /** The merged hours: for a based calendar, its own over its base's. */
  week: TimeRange[][];
  exceptions: Record<string, TimeRange[]>;
  /** Ordered by `from`, never overlapping. */
  work_weeks: WorkWeek[];
  base: CalendarBase | null;
}

export interface Baseline {
  number: number;
  start: number;
  finish: number;
  work: number;
  cost: number;
  /** Segments of a split task when the baseline was set; empty otherwise. */
  segments: Segment[];
  /** Fixed-cost part of `cost`; absent in baselines saved before it was kept. */
  fixed_cost?: number;
}

/** Interruption of a task's work; both values are working minutes. */
export interface Split {
  offset: number;
  gap: number;
  /** Made by leveling to let an urgent task in; omitted when false. */
  leveling?: boolean;
}

/** One scheduled piece of a split task, as instants. */
export interface Segment {
  start: number;
  finish: number;
}

export interface Task {
  uid: number;
  id: number;
  name: string;
  wbs: string;
  outline_level: number;
  parent_uid: number | null;
  notes: string;
  priority: number;
  milestone: boolean;
  summary: boolean;
  manual: boolean;
  duration: number;
  constraint_type: ConstraintType;
  constraint_date: number | null;
  calendar_id: number | null;
  task_type: TaskType;
  effort_driven: boolean;
  percent_complete: number;
  actual_start: number | null;
  actual_finish: number | null;
  remaining_duration: number;
  remaining_work: number;
  fixed_cost: number;
  fixed_cost_accrual: Accrual;
  leveling_delay: number;
  /** Urgent: when leveling, it goes ahead of work in progress ranked below it on the same people. */
  interrupts: boolean;
  /** The least and most effort for the finish forecast, in working minutes; both or neither. */
  work_low?: number | null;
  work_high?: number | null;
  /** The issue in an external tracker this task was pulled from. */
  external?: ExternalLink | null;
  start: number;
  finish: number;
  early_start: number;
  early_finish: number;
  late_start: number;
  late_finish: number;
  total_slack: number;
  free_slack: number;
  critical: boolean;
  units: number;
  work: number;
  actual_work: number;
  cost: number;
  actual_cost: number;
  remaining_cost: number;
  /** Hand-entered actual cost; counts only while `Project.editable_actual_costs` is on. */
  manual_actual_cost: number | null;
  conflict: boolean;
  predecessors: string;
  baselines: Baseline[];
  deadline: number | null;
  late: boolean;
  recurrence: Recurrence | null;
  series_uid: number | null;
  occurrence_date: number | null;
  detached: boolean;
  /** Custom field values by field id (JSON object keys are strings); no key, no value. */
  values: Record<string, FieldValue>;
  splits: Split[];
  /** Computed by the engine; empty when the task is not split. */
  segments: Segment[];
}

/** Mirror of `fields::FieldKind`. A choice is one of `options`, in the user's order. */
export type FieldKind = { type: "text" } | { type: "number" } | { type: "choice"; options: string[] };

/** Mirror of `level_order::Criterion`: what one key of the leveling order compares. */
export type Criterion =
  | { type: "start" }
  | { type: "priority" }
  | { type: "slack" }
  | { type: "deadline" }
  | { type: "duration" }
  | { type: "id" }
  | { type: "field"; id: number };

/** Mirror of `level_order::SortKey`. */
export interface SortKey {
  criterion: Criterion;
  descending: boolean;
}

/** Mirror of `fields::FieldDef`: a custom task field the project declares. */
export interface FieldDef {
  id: number;
  name: string;
  kind: FieldKind;
}

/** Mirror of `fields::FieldValue`: a number for number fields, text for the others. */
export type FieldValue = number | string;

/** Weekdays use 0 = Monday … 6 = Sunday, like calendar weeks. Ordinals are 1–4, or 5 for the last one. */
export type RecurrencePattern =
  | { kind: "daily"; every: number }
  | { kind: "calendar_daily"; every: number }
  | { kind: "weekly"; every: number; weekdays: number[] }
  | { kind: "monthly"; every: number; day: number }
  | { kind: "monthly_nth"; every: number; ordinal: number; weekday: number }
  | { kind: "yearly"; every: number; month: number; day: number }
  | { kind: "yearly_nth"; every: number; month: number; ordinal: number; weekday: number };

/** Units are 1 for 100 %. */
export interface SeriesResource {
  uid: number;
  units: number;
}

export type RecurrenceEnd = { kind: "count"; count: number } | { kind: "until"; date: number };

export interface RecurrenceSpec {
  name: string;
  duration: number;
  start: number;
  pattern: RecurrencePattern;
  end: RecurrenceEnd;
  calendar_id: number | null;
  /** Only in series saved before per-resource units; read as units 1. */
  resource_uids?: number[];
  resources: SeriesResource[];
  /** Never sets the project finish or the critical path. Absent in older series. */
  background?: boolean;
}

export interface Recurrence {
  spec: RecurrenceSpec;
  skipped: number[];
}

export interface Dependency {
  uid: number;
  predecessor_uid: number;
  successor_uid: number;
  kind: DepKind;
  lag: number;
}

export interface Resource {
  uid: number;
  name: string;
  kind: ResourceKind;
  max_units: number;
  calendar_id: number | null;
  standard_rate: number;
  overtime_rate: number;
  cost_per_use: number;
  /** Days with other units than `max_units`, in date order. */
  availability: AvailabilityPeriod[];
  /** Ids of the project's skills this resource has, ascending. */
  skills: number[];
  /** Email address; absent when unknown. Unique in the project, letter case aside. */
  email?: string;
  /** Role or team, a label; absent when none. */
  group?: string;
  /** The level of each skill that has one: skill id (as a key) → level id. */
  skill_levels?: Record<string, number>;
  /** Own productivity factor (0.5–1); absent follows the project's for people, 1 for machines. */
  focus?: number | null;
}

/** Mirror of `skill_levels::SkillLevel`, one step of the project's scale. */
export interface SkillLevel {
  id: number;
  name: string;
}

/** Mirror of `skill_levels::SkillLevelInput`: an existing level by id, or a new one. */
export interface SkillLevelInput {
  id?: number | null;
  name: string;
}

/** Mirror of `skill_levels::SkillLevelOf`: the level a resource has in one skill. */
export interface SkillLevelOf {
  skill: number;
  level: number;
}

/** Mirror of `skills::SkillDef`. */
export interface SkillDef {
  id: number;
  name: string;
}

/** Mirror of `skills::Condition`: the task's value of `field` is one of `values`. */
export interface Condition {
  field: number;
  values: FieldValue[];
}

/** Mirror of `skills::RuleTarget`: one person, or everyone with a skill. */
export type RuleTarget =
  | { type: "resource"; uid: number }
  | { type: "skill"; id: number; min_level?: number | null };

/**
 * Mirror of `skills::AssignmentRule`: all conditions must hold (none matches every task);
 * the first rule that matches a task is used.
 */
export interface AssignmentRule {
  conditions: Condition[];
  target: RuleTarget;
  units: number;
}

/** Mirror of `suggest::Pick`: one person offered for a task, with estimated dates. */
export interface Pick {
  resource_uid: number;
  start: number;
  finish: number;
}

/** Mirror of `suggest::NoPick`: why a task has no one to offer. */
export type NoPick = "no_rule" | "no_one_with_skill" | "no_room";

/** Mirror of `suggest::Suggestion`: the people offered for one unassigned task, best first. */
export interface Suggestion {
  task_uid: number;
  /** Index of the rule that matched. */
  rule: number | null;
  /** Units the assignment would have: the rule's. */
  units: number;
  picks: Pick[];
  reason: NoPick | null;
}

/** Mirror of `skills::TaskAssignment`, one row of `assign_tasks`. */
export interface TaskAssignment {
  task_uid: number;
  resource_uid: number;
  units: number;
}

/** Mirror of `load_map::Bucket`. */
export type LoadBucket = "day" | "week" | "month";

/** Mirror of `load_map::LoadGroup`. */
export type LoadGroup =
  | { type: "person" }
  | { type: "group" }
  | { type: "skill"; id: number; min_level?: number };

/** Mirror of `load_map::LoadCell`: working minutes of work and of capacity. */
export interface LoadCell {
  work: number;
  unassigned?: number;
  capacity: number;
  baseline?: number;
}

/** Mirror of `load_map::LoadRow`. */
export interface LoadRow {
  resource_uid?: number;
  label: string;
  cells: LoadCell[];
}

/** Mirror of `load_map::LoadMap`: `buckets` are the first day of each, "YYYY-MM-DD". */
export interface LoadMap {
  buckets: string[];
  rows: LoadRow[];
}

/** Mirror of `curves::CurveMeasure`. */
export type CurveMeasure = "cost" | "work" | "progress";

/** Mirror of `curves::CurveBucket`. */
export type CurveBucket = "day" | "week" | "month";

/** Mirror of `curves::CurveFilter`. */
export type CurveFilter =
  | { type: "project" }
  | { type: "person"; uid: number }
  | { type: "group"; name: string }
  | { type: "skill"; id: number; min_level?: number };

/** Mirror of `curves::CurvePoint`. */
export interface CurvePoint {
  date: number;
  label: string;
  baseline_period?: number | null;
  scheduled_period: number;
  actual_period?: number | null;
  earned_period?: number | null;
  baseline_cumulative?: number | null;
  scheduled_cumulative: number;
  actual_cumulative?: number | null;
  earned_cumulative?: number | null;
}

/** Mirror of `curves::CurvesReport`. */
export interface CurvesReport {
  measure: CurveMeasure;
  bucket: CurveBucket;
  filter: CurveFilter;
  baseline: number | null;
  status_date: number | null;
  baseline_finish: number | null;
  scheduled_finish: number | null;
  total_baseline: number | null;
  total_scheduled: number;
  total_actual: number | null;
  total_earned?: number | null;
  points: CurvePoint[];
}

/** Mirror of `forecast::Percentiles`: finish dates (ms) by which that share of runs had finished. */
export interface Percentiles {
  p50: number;
  p80: number;
  p90: number;
}

/** Mirror of `forecast::TaskForecast`. */
export interface TaskForecast extends Percentiles {
  uid: number;
  /** Share of runs finished by the task's deadline; null without one. */
  on_time: number | null;
  /** Share of runs in which the task was critical. */
  criticality: number;
}

/** Mirror of `forecast::Forecast`. */
export interface Forecast {
  iterations: number;
  /** The project finish as scheduled now. */
  planned_finish: number;
  /** The project finish once leveled, with today's effort and no variation. */
  leveled_finish: number;
  finish: Percentiles;
  /** Runs finishing the project on each day (midnight of that day, ms), in date order. */
  histogram: [number, number][];
  tasks: TaskForecast[];
}

/** Mirror of `command::LevelingDelay`. */
export interface LevelingDelay {
  uid: number;
  minutes: number;
}

/** Mirror of `command::LevelingSplits`: all the leveling splits of one task. */
export interface LevelingSplits {
  uid: number;
  splits: Split[];
}

/** Mirror of `plan::PlanMove`: a task whose dates the plan changes. */
export interface PlanMove {
  uid: number;
  start_before: number;
  finish_before: number;
  start: number;
  finish: number;
}

/** Mirror of `plan::PlanPreview`, what `preview_plan` returns and `apply_plan` takes back. */
export interface PlanPreview {
  assignments: TaskAssignment[];
  delays: LevelingDelay[];
  splits: LevelingSplits[];
  moves: PlanMove[];
  late: number[];
  overallocated: number[];
}

/** Mirror of `import::ImportResource`, one person or machine of `import_rows`. */
export interface ImportResource {
  name: string;
  kind: ResourceKind;
  max_units?: number | null;
  standard_rate?: number | null;
  /** Skill names, each optionally `Skill:Level`. */
  skills?: string[];
  group?: string;
}

/** Mirror of `import::ImportAssignee`: a person by name, letter case aside. */
export interface ImportAssignee {
  name: string;
  units: number;
}

/**
 * Mirror of `import::ImportTask`, one task of `import_rows`. `work` and `duration` are
 * working minutes (a task with `work` is Fixed Work); instants are project wall-clock
 * milliseconds; `values` pairs a field name with its value; `after` lists rows of the same
 * import (0 for the first task) this one waits for.
 */
export interface ImportTask {
  name: string;
  level?: number;
  work?: number | null;
  work_low?: number | null;
  work_high?: number | null;
  duration?: number | null;
  priority?: number | null;
  deadline?: number | null;
  start_no_earlier?: number | null;
  values?: [string, FieldValue][];
  assignees?: ImportAssignee[];
  after?: number[];
}

/** Mirror of `import::ImportColumn` (serde's external tagging). */
export type ImportColumn =
  | "row"
  | "name"
  | "kind"
  | "max_units"
  | "rate"
  | "skills"
  | "group"
  | "level"
  | "work"
  | "work_low"
  | "work_high"
  | "duration"
  | "priority"
  | "deadline"
  | "start_no_earlier"
  | "assignees"
  | "after"
  | { field: string };

/** What a pulled row changes in its task (mirror of `pull::PullField`). */
export type PullField = "name" | "work" | "start" | "due" | "assignee" | "status" | "fields" | "links" | "present";

/** Mirror of `pull::PullRowDiff`: a pulled issue and what pulling it does. */
export interface PullRowDiff {
  row: number;
  key: string;
  name: string;
  uid: number;
  kind: "add" | "change" | "same";
  fields: PullField[];
}

/** Mirror of `pull::GoneTask`: a linked task whose issue the source no longer has. */
export interface GoneTask {
  uid: number;
  key: string;
  name: string;
}

/** The cell of a pulled row a problem is in (mirror of `pull::PullColumn`). */
export type PullColumn = "row" | "key" | "name" | "parent" | "work" | "assignee" | "after" | { field: string };

/** Mirror of `tracker::PullProblem`; `row` counts from 0, null is the pull as a whole. */
export interface PullProblem {
  message: string;
  row: number | null;
  key: string;
  column: PullColumn;
}

/**
 * Mirror of `pull::PullConflict`: a field changed at the source and in Sheafly since the
 * last sync, to different values. Values are canonical text (a day is `YYYY-MM-DD`, effort
 * is minutes, status is `not_started`, `in_progress` or `done`); null is no value.
 */
export interface PullConflict {
  row: number;
  key: string;
  name: string;
  uid: number;
  /** `name`, `work`, `start`, `due`, `status`, `assignee` or `field:<id>`. */
  field: string;
  base: string | null;
  source: string | null;
  local: string | null;
}

/** Mirror of `tracker::PullChanges`: what a pull would do to the project as it is now. */
export interface PullChanges {
  changes: PullRowDiff[];
  gone: GoneTask[];
  problems: PullProblem[];
  /** Fields changed on both sides that the resolutions given do not settle. */
  conflicts: PullConflict[];
  /** A conflict settled by keeping Sheafly changes no task but is worth applying. */
  bases_changed: boolean;
}

/** Mirror of `backlog::PullWarning`. */
export interface PullWarning {
  key: string;
  kind: "no_email" | "skipped" | "field_value";
}

/** Mirror of `tracker::PullPreview`: what `tracker_pull` fetched; send `rows` in `apply_pull`. */
export interface PullPreview extends PullChanges {
  rows: PullTask[];
  pulled_at: number;
  warnings: PullWarning[];
}

/** The `tracker-progress` event: issues fetched so far, and how many there are. */
export interface PullProgress {
  fetched: number;
  total: number | null;
}

/** One problem `check_import` found; `row` counts from 0 within its table, null is the whole import. */
export interface ImportProblem {
  table: "resources" | "tasks";
  row: number | null;
  column: ImportColumn;
  message: string;
}

/** A table read for import (`pick_table`, `read_table_sheet`): every cell as text. */
export interface ImportSheet {
  sheets: string[];
  sheet: string | null;
  rows: string[][];
  /** The file's line or row number (from 1) of each row; blank rows are left out of `rows`. */
  lines: number[];
}

/**
 * Mirror of `availability::AvailabilityPeriod`: `units` on every day from `from` to `to`
 * (both "YYYY-MM-DD", both included); `null` is no limit on that side.
 */
export interface AvailabilityPeriod {
  from: string | null;
  to: string | null;
  units: number;
}

export interface AssignmentBaseline {
  number: number;
  work: number;
  /** Cost when the baseline was set; absent or 0 in baselines saved before it was kept. */
  cost?: number;
}

export interface Assignment {
  uid: number;
  task_uid: number;
  resource_uid: number;
  units: number;
  work: number;
  overtime_work: number;
  actual_work: number;
  remaining_work: number;
  cost: number;
  actual_cost: number;
  /** Hand-entered actual cost; counts only while costs are editable and the task has no entry. */
  manual_actual_cost: number | null;
  /** Saved baselines for this assignment: baseline number and work. */
  baselines?: AssignmentBaseline[];
}

export interface Overallocation {
  resource_uid: number;
  date: string;
  units: number;
  max_units: number;
}

export interface UsageDay {
  date: string;
  hours: number;
}

export interface UsageRow {
  task_uid: number;
  assignment_uid: number | null;
  resource_uid: number | null;
  label: string;
  days: UsageDay[];
}

export interface Project {
  name: string;
  start: number;
  status_date: number | null;
  calendar_id: number;
  calendars: Calendar[];
  tasks: Task[];
  dependencies: Dependency[];
  resources: Resource[];
  assignments: Assignment[];
  /** UIDs the engine gives the next task and resource; the UI names new items with them. */
  next_task_uid: number;
  next_resource_uid: number;
  overallocations: Overallocation[];
  usage: UsageRow[];
  /** Actual costs are entered by hand instead of computed from progress. */
  editable_actual_costs: boolean;
  /** Custom task fields, in the order they were added. */
  fields: FieldDef[];
  /** The order leveling places tasks in; empty for the default (start, priority, slack). */
  leveling_order: SortKey[];
  /** Skills resources can have, in the order they were added. */
  skills: SkillDef[];
  next_skill_id: number;
  /** Skill levels, lowest first; empty when levels are not used. */
  skill_levels?: SkillLevel[];
  next_skill_level_id?: number;
  /** Ordered rules from custom field values to a person or a skill. */
  assignment_rules: AssignmentRule[];
  /** How tasks' effort varies together in the finish forecast. */
  forecast: ForecastSettings;
  /** Share of time at work people give to project work, 0.5–1; it slows their rate of work. */
  focus_factor: number;
  /** The external task tracker the project is pulled from, if any. */
  source: Source | null;
}

/** Mirror of `source::TrackerKind`. */
export type TrackerKind = "backlog" | "jira";

/** Mirror of `source::StatusClass`. */
export type StatusClass = "not_started" | "in_progress" | "done";

/** Mirror of `source::StatusMapping`. */
export interface StatusMapping {
  status: string;
  class: StatusClass;
}

/** Mirror of `source::Direction`. */
export type SyncDirection = "both" | "pull" | "push" | "off";

/** Mirror of `source::OnConflict`: what happens when a field changed on both sides. */
export type OnConflict = "ask" | "source" | "sheafly";

/** Mirror of `source::FieldPolicy`. */
export interface FieldPolicy {
  /** `name`, `work`, `start`, `due`, `status`, `assignee`, `field:<id>` or `fields`. */
  field: string;
  direction: SyncDirection;
  on_conflict: OnConflict;
  /** Which field at the source to read, when not the tracker's usual one. */
  source_field?: string | null;
  /** Which field at the source to write, when not the tracker's usual one. */
  push_field?: string | null;
}

/** Mirror of `source::SyncSettings`. */
export interface SyncSettings {
  fields: FieldPolicy[];
  in_progress_percent: number;
  start_hour: number;
  due_hour: number;
}

export const DEFAULT_SYNC_SETTINGS: SyncSettings = { fields: [], in_progress_percent: 50, start_hour: 8, due_hour: 17 };

/** Mirror of `source::Source`. */
export interface Source {
  kind: TrackerKind;
  site: string;
  project_key: string;
  filter: string;
  /** Working hours one story point stands for. */
  hours_per_point: number;
  status_map: StatusMapping[];
  /** How each field syncs and the constants the sync uses. */
  sync: SyncSettings;
}

/** Mirror of `source::ExternalLink`. */
export interface ExternalLink {
  key: string;
  synced_at: number;
  gone?: boolean;
  /** The days (`YYYY-MM-DD`) last written to the source by a push. */
  pushed_start?: string | null;
  pushed_due?: string | null;
  /** What the source and Sheafly held at the last sync, by field key (`name`, `work`, `start`, `due`, `status`, `assignee`, `field:<id>`). */
  base?: { source: Record<string, string>; local: Record<string, string> } | null;
}

/** Which side a conflict resolution keeps (mirror of `sync::Side`). */
export type Side = "source" | "sheafly";

/** Mirror of `sync::Resolution`: the user's choice for one field of one issue. */
export interface Resolution {
  key: string;
  field: string;
  keep: Side;
}

/** Mirror of `push::PushOptions`: what a push writes. */
export interface PushOptions {
  start: boolean;
  due: boolean;
  assignee: boolean;
  /** Replace values already at the source; off, only empty ones are filled. */
  overwrite: boolean;
}

/** Mirror of `push::DayChange`: a day at the source and the day to write (`YYYY-MM-DD`). */
export interface DayChange {
  from: string | null;
  to: string;
}

/** Mirror of `push::AssigneeChange`, by email. */
export interface AssigneeChange {
  from: string | null;
  to: string;
  resource_uid: number;
  /** The person's name, for a tracker that finds people by name. */
  to_name: string;
}

/** Mirror of `push::PushChange`: what a push writes for one issue. */
export interface PushChange {
  uid: number;
  key: string;
  name: string;
  start: DayChange | null;
  due: DayChange | null;
  assignee: AssigneeChange | null;
  /** The field at the source the start / finish day goes to (`custom:<name>`), when not the usual one. */
  start_field?: string | null;
  due_field?: string | null;
}

export type PushSkipReason = "many_assignees" | "no_email" | "source_changed";

/** Mirror of `push::PushSkip`. */
export interface PushSkip {
  uid: number;
  key: string;
  name: string;
  reason: PushSkipReason;
  /** The field a `source_changed` skip is about (`start`, `due` or `assignee`). */
  field?: string | null;
}

/** Mirror of `push::MissingTask`: a linked task whose issue the source did not return. */
export interface MissingTask {
  uid: number;
  key: string;
  name: string;
}

/** Mirror of `push::PushPlan`. */
export interface PushPlan {
  changes: PushChange[];
  skipped: PushSkip[];
  missing: MissingTask[];
}

/** Mirror of `PushPreview` in the shell: the plan and the people to assign that the source lacks. */
export interface PushPreview {
  plan: PushPlan;
  unknown_assignees: string[];
}

/** Mirror of `backlog_push::PushReport`: what a push did. */
export interface PushReport {
  done: string[];
  /** The days written, for `record_push`. */
  entries: PushedEntry[];
  failed: { key: string; message: string }[];
  not_sent: string[];
  stopped: string | null;
}

/** The `tracker-push-progress` event. */
export interface PushProgress {
  done: number;
  total: number;
}

/** Mirror of `push::PushedEntry`: the days a push wrote for one issue. */
export interface PushedEntry {
  key: string;
  start?: string | null;
  due?: string | null;
  /** The email written as the assignee. */
  assignee?: string | null;
  /** The days written to a field of their own (`planned_start`, `planned_finish`). */
  planned_start?: string | null;
  planned_finish?: string | null;
}

/** Mirror of `pull::PullTask`, one issue of `apply_pull` as the adapter read it. */
export interface PullTask {
  key: string;
  parent_key?: string | null;
  name: string;
  /** Effort in working minutes. */
  work?: number | null;
  start?: number | null;
  due?: number | null;
  status?: StatusClass | null;
  assignee_email?: string | null;
  /** Their name, for a tracker that hides emails; used when there is no email. */
  assignee_name?: string | null;
  values?: [string, FieldValue][];
  after?: string[];
  /** What the field a push writes the computed start / finish to holds now (an instant). */
  planned_start?: number | null;
  planned_finish?: number | null;
}

/** Mirror of `model::Strength`: a share of 0, 0.3, 0.5 or 0.8 in common. */
export type Strength = "none" | "low" | "medium" | "high";

/** Mirror of `model::ForecastSettings`. */
export interface ForecastSettings {
  /** A text or choice custom field; tasks with the same value vary together by `group`. */
  field: number | null;
  group: Strength;
  common: Strength;
}

export interface Snapshot {
  project: Project;
  path: string | null;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  /** Commands applied and not undone; a dialog records it to roll back to on Hủy. */
  history_cursor: number;
}

export type Command =
  | { type: "rename_project"; name: string }
  | { type: "set_project_start"; start: number }
  | { type: "set_status_date"; date: number | null }
  | { type: "add_task"; name: string; after_uid: number | null }
  | { type: "add_milestone"; name: string; after_uid: number | null }
  | { type: "delete_task"; uid: number }
  | { type: "rename_task"; uid: number; name: string }
  | { type: "set_duration"; uid: number; minutes: number }
  | { type: "set_manual"; uid: number; manual: boolean; start: number | null; finish: number | null }
  | { type: "set_constraint"; uid: number; constraint_type: ConstraintType; date: number | null }
  | { type: "indent"; uid: number }
  | { type: "outdent"; uid: number }
  | { type: "set_predecessors"; uid: number; text: string }
  | { type: "add_dependency"; predecessor_uid: number; successor_uid: number; kind: DepKind; lag: number }
  | { type: "remove_dependency"; uid: number }
  | { type: "set_notes"; uid: number; notes: string }
  | { type: "set_priority"; uid: number; priority: number }
  | { type: "set_task_calendar"; uid: number; calendar_id: number | null }
  | { type: "add_holiday"; date: string }
  | { type: "remove_holiday"; date: string }
  | { type: "create_calendar"; name: string; base_id: number | null }
  | { type: "set_calendar_base"; calendar_id: number; base_id: number | null }
  | { type: "set_project_calendar"; calendar_id: number }
  | { type: "rename_calendar"; calendar_id: number; name: string }
  | { type: "set_calendar_week"; calendar_id: number; week: TimeRange[][] }
  | { type: "set_calendar_work_week"; calendar_id: number; name: string; from: string; to: string; week: TimeRange[][] }
  | { type: "clear_calendar_work_week"; calendar_id: number; from: string }
  | {
      type: "set_calendar_exceptions";
      calendar_id: number;
      from: string;
      to: string;
      ranges: TimeRange[];
    }
  | { type: "clear_calendar_exceptions"; calendar_id: number; from: string; to: string }
  | { type: "move_task"; uid: number; start: number }
  | { type: "resize_task"; uid: number; finish: number }
  | { type: "add_resource"; name: string; kind: ResourceKind }
  | {
      type: "update_resource";
      uid: number;
      name: string;
      kind: ResourceKind;
      max_units: number;
      calendar_id: number | null;
      standard_rate: number;
      overtime_rate: number;
      cost_per_use: number;
    }
  | { type: "delete_resource"; uid: number }
  | { type: "assign_resource"; task_uid: number; resource_uid: number; units: number }
  | { type: "update_assignment"; uid: number; units: number | null; work: number | null; overtime_work: number | null }
  | { type: "remove_assignment"; uid: number }
  | { type: "set_task_type"; uid: number; task_type: TaskType; effort_driven: boolean }
  | { type: "set_work"; uid: number; minutes: number }
  | { type: "set_percent_complete"; uid: number; percent: number }
  | { type: "set_actual_start"; uid: number; start: number | null }
  | { type: "set_actual_finish"; uid: number; finish: number | null }
  | { type: "set_remaining_duration"; uid: number; minutes: number }
  | { type: "set_remaining_work"; uid: number; minutes: number }
  | { type: "set_overtime_work"; uid: number; minutes: number }
  | { type: "set_fixed_cost"; uid: number; cost: number; accrual: Accrual }
  | { type: "set_baseline"; number: number }
  | { type: "clear_baseline"; number: number }
  | { type: "level_resources" }
  | {
      type: "apply_leveling";
      delays: LevelingDelay[];
      /** Tasks whose leveling splits changed, each with all of them. */
      splits?: LevelingSplits[];
    }
  | { type: "clear_leveling" }
  | { type: "reschedule_uncompleted" }
  | { type: "set_leveling_delay"; uid: number; minutes: number }
  | { type: "set_interrupts"; uid: number; interrupts: boolean }
  | { type: "set_work_range"; uid: number; low: number | null; high: number | null }
  | { type: "set_forecast_settings"; settings: ForecastSettings }
  | { type: "set_deadline"; uid: number; date: number | null }
  | { type: "set_editable_actual_costs"; editable: boolean }
  | { type: "set_task_actual_cost"; uid: number; cost: number | null }
  | { type: "set_assignment_actual_cost"; uid: number; cost: number | null }
  | { type: "create_recurring_task"; after_uid: number | null; spec: RecurrenceSpec }
  | { type: "update_recurring_task"; uid: number; spec: RecurrenceSpec }
  | { type: "split_task"; uid: number; at: number; gap: number }
  | { type: "remove_split"; uid: number; index: number | null }
  | { type: "move_segment"; uid: number; index: number; start: number }
  | { type: "paste_tasks"; after_uid: number | null; clipboard: TaskClipboard }
  | { type: "fill_down"; from_uid: number; to_uids: number[]; field: FillField }
  | { type: "add_field"; name: string; kind: FieldKind }
  | {
      type: "update_field";
      id: number;
      name: string;
      kind: FieldKind;
      /** Choices renamed in place, so tasks keep them. */
      renamed_options: { from: string; to: string }[];
    }
  | { type: "delete_field"; id: number }
  | { type: "set_field_value"; uid: number; field_id: number; value: FieldValue | null }
  | { type: "set_leveling_order"; order: SortKey[] }
  | { type: "set_resource_availability"; uid: number; periods: AvailabilityPeriod[] }
  | { type: "add_skill"; name: string }
  | { type: "rename_skill"; id: number; name: string }
  | { type: "delete_skill"; id: number }
  | { type: "set_resource_skills"; uid: number; skills: number[]; levels?: SkillLevelOf[] }
  | { type: "set_skill_levels"; levels: SkillLevelInput[] }
  | { type: "set_resource_group"; uid: number; group: string }
  | { type: "set_focus_factor"; factor: number }
  | { type: "set_resource_focus"; uid: number; focus: number | null }
  | { type: "set_assignment_rules"; rules: AssignmentRule[] }
  | { type: "assign_tasks"; assignments: TaskAssignment[] }
  | { type: "set_resource_email"; uid: number; email: string }
  | { type: "set_source"; source: Source | null }
  | { type: "apply_pull"; rows: PullTask[]; pulled_at: number; resolutions?: Resolution[] }
  | { type: "record_push"; entries: PushedEntry[] }
  | { type: "import_rows"; after_uid: number | null; resources: ImportResource[]; tasks: ImportTask[] }
  | { type: "apply_plan"; assignments: TaskAssignment[]; delays: LevelingDelay[]; splits: LevelingSplits[] };

/** Mirror of `clipboard::TaskCopy`: one copied task, its level relative to the copy. */
export interface TaskCopy {
  name: string;
  level: number;
  milestone: boolean;
  manual: boolean;
  duration: number;
  work: number;
  start: number;
  finish: number;
  notes: string;
  priority: number;
  calendar_id: number | null;
  /** Matches the calendar by name when pasting into another project. */
  calendar_name: string | null;
  constraint_type: ConstraintType;
  constraint_date: number | null;
  deadline: number | null;
  task_type: TaskType;
  effort_driven: boolean;
  fixed_cost: number;
  fixed_cost_accrual: Accrual;
  splits: Split[];
  assignments: { resource_uid: number; resource_name: string; units: number; work: number; focus?: number }[];
  /** The project's productivity factor where the copy was made. */
  focus?: number;
  /** Custom field values by field name, as `[name, value]` pairs. */
  values: [string, FieldValue][];
}

/** Mirror of `clipboard::TaskClipboard`; links point at positions in `tasks`. */
export interface TaskClipboard {
  tasks: TaskCopy[];
  links: { from: number; to: number; kind: DepKind; lag: number }[];
}

/** Mirror of `clipboard::FillField`: the columns fill-down copies; `{ field }` is a custom field. */
export type FillField =
  | "name"
  | "duration"
  | "start"
  | "predecessors"
  | "percent_complete"
  | "calendar"
  | "interrupts"
  | "work_range"
  | { field: number };

export type ViewId = "gantt" | "tracking" | "resources" | "matrix" | "usage" | "load" | "curves" | "forecast" | "network";

export interface MatrixCell {
  assignment_uid?: number;
  units: number;
  work: number;
}

export interface MatrixColumn {
  id: string;
  label: string;
  group: string;
  resource_uid?: number;
  total_work: number;
}

export interface MatrixRole {
  name: string;
  resource_uids: number[];
  total_work: number;
}

export interface UnassignedTarget {
  rule_index: number;
  target: RuleTarget;
}

export interface MatrixRow {
  task_uid: number;
  task_id: number;
  task_name: string;
  outline_level: number;
  summary: boolean;
  cells: (MatrixCell | null)[];
  role_cells: (MatrixCell | null)[];
  unassigned?: UnassignedTarget;
  total_work: number;
}

export interface AssignmentMatrix {
  columns: MatrixColumn[];
  roles: MatrixRole[];
  rows: MatrixRow[];
  total_work: number;
}

export interface NetworkOptions {
  include_summary: boolean;
  visible_uids: number[] | null;
}

// Network diagram geometry comes from the engine in pixels at zoom 1.
export interface NetworkNode {
  uid: number;
  column: number;
  row: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface NetworkPoint {
  x: number;
  y: number;
}

export interface NetworkEdge {
  /** The link drawn; a link to a hidden summary can be drawn as several edges. */
  uid: number;
  /** Boxes joined: a task inside a hidden summary, or a recurring series' box. */
  from_uid: number;
  to_uid: number;
  kind: DepKind;
  lag: number;
  points: NetworkPoint[];
}

export interface NetworkLayout {
  nodes: NetworkNode[];
  edges: NetworkEdge[];
  columns: number;
  width: number;
  height: number;
  crossings: number;
}

// EVM values come from the engine; null means undefined because a denominator was not above zero.
export interface EvmMetrics {
  bac: number;
  pv: number;
  ev: number;
  ac: number;
  sv: number;
  cv: number;
  spi: number | null;
  cpi: number | null;
  eac: number | null;
  etc: number | null;
  vac: number | null;
}

export interface EvmReport {
  status_date: number | null;
  has_baseline: boolean;
  metrics: EvmMetrics | null;
}

export interface BurndownPoint {
  date: number;
  remaining_work: number;
}

export interface BurndownSnapshot {
  date: number;
  remaining_work: number;
  ev: number;
  ac: number;
}

export interface BurndownReport {
  status_date: number | null;
  baseline_start: number | null;
  baseline_finish: number | null;
  baseline_work: number;
  ideal: BurndownPoint[];
  /** Baseline 0 work still planned over time; flat across split gaps. */
  planned: BurndownPoint[];
  actual: BurndownSnapshot[];
  current_remaining_work: number;
}

/** Cumulative values at one instant; EV and AC are null after the status date. */
export interface SCurvePoint {
  date: number;
  pv: number;
  ev: number | null;
  ac: number | null;
}

export interface SCurveReport {
  status_date: number | null;
  baseline_finish: number | null;
  step: "day" | "week";
  points: SCurvePoint[];
}

export interface BaselineStatistics {
  start: number;
  finish: number;
  duration: number;
  work: number;
  cost: number;
}

/** Totals for the Thống kê tab of Thông tin dự án, computed by the engine. */
export interface ProjectStatistics {
  start: number;
  finish: number;
  duration: number;
  work: number;
  actual_work: number;
  remaining_work: number;
  cost: number;
  actual_cost: number;
  remaining_cost: number;
  percent_complete: number;
  baseline: BaselineStatistics | null;
}

export interface ProjectMetrics {
  evm: EvmReport;
  burndown: BurndownReport;
  s_curve: SCurveReport;
  statistics: ProjectStatistics;
}
export type Scale = "day" | "week" | "month";

/** The plan of a Sheafly Pro license (`sheafly_license::Plan`). */
export type LicensePlan = "pro" | "business" | "trial";

/** Why a key does not unlock Pro on this build (`Problem::code`, or `not_saved`). */
export type LicenseProblem =
  | "malformed"
  | "newer_format"
  | "unknown_key"
  | "bad_signature"
  | "revoked"
  | "expired"
  | "updates_ended"
  | "not_saved";

/** AI recommendations, as `ai_settings` reports them (`settings::AiSettings`). */
export interface AiSettings {
  enabled: boolean;
  /** A Claude model ID, e.g. "claude-opus-5-5". */
  model: string;
  /** Send "Task 42" and "Person 7" instead of names. */
  mask: boolean;
  notice_accepted: boolean;
}

/** What `ai_test` found (`ai::AiCheck`). */
export interface AiCheck {
  model: string;
  input_tokens: number;
  output_tokens: number;
}

/** How serious a risk card is (`advice::Severity`). */
export type RiskSeverity = "high" | "medium" | "low";

/** One risk the AI found (`advice::RiskCard`); tasks are `T42` and people `R7`, in the text too. */
export interface RiskCard {
  severity: RiskSeverity;
  title: string;
  what: string;
  why: string;
  /** The tasks driving the risk, earliest first. */
  chain: string[];
  focus: string;
  tasks: string[];
  people: string[];
}

/** What `ai_risks` answers (`ai_commands::RiskReport`). */
export interface RiskReport {
  cards: RiskCard[];
  input_tokens: number;
  output_tokens: number;
  requests: number;
  /** The summary sent was cut to its size limit. */
  truncated: boolean;
  omitted_tasks: number;
  omitted_resources: number;
}

/** The license of this computer, as `license_status` reports it (`license::LicenseView`). */
export interface LicenseView {
  /** `none`: Free, no key; `active`: Pro on this build; `inactive`: a key that does not work here. */
  state: "none" | "active" | "inactive";
  plan: LicensePlan | null;
  name: string | null;
  email: string | null;
  seats: number | null;
  /** Releases made up to this day ("2027-10-24") are covered. */
  updates_until: string | null;
  /** The last day of a trial. */
  expires: string | null;
  problem: LicenseProblem | null;
  /** The day the problem is about: the end of a trial or of updates. */
  problem_day: string | null;
  /** The day this build was made. */
  build_date: string;
  /** Feature codes unlocked on this build, e.g. "trackers". */
  features: string[];
}

/** The schedule health check, DCMA's 14 points (`sheafly_engine::health::HealthReport`). */
export interface HealthReport {
  /** The 14 criteria in order, criterion `n` at index `n - 1`. */
  criteria: HealthCriterion[];
  /** Criteria passed out of those that apply; criterion 10 counts in neither. */
  passed: number;
  applicable: number;
  /** Sheafly's rating (DCMA has no overall score); null when nothing applies. */
  rating: HealthRating | null;
}

export type HealthRating = "good" | "acceptable" | "at_risk";

export type HealthStatus = "pass" | "fail" | "info" | "not_applicable";

export type HealthLimit = { kind: "at_most"; value: number } | { kind: "at_least"; value: number };

export type HealthNotApplicable =
  | "no_tasks"
  | "no_links"
  | "no_baseline"
  | "no_status_date"
  | "no_resources"
  | "nothing_due"
  | "no_critical_task"
  | "no_target"
  | "no_path_left";

export type HealthReason =
  | "no_predecessor"
  | "no_successor"
  | "no_links"
  | "lead"
  | "lag"
  | "not_finish_to_start"
  | "hard_constraint"
  | "manual"
  | "high_slack"
  | "negative_slack"
  | "past_deadline"
  | "long_duration"
  | "actual_after_status"
  | "planned_before_status"
  | "no_resource"
  | "not_finished"
  | "finished_late"
  | "tested";

export interface HealthCriterion {
  /** 1 Logic … 14 BEI, as DCMA numbers them. */
  number: number;
  status: HealthStatus;
  not_applicable: HealthNotApplicable | null;
  /** What fails it (tasks, or links for 2–4) out of `total`; for 14 the tasks finished and due. */
  count: number;
  total: number;
  /** A share for 1–11 (for 4 the share of finish-to-start links), the index for 13 and 14. */
  value: number | null;
  limit: HealthLimit | null;
  /** The tasks to look at, in project order. */
  tasks: { uid: number; reason: HealthReason }[];
}
