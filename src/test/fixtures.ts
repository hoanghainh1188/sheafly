import type {
  HealthCriterion,
  CashFlowPeriod,
  CashFlowReport,
  HealthReport,
  Assignment,
  BurndownReport,
  Calendar,
  EvmMetrics,
  LicenseView,
  Project,
  ProjectMetrics,
  ProjectStatistics,
  Resource,
  SCurveReport,
  Snapshot,
  Task,
} from "../types";

export const DAY_MS = 86_400_000;
export const HOUR_MS = 3_600_000;

/** Monday 05/01/2026 08:00 in project wall-clock (UTC fields). */
export const MONDAY_8AM = Date.UTC(2026, 0, 5, 8, 0);

export function at(year: number, month: number, day: number, hour = 0, minute = 0): number {
  return Date.UTC(year, month - 1, day, hour, minute);
}

export function makeTask(overrides: Partial<Task> & { uid: number }): Task {
  const start = overrides.start ?? MONDAY_8AM;
  return {
    id: overrides.uid,
    name: `Task ${overrides.uid}`,
    wbs: String(overrides.uid),
    outline_level: 1,
    parent_uid: null,
    notes: "",
    priority: 500,
    milestone: false,
    summary: false,
    manual: false,
    duration: 480,
    constraint_type: "asap",
    constraint_date: null,
    calendar_id: null,
    task_type: "fixed_units",
    effort_driven: false,
    percent_complete: 0,
    actual_start: null,
    actual_finish: null,
    remaining_duration: 480,
    remaining_work: 480,
    fixed_cost: 0,
    fixed_cost_accrual: "prorated",
    leveling_delay: 0,
    interrupts: false,
    start,
    finish: start + 9 * HOUR_MS,
    early_start: start,
    early_finish: start + 9 * HOUR_MS,
    late_start: start,
    late_finish: start + 9 * HOUR_MS,
    total_slack: 0,
    free_slack: 0,
    critical: false,
    units: 1,
    work: 480,
    actual_work: 0,
    cost: 0,
    actual_cost: 0,
    remaining_cost: 0,
    manual_actual_cost: null,
    conflict: false,
    predecessors: "",
    baselines: [],
    deadline: null,
    late: false,
    recurrence: null,
    series_uid: null,
    occurrence_date: null,
    detached: false,
    values: {},
    splits: [],
    segments: [],
    ...overrides,
  };
}

export function makeResource(overrides: Partial<Resource> & { uid: number }): Resource {
  return {
    name: `NL ${overrides.uid}`,
    kind: "person",
    max_units: 1,
    calendar_id: null,
    standard_rate: 0,
    overtime_rate: 0,
    cost_per_use: 0,
    availability: [],
    skills: [],
    ...overrides,
  };
}

export function makeAssignment(
  overrides: Partial<Assignment> & { uid: number; task_uid: number; resource_uid: number },
): Assignment {
  return {
    units: 1,
    work: 480,
    overtime_work: 0,
    actual_work: 0,
    remaining_work: 480,
    cost: 0,
    actual_cost: 0,
    manual_actual_cost: null,
    baselines: [],
    ...overrides,
  };
}

export function standardCalendar(id = 1, name = "Tiêu chuẩn"): Calendar {
  const workday = [
    { start_minute: 8 * 60, end_minute: 12 * 60 },
    { start_minute: 13 * 60, end_minute: 17 * 60 },
  ];
  return { id, name, week: [workday, workday, workday, workday, workday, [], []], exceptions: {}, work_weeks: [], base: null };
}

export function makeProject(overrides: Partial<Project> = {}): Project {
  return {
    name: "Dự án thử",
    start: MONDAY_8AM,
    status_date: null,
    calendar_id: 1,
    calendars: [standardCalendar()],
    tasks: [],
    dependencies: [],
    resources: [],
    assignments: [],
    next_task_uid: 1,
    next_resource_uid: 1,
    overallocations: [],
    usage: [],
    editable_actual_costs: false,
    fields: [],
    leveling_order: [],
    skills: [],
    next_skill_id: 1,
    assignment_rules: [],
    forecast: { field: null, group: "medium", common: "low" },
    focus_factor: 1,
    source: null,
    cash_flow: { contract_value: null, opening_balance: 0, retention: null },
    payments: [],
    next_payment_id: 1,
    ...overrides,
  };
}

export function makeSnapshot(project: Project = makeProject()): Snapshot {
  return { project, path: null, dirty: false, can_undo: false, can_redo: false, history_cursor: 0 };
}

export function makeEvm(overrides: Partial<EvmMetrics> = {}): EvmMetrics {
  return {
    bac: 10_000_000,
    pv: 4_000_000,
    ev: 3_000_000,
    ac: 3_500_000,
    sv: -1_000_000,
    cv: -500_000,
    spi: 0.75,
    cpi: 0.857,
    eac: 11_666_667,
    etc: 8_166_667,
    vac: -1_666_667,
    ...overrides,
  };
}

export function emptyBurndown(overrides: Partial<BurndownReport> = {}): BurndownReport {
  return {
    status_date: null,
    baseline_start: null,
    baseline_finish: null,
    baseline_work: 0,
    ideal: [],
    planned: [],
    actual: [],
    current_remaining_work: 0,
    ...overrides,
  };
}

