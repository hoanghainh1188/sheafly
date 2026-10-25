import type {
  AiCheck,
  AiSettings,
  RiskReport,
  FixReport,
  Proposal,
  ProposalTrial,
  AssignmentMatrix,
  Command,
  CurvesReport,
  FieldValue,
  ImportProblem,
  ImportSheet,
  LicenseView,
  LoadMap,
  PlanPreview,
  NetworkLayout,
  ProjectMetrics,
  Snapshot,
  Suggestion,
  Task,
  TaskAssignment,
  TaskClipboard,
  Forecast,
  HealthReport,
  PullChanges,
  PullPreview,
  PushPreview,
  PushReport,
} from "../types";
import { makeHealthReport, makeLicense, makeMetrics, makeProLicense, makeResource, makeSnapshot, makeTask } from "./fixtures";

export interface InvokeCall {
  cmd: string;
  args: Record<string, unknown> | undefined;
}

/**
 * In-memory stand-in for the Tauri commands in src/api.ts. It records every call and
 * applies a handful of commands so component flows can round-trip without the engine.
 */
class FakeBackend {
  calls: InvokeCall[] = [];
  snapshot: Snapshot = makeSnapshot();
  metrics: ProjectMetrics = makeMetrics();
  network: NetworkLayout = emptyNetwork();
  /** What `suggest_assignments` answers: made up by the test, the engine ranks people. */
  suggestions: Suggestion[] = [];
  /** What `preview_plan` answers; by default the assignments asked for and no leveling. */
  planPreview: PlanPreview | null = null;
  /** What `forecast` answers (null is a run stopped by a newer one). */
  forecast: Forecast | null = null;
  /** What `schedule_health` answers. */
  health: HealthReport = makeHealthReport();
  /** What `load_map` answers. */
  loadMap: LoadMap = { buckets: [], rows: [] };
  /** What `assignment_matrix` answers. */
  assignmentMatrix: AssignmentMatrix = { columns: [], roles: [], rows: [], total_work: 0 };
  /** What `curves` answers. */
  curvesReport: CurvesReport = {
    measure: "cost",
    bucket: "week",
    filter: { type: "project" },
    baseline: null,
    status_date: null,
    baseline_finish: null,
    scheduled_finish: null,
    total_baseline: 0,
    total_scheduled: 0,
    total_actual: 0,
    total_earned: 0,
    points: [],
  };
  /** What `check_import` answers. */
  importProblems: ImportProblem[] = [];
  /** What `tracker_pull` answers (null is a cancelled pull), and `tracker_test` calls the project. */
  pullPreview: PullPreview | null = null;
  /** What `tracker_check` answers; by default the preview's own changes. */
  pullChanges: PullChanges | null = null;
  /** What `tracker_push_preview` answers (null is a cancelled one) and `tracker_push` reports. */
  pushPreview: PushPreview | null = null;
  pushReport: PushReport = { done: [], entries: [], failed: [], not_sent: [], stopped: null };
  /** Sites with a saved API key (`tracker_has_key`). */
  trackerKeys = new Set<string>();
  /** The AI settings (`ai_settings`), whether an Anthropic key is saved, and what `ai_test` answers. */
  ai: AiSettings = defaultAiSettings();
  aiKey = false;
  aiCheck: AiCheck = { model: "claude-opus-5-5", input_tokens: 12, output_tokens: 3 };
  /** What `ai_risks` answers (null is a stopped request). */
  aiRisks: RiskReport | null = emptyRiskReport();
  /** What `ai_fixes` answers (null is a stopped request). */
  aiFixes: FixReport | null = emptyFixReport();
  /** What `ai_trial` answers by proposal id: a trial, or the engine's refusal as text. By
   * default an improvement whose commands are the proposal's. */
  aiTrials: Record<string, ProposalTrial | string> = {};
  /** What `pick_table` answers (null is a cancelled dialog); `read_table_sheet` reads it too. */
  table: ImportSheet | null = null;
  /** What `check_autosave` restores; null when there is no autosave. */
  autosave: Snapshot | null = null;
  /** The remembered interface language (`get_language`); null follows the system. */
  language: string | null = null;
  /** The remembered theme (`get_theme`). */
  theme = "system";
  /** This computer's license (`license_status`). */
  license: LicenseView = makeLicense();
  /** What `license_install` refuses with; null accepts any key as an active Pro license. */
  licenseRefusal: LicenseView | null = null;
  private failures = new Map<string, unknown>();
  private holds = new Map<string, Promise<void>>();

  reset(snapshot: Snapshot = makeSnapshot(), metrics: ProjectMetrics = makeMetrics()): void {
    this.calls = [];
    this.snapshot = structuredClone(snapshot);
    this.metrics = structuredClone(metrics);
    this.network = emptyNetwork();
    this.suggestions = [];
    this.importProblems = [];
    this.planPreview = null;
    this.loadMap = { buckets: [], rows: [] };
    this.assignmentMatrix = { columns: [], roles: [], rows: [], total_work: 0 };
    this.curvesReport = {
      measure: "cost",
      bucket: "week",
      filter: { type: "project" },
      baseline: null,
      status_date: null,
      baseline_finish: null,
      scheduled_finish: null,
      total_baseline: 0,
      total_scheduled: 0,
      total_actual: 0,
      total_earned: 0,
      points: [],
    };
    this.table = null;
    this.pullPreview = null;
    this.pullChanges = null;
    this.pushPreview = null;
    this.pushReport = { done: [], entries: [], failed: [], not_sent: [], stopped: null };
    this.trackerKeys.clear();
    this.ai = defaultAiSettings();
    this.aiKey = false;
    this.aiCheck = { model: "claude-opus-5-5", input_tokens: 12, output_tokens: 3 };
    this.aiRisks = emptyRiskReport();
    this.aiFixes = emptyFixReport();
    this.aiTrials = {};
    this.failures.clear();
    this.holds.clear();
    this.autosave = null;
    this.language = null;
    this.theme = "system";
    this.license = makeLicense();
    this.licenseRefusal = null;
    this.health = makeHealthReport();
  }

  /** The next call to `cmd` waits until the returned function runs, like a slow Rust command. */
  holdNext(cmd: string): () => void {
    let release = (): void => {};
    this.holds.set(cmd, new Promise<void>((resolve) => (release = resolve)));
    return () => release();
  }

  /** The next call to `cmd` rejects with `error`, like a Rust `Err(String)`. */
  failNext(cmd: string, error: unknown): void {
    this.failures.set(cmd, error);
  }

  commands(): Command[] {
    return this.calls
      .filter((call) => call.cmd === "apply_command")
      .map((call) => call.args?.command as Command);
  }