export function makeMetrics(
  evm: Partial<ProjectMetrics["evm"]> = {},
  burndown: Partial<BurndownReport> = {},
  sCurve: Partial<SCurveReport> = {},
): ProjectMetrics {
  return {
    evm: { status_date: at(2026, 1, 15), has_baseline: true, metrics: makeEvm(), ...evm },
    burndown: emptyBurndown(burndown),
    s_curve: { status_date: null, baseline_finish: null, step: "day", points: [], ...sCurve },
    statistics: makeStatistics(),
  };
}

/**
 * Outline used by the view-row tests:
 * 1 Thiết kế (summary)            uid 1
 *   1.1 Phác thảo (critical, An)  uid 2
 *   1.2 Duyệt (late, An + blank)  uid 3
 *   1.3 Kiểm thử (summary)        uid 4
 *     1.3.1 Chạy thử              uid 5
 * 2 Thi công (summary)            uid 6
 *   2.1 Đổ móng (blank resource)  uid 7
 * 3 Bàn giao (top-level leaf)     uid 8
 */
export function outlineProject(): Project {
  const tasks = [
    makeTask({ uid: 1, name: "Thiết kế", wbs: "1", summary: true }),
    makeTask({ uid: 2, name: "Phác thảo", wbs: "1.1", outline_level: 2, parent_uid: 1, critical: true }),
    makeTask({ uid: 3, name: "Duyệt", wbs: "1.2", outline_level: 2, parent_uid: 1, late: true }),
    makeTask({ uid: 4, name: "Kiểm thử", wbs: "1.3", outline_level: 2, parent_uid: 1, summary: true }),
    makeTask({ uid: 5, name: "Chạy thử", wbs: "1.3.1", outline_level: 3, parent_uid: 4 }),
    makeTask({ uid: 6, name: "Thi công", wbs: "2", summary: true }),
    makeTask({ uid: 7, name: "Đổ móng", wbs: "2.1", outline_level: 2, parent_uid: 6 }),
    makeTask({ uid: 8, name: "Bàn giao", wbs: "3", milestone: true, duration: 0 }),
  ];
  const resources = [
    makeResource({ uid: 10, name: "An" }),
    makeResource({ uid: 11, name: "   " }),
    makeResource({ uid: 12, name: "Chi" }),
  ];
  const assignments = [
    makeAssignment({ uid: 100, task_uid: 2, resource_uid: 10 }),
    makeAssignment({ uid: 101, task_uid: 3, resource_uid: 11 }),
    makeAssignment({ uid: 102, task_uid: 3, resource_uid: 10 }),
    makeAssignment({ uid: 103, task_uid: 7, resource_uid: 11 }),
  ];
  return makeProject({ tasks, resources, assignments });
}

export function makeStatistics(overrides: Partial<ProjectStatistics> = {}): ProjectStatistics {
  return {
    start: MONDAY_8AM,
    finish: MONDAY_8AM + 9 * HOUR_MS,
    duration: 480,
    work: 480,
    actual_work: 0,
    remaining_work: 480,
    cost: 0,
    actual_cost: 0,
    remaining_cost: 0,
    percent_complete: 0,
    baseline: null,
    ...overrides,
  };
}

/** A license view as `license_status` answers; Free with no key unless overridden. */
export function makeLicense(overrides: Partial<LicenseView> = {}): LicenseView {
  return {
    state: "none",
    plan: null,
    name: null,
    email: null,
    seats: null,
    updates_until: null,
    expires: null,
    problem: null,
    problem_day: null,
    build_date: "2026-10-24",
    features: [],
    ...overrides,
  };
}

/** An active Sheafly Pro license. */
export function makeProLicense(overrides: Partial<LicenseView> = {}): LicenseView {
  return makeLicense({
    state: "active",
    plan: "pro",
    name: "Lan",
    email: "lan@example.com",
    seats: 1,
    updates_until: "2027-10-24",
    features: ["trackers", "capacity"],
    ...overrides,
  });
}

/** A health criterion that passes with nothing to look at. */
export function makeHealthCriterion(number: number, overrides: Partial<HealthCriterion> = {}): HealthCriterion {
  return {
    number,
    status: "pass",
    not_applicable: null,
    count: 0,
    total: 10,
    value: 0,
    limit: { kind: "at_most", value: 0.05 },
    tasks: [],
    ...overrides,
  };
}

/** A health report where every criterion passes, with `criteria` replacing some by number. */
export function makeCashFlowReport(periods: CashFlowPeriod[] = []): CashFlowReport {
  const balances = periods.map((p) => p.balance.forecast);
  return {
    bucket: "month",
    baseline: null,
    status_date: null,
    opening_balance: 0,
    periods,
    payments: [],
    lowest_balance: balances.length > 0 ? Math.min(...balances) : null,
    first_deficit: periods.find((p) => p.deficit)?.date ?? null,
    notes: [],
  };
}

export function makeHealthReport(criteria: HealthCriterion[] = []): HealthReport {
  const all = Array.from({ length: 14 }, (_, i) => criteria.find((c) => c.number === i + 1) ?? makeHealthCriterion(i + 1));
  const passed = all.filter((c) => c.status === "pass").length;
  const failed = all.filter((c) => c.status === "fail").length;
  return { criteria: all, passed, applicable: passed + failed, rating: failed === 0 ? "good" : "at_risk" };
}