  async invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
    this.calls.push({ cmd, args });
    const hold = this.holds.get(cmd);
    if (hold) {
      this.holds.delete(cmd);
      await hold;
    }
    if (this.failures.has(cmd)) {
      const error = this.failures.get(cmd);
      this.failures.delete(cmd);
      throw error;
    }
    // Window plugin calls such as setting the title.
    if (cmd.startsWith("plugin:")) return undefined;
    switch (cmd) {
      case "open_pending":
        // Nothing queued, as when the app is opened normally.
        return null;
      case "open_recent":
      case "project_state":
      case "new_project":
      case "open_project":
      case "save_project":
      case "save_project_as":
      case "import_mspdi":
      case "compact_history":
      case "undo":
      case "redo":
        return structuredClone(this.snapshot);
      case "project_metrics":
        return structuredClone(this.metrics);
      case "network_layout":
        return structuredClone(this.network);
      case "suggest_assignments":
        return structuredClone(this.suggestions);
      case "load_map":
        return structuredClone(this.loadMap);
      case "assignment_matrix":
        return structuredClone(this.assignmentMatrix);
      case "curves":
        return structuredClone(this.curvesReport);
      case "forecast":
        return structuredClone(this.forecast);
      case "schedule_health":
        return structuredClone(this.health);
      case "preview_plan":
        return structuredClone(
          this.planPreview ?? {
            assignments: args?.assignments as TaskAssignment[],
            delays: [],
            splits: [],
            moves: [],
            late: [],
            overallocated: [],
          },
        );
      case "apply_plan": {
        const assignments = args?.assignments as TaskAssignment[];
        if (assignments.length > 0) this.apply({ type: "assign_tasks", assignments });
        return structuredClone(this.snapshot);
      }
      case "check_import":
        return structuredClone(this.importProblems);
      case "license_status":
        return structuredClone(this.license);
      case "license_install":
        if (this.licenseRefusal) throw structuredClone(this.licenseRefusal);
        this.license = makeProLicense();
        return structuredClone(this.license);
      case "license_remove":
        this.license = makeLicense();
        return structuredClone(this.license);
      case "ai_settings":
        return structuredClone(this.ai);
      case "ai_set_settings": {
        const settings = args?.settings as AiSettings;
        if (settings.enabled && !settings.notice_accepted) throw "Hãy đọc và đồng ý thông báo về dữ liệu trước khi bật gợi ý AI";
        this.ai = structuredClone(settings);
        return structuredClone(this.ai);
      }
      case "ai_has_key":
        return this.aiKey;
      case "ai_save_key":
        this.aiKey = true;
        return undefined;
      case "ai_forget_key":
        this.aiKey = false;
        return undefined;
      case "ai_test":
        return structuredClone(this.aiCheck);
      case "ai_risks":
        return structuredClone(this.aiRisks);
      case "ai_fixes":
        return structuredClone(this.aiFixes);
      case "ai_trial": {
        const proposal = args?.proposal as Proposal;
        const answer = this.aiTrials[proposal.id] ?? improvedTrial(proposal);
        if (typeof answer === "string") throw answer;
        return structuredClone(answer);
      }
      case "ai_apply":
        this.apply({ type: "apply_proposal", commands: args?.commands as Command[] });
        return structuredClone(this.snapshot);
      case "ai_cancel":
        return undefined;
      case "tracker_has_key":
        return this.trackerKeys.has(args?.site as string);
      case "tracker_save_key":
        this.trackerKeys.add(args?.site as string);
        return undefined;
      case "tracker_forget_key":
        this.trackerKeys.delete(args?.site as string);
        return undefined;
      case "tracker_test":
        return "Dự án mẫu";
      case "tracker_pull":
        return structuredClone(this.pullPreview);
      case "tracker_check": {
        const base = this.pullPreview;
        return structuredClone(
          this.pullChanges ?? {
            changes: base?.changes ?? [],
            gone: base?.gone ?? [],
            problems: base?.problems ?? [],
            conflicts: base?.conflicts ?? [],
            bases_changed: base?.bases_changed ?? false,
          },
        );
      }
      case "tracker_push_preview":
        return structuredClone(this.pushPreview);
      case "tracker_push":
        return structuredClone(this.pushReport);
      case "tracker_cancel":
        return undefined;
      case "pick_table":
        return structuredClone(this.table);
      case "read_table_sheet":
        if (!this.table?.sheets.includes(args?.sheet as string)) throw "Không có trang này trong tệp";
        return { ...structuredClone(this.table), sheet: args?.sheet as string };
      case "export_mspdi":
      case "save_pdf":
        return undefined;
      case "check_autosave":
        if (this.autosave) this.snapshot = structuredClone(this.autosave);
        return this.autosave ? structuredClone(this.autosave) : null;
      case "get_theme":
        return this.theme;
      case "set_theme":
        this.theme = args?.theme as string;
        return null;
      case "get_language":
        return this.language;
      case "set_language":
        if (args?.remember) this.language = args.language as string;
        return undefined;
      case "copy_tasks":
        return this.copy(args?.uids as number[]);
      case "apply_command":
        this.apply(args?.command as Command);
        return structuredClone(this.snapshot);
      default:
        throw `Fake backend: lệnh ${cmd} chưa hỗ trợ`;
    }
  }

  /** A clipboard with the names and durations of `uids`, in sheet order. */
  private copy(uids: number[]): TaskClipboard {
    const tasks = this.snapshot.project.tasks.filter((task: Task) => uids.includes(task.uid));
    return {
      tasks: tasks.map((task) => ({
        name: task.name,
        level: 0,
        milestone: task.milestone,
        manual: task.manual,
        duration: task.duration,
        work: task.work,
        start: task.start,
        finish: task.finish,
        notes: task.notes,
        priority: task.priority,
        calendar_id: task.calendar_id,
        calendar_name: null,
        constraint_type: task.constraint_type,
        constraint_date: task.constraint_date,
        deadline: task.deadline,
        task_type: task.task_type,
        effort_driven: task.effort_driven,
        fixed_cost: task.fixed_cost,
        fixed_cost_accrual: task.fixed_cost_accrual,
        splits: task.splits,
        assignments: [],
        values: this.snapshot.project.fields.flatMap((field): [string, FieldValue][] => {
          const value = task.values[field.id];
          return value === undefined ? [] : [[field.name, value]];
        }),
      })),
      links: [],
    };
  }

  private apply(command: Command): void {
    const project = this.snapshot.project;
    this.snapshot.dirty = true;
    this.snapshot.can_undo = true;
    switch (command.type) {
      case "rename_project":
        project.name = command.name;
        return;
      case "set_source":
        project.source = command.source;
        return;
      case "set_resource_email": {
        const resource = project.resources.find((item) => item.uid === command.uid);
        if (!resource) throw `Không có nguồn lực ${command.uid}`;
        resource.email = command.email;
        return;
      }
      case "record_push":
        for (const entry of command.entries) {
          const task = project.tasks.find((item) => item.external?.key === entry.key);
          if (task?.external) task.external = { ...task.external, pushed_start: entry.start ?? task.external.pushed_start, pushed_due: entry.due ?? task.external.pushed_due };
        }
        return;
      case "apply_pull":
        // Only the count of tasks: the engine's mapping is tested in Rust.
        for (const row of command.rows) {
          project.tasks.push(makeTask({ uid: Math.max(0, ...project.tasks.map((item) => item.uid)) + 1, name: row.name }));
        }
        return;
      case "set_status_date":
        project.status_date = command.date;
        return;
      case "rename_task": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task) throw `Không có công việc ${command.uid}`;
        task.name = command.name;
        return;
      }
      case "create_recurring_task": {
        const uid = Math.max(0, ...project.tasks.map((task) => task.uid)) + 1;
        project.tasks.push(
          makeTask({
            uid,
            name: command.spec.name,
            summary: true,
            start: command.spec.start,
            recurrence: { spec: command.spec, skipped: [] },
          }),
        );
        return;
      }
      case "update_recurring_task": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task?.recurrence) throw `Không có chuỗi lặp ${command.uid}`;
        task.name = command.spec.name;
        task.recurrence = { ...task.recurrence, spec: command.spec };
        return;
      }
      case "paste_tasks": {
        // Only names, durations and order: enough for the UI flows; the engine does the rest.
        const at = command.after_uid == null ? project.tasks.length : project.tasks.findIndex((item) => item.uid === command.after_uid) + 1;
        let uid = Math.max(0, ...project.tasks.map((item) => item.uid));
        const pasted = command.clipboard.tasks.map((copy) => makeTask({ uid: ++uid, name: copy.name, duration: copy.duration }));
        project.tasks.splice(at, 0, ...pasted);
        return;
      }
      case "fill_down": {
        const source = project.tasks.find((item) => item.uid === command.from_uid);
        if (!source) throw `Không có công việc ${command.from_uid}`;
        for (const task of project.tasks.filter((item) => command.to_uids.includes(item.uid))) {
          if (command.field === "name") task.name = source.name;
          if (command.field === "duration") task.duration = source.duration;
          const flagged = !task.summary && !task.milestone && !source.summary && !source.milestone;
          if (command.field === "interrupts" && flagged) task.interrupts = source.interrupts;
          if (command.field === "work_range" && flagged) {
            task.work_low = source.work_low ?? null;
            task.work_high = source.work_high ?? null;
          }
          if (typeof command.field === "object") setValue(task, command.field.field, source.values[command.field.field] ?? null);
        }
        return;
      }
      case "add_field": {
        if (project.fields.some((field) => field.name.toLowerCase() === command.name.trim().toLowerCase())) throw "Đã có trường cùng tên";
        const id = Math.max(0, ...project.fields.map((field) => field.id)) + 1;
        project.fields.push({ id, name: command.name.trim(), kind: command.kind });
        return;
      }
      case "update_field": {
        // Renames and replaces the kind; the engine also converts or clears values.
        const field = project.fields.find((item) => item.id === command.id);
        if (!field) throw "Không có trường này";
        field.name = command.name.trim();
        field.kind = command.kind;
        return;
      }
      case "delete_field": {
        project.fields = project.fields.filter((field) => field.id !== command.id);
        project.assignment_rules = project.assignment_rules.filter((rule) =>
          rule.conditions.every((condition) => condition.field !== command.id),
        );
        project.leveling_order = project.leveling_order.filter(
          (key) => !(key.criterion.type === "field" && key.criterion.id === command.id),
        );
        for (const task of project.tasks) setValue(task, command.id, null);
        return;
      }
      case "set_field_value": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task) throw `Không có công việc ${command.uid}`;
        if (!project.fields.some((field) => field.id === command.field_id)) throw "Không có trường này";
        setValue(task, command.field_id, command.value);
        return;
      }
      case "set_leveling_order": {
        // The engine's checks: fields that exist, no criterion twice, at most 8.
        const seen = new Set<string>();
        for (const key of command.order) {
          const c = key.criterion;
          if (c.type === "field" && !project.fields.some((field) => field.id === c.id)) throw "Không có trường này";
          const name = c.type === "field" ? `field:${c.id}` : c.type;
          if (seen.has(name)) throw "Mỗi tiêu chí chỉ có một lần trong thứ tự cân bằng";
          seen.add(name);
        }
        if (command.order.length > 8) throw "Thứ tự cân bằng có tối đa 8 tiêu chí";
        project.leveling_order = structuredClone(command.order);
        return;
      }
      case "set_resource_availability": {
        const resource = project.resources.find((item) => item.uid === command.uid);
        if (!resource) throw "Không tìm thấy nguồn lực";
        // The engine's checks, roughly: sorted by start (open first), no shared days.
        const start = (key: string | null) => key ?? "";
        const periods = [...command.periods].sort((a, b) => start(a.from).localeCompare(start(b.from)));
        for (const period of periods) {
          if (!(period.units >= 0)) throw "Đơn vị khả dụng phải là một số không âm";
          if (period.from && period.to && period.from > period.to) throw "Khoảng khả dụng phải kết thúc vào hoặc sau ngày bắt đầu";
        }
        for (let i = 1; i < periods.length; i += 1) {
          const [before, after] = [periods[i - 1], periods[i]];
          if (before.to === null || after.from === null || after.from <= before.to) {
            throw "Các khoảng khả dụng của một nguồn lực không được trùng ngày";
          }
        }
        resource.availability = periods;
        return;
      }
      case "add_skill": {
        const name = command.name.trim();
        if (!name || name.includes(",")) throw "Tên kỹ năng phải có từ 1 đến 50 ký tự và không có dấu phẩy";
        if (project.skills.some((skill) => skill.name.toLowerCase() === name.toLowerCase())) throw "Đã có kỹ năng cùng tên";
        const id = Math.max(project.next_skill_id, ...project.skills.map((skill) => skill.id + 1));
        project.skills.push({ id, name });
        project.next_skill_id = id + 1;
        return;
      }
      case "rename_skill": {
        const skill = project.skills.find((item) => item.id === command.id);
        if (!skill) throw "Không có kỹ năng này";
        const name = command.name.trim();
        if (!name || name.includes(",")) throw "Tên kỹ năng phải có từ 1 đến 50 ký tự và không có dấu phẩy";
        if (project.skills.some((item) => item.id !== skill.id && item.name.toLowerCase() === name.toLowerCase())) {
          throw "Đã có kỹ năng cùng tên";
        }
        skill.name = name;
        return;
      }
      case "delete_skill": {
        if (!project.skills.some((item) => item.id === command.id)) throw "Không có kỹ năng này";
        project.skills = project.skills.filter((item) => item.id !== command.id);
        for (const resource of project.resources) resource.skills = resource.skills.filter((id) => id !== command.id);
        project.assignment_rules = project.assignment_rules.filter(
          (rule) => !(rule.target.type === "skill" && rule.target.id === command.id),
        );
        return;
      }
      case "set_resource_skills": {
        const resource = project.resources.find((item) => item.uid === command.uid);
        if (!resource) throw "Không tìm thấy nguồn lực";
        if (command.skills.some((id) => !project.skills.some((skill) => skill.id === id))) throw "Không có kỹ năng này";
        resource.skills = [...new Set(command.skills)].sort((a, b) => a - b);
        return;
      }
      case "set_assignment_rules": {
        // The engine's checks that the dialogs can run into.
        for (const rule of command.rules) {
          const fields = rule.conditions.map((condition) => condition.field);
          if (new Set(fields).size !== fields.length) throw "Một quy tắc chỉ có một điều kiện cho mỗi trường";
          if (rule.conditions.some((condition) => condition.values.length === 0)) throw "Mỗi điều kiện cần ít nhất một giá trị";
          if (fields.some((id) => !project.fields.some((field) => field.id === id))) throw "Không có trường này";
          const target = rule.target;
          if (target.type === "skill" && !project.skills.some((skill) => skill.id === target.id)) throw "Không có kỹ năng này";
          if (target.type === "resource" && !project.resources.some((item) => item.uid === target.uid)) throw "Không tìm thấy nguồn lực";
          if (!(rule.units > 0 && rule.units <= 1000)) throw "Đơn vị của quy tắc phải lớn hơn 0 và không quá 1.000";
        }
        project.assignment_rules = structuredClone(command.rules);
        return;
      }
      case "assign_tasks": {
        if (command.assignments.length === 0) throw "Không có việc nào để giao";
        for (const row of command.assignments) {
          if (!project.tasks.some((task) => task.uid === row.task_uid)) throw `Không có công việc ${row.task_uid}`;
          if (!project.resources.some((item) => item.uid === row.resource_uid)) throw "Không tìm thấy nguồn lực";
          if (!(row.units > 0)) throw "Đơn vị nguồn lực phải lớn hơn 0";
        }
        let uid = Math.max(0, ...project.assignments.map((item) => item.uid));
        for (const row of command.assignments) {
          project.assignments.push({
            uid: ++uid,
            task_uid: row.task_uid,
            resource_uid: row.resource_uid,
            units: row.units,
            work: 480 * row.units,
            overtime_work: 0,
            actual_work: 0,
            remaining_work: 480 * row.units,
            cost: 0,
            actual_cost: 0,
            manual_actual_cost: null,
          });
        }
        this.suggestions = this.suggestions.filter(
          (suggestion) => !command.assignments.some((row) => row.task_uid === suggestion.task_uid),
        );
        return;
      }
      case "import_rows": {
        if (command.resources.length === 0 && command.tasks.length === 0) throw "Không có dòng nào để nhập";
        // Only names and order: the fake does not schedule or assign.
        for (const row of command.resources) {
          if (project.resources.some((item) => item.name.toLowerCase() === row.name.trim().toLowerCase())) continue;
          project.resources.push(makeResource({ uid: project.next_resource_uid++, name: row.name.trim(), kind: row.kind }));
        }
        let at = project.tasks.length;
        if (command.after_uid != null) {
          const anchor = project.tasks.findIndex((task) => task.uid === command.after_uid);
          if (anchor < 0) throw "Không tìm thấy công việc";
          // After the anchor's subtree, as the engine does.
          const level = project.tasks[anchor].outline_level;
          at = anchor + 1;
          while (at < project.tasks.length && project.tasks[at].outline_level > level) at++;
        }
        const added = command.tasks.map((row) => makeTask({ uid: project.next_task_uid++, name: row.name.trim() }));
        project.tasks.splice(at, 0, ...added);
        return;
      }
      case "create_calendar": {
        const id = Math.max(0, ...project.calendars.map((item) => item.id)) + 1;
        const base = project.calendars.find((item) => item.id === command.base_id);
        const source = base ?? project.calendars[0];
        project.calendars.push({
          ...structuredClone(source),
          id,
          name: command.name,
          base: base ? { id: base.id, week: base.week.map(() => null), exceptions: {}, work_weeks: [] } : null,
        });
        return;
      }
      case "set_project_calendar":
        project.calendar_id = command.calendar_id;
        return;
      case "set_calendar_base": {
        const calendar = project.calendars.find((item) => item.id === command.calendar_id);
        if (!calendar) throw `Không có lịch ${command.calendar_id}`;
        calendar.base =
          command.base_id == null
            ? null
            : { id: command.base_id, week: calendar.week.map(() => null), exceptions: {}, work_weeks: [] };
        return;
      }
      case "set_calendar_work_week": {
        const calendar = project.calendars.find((item) => item.id === command.calendar_id);
        if (!calendar) throw `Không có lịch ${command.calendar_id}`;
        const week = { name: command.name, from: command.from, to: command.to, week: command.week };
        calendar.work_weeks = [...calendar.work_weeks.filter((item) => item.from !== command.from), week].sort((a, b) =>
          a.from.localeCompare(b.from),
        );
        return;
      }
      case "clear_calendar_work_week": {
        const calendar = project.calendars.find((item) => item.id === command.calendar_id);
        if (!calendar) throw `Không có lịch ${command.calendar_id}`;
        calendar.work_weeks = calendar.work_weeks.filter((item) => item.from !== command.from);
        return;
      }
      case "set_assignment_actual_cost": {
        const assignment = project.assignments.find((item) => item.uid === command.uid);
        if (!assignment) throw `Không có phân công ${command.uid}`;
        assignment.manual_actual_cost = command.cost;
        if (project.editable_actual_costs && command.cost != null) assignment.actual_cost = command.cost;
        return;
      }
      case "set_interrupts": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task) throw `Không có công việc ${command.uid}`;
        if (task.summary || task.milestone) throw "Chỉ việc lá mới chen ngang được";
        task.interrupts = command.interrupts;
        return;
      }
      case "set_work_range": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task) throw `Không có công việc ${command.uid}`;
        if (task.summary || task.milestone) throw "Chỉ việc lá mới có khoảng effort";
        if ((command.low == null) !== (command.high == null)) throw "Khoảng effort cần cả hai mức";
        if (command.low != null && command.high != null && (command.low < 0 || command.low > command.high)) {
          throw "Khoảng effort không hợp lệ";
        }
        task.work_low = command.low;
        task.work_high = command.high;
        return;
      }
      case "set_forecast_settings":
        project.forecast = structuredClone(command.settings);
        return;
      case "set_editable_actual_costs":
        project.editable_actual_costs = command.editable;
        return;
      case "set_task_actual_cost": {
        const task = project.tasks.find((item) => item.uid === command.uid);
        if (!task) throw `Không có công việc ${command.uid}`;
        task.manual_actual_cost = command.cost;
        if (project.editable_actual_costs && command.cost != null) task.actual_cost = command.cost;
        return;
      }
      case "apply_proposal":
        for (const inner of command.commands) this.apply(inner);
        return;
      default:
        // Other commands are only recorded; tests assert on the payload.
        return;
    }
  }
}

/** Set or clear a task's custom field value without mutating the old map. */
function setValue(task: Task, fieldId: number, value: FieldValue | null): void {
  const { [fieldId]: _old, ...rest } = task.values;
  task.values = value === null || value === "" ? rest : { ...rest, [fieldId]: value };
}

function emptyRiskReport(): RiskReport {
  return { cards: [], input_tokens: 0, output_tokens: 0, requests: 1, truncated: false, omitted_tasks: 0, omitted_resources: 0 };
}

function emptyFixReport(): FixReport {
  return { proposals: [], note: "", input_tokens: 0, output_tokens: 0, requests: 1, truncated: false, omitted_tasks: 0, omitted_resources: 0 };
}

/** A trial that finds the proposal better on every count and changes no date. */
function improvedTrial(proposal: Proposal): ProposalTrial {
  const before = { finish: 0, late: 1, over_minutes: 480, critical: 2 };
  return {
    commands: proposal.commands,
    assignments: [],
    moves: [],
    late: [],
    overallocated: [],
    before,
    after: { ...before, late: 0, over_minutes: 0 },
    target_before: 480,
    target_after: 0,
    verdict: "improved",
    worse: [],
  };
}

function defaultAiSettings(): AiSettings {
  return { enabled: false, model: "claude-opus-5-5", mask: false, notice_accepted: false };
}

function emptyNetwork(): NetworkLayout {
  return { nodes: [], edges: [], columns: 0, width: 0, height: 0, crossings: 0 };
}

export const fakeBackend = new FakeBackend();
