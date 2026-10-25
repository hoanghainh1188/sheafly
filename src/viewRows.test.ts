import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { setCurrentLang } from "./i18n";
import { makeAssignment, makeProject, makeResource, makeTask, outlineProject } from "./test/fixtures";
import {
  buildViewRows,
  healthCriterionOf,
  type FilterMode,
  type GroupMode,
  type HighlightMode,
  type ViewRow,
} from "./viewRows";

/** Compact view of rows: "# label" for group headers, uid (with "*" when highlighted) for tasks. */
function shape(rows: ViewRow[]): string[] {
  return rows.map((row) =>
    row.kind === "group" ? `# ${row.label}` : `${row.task.uid}${row.highlighted ? "*" : ""}`,
  );
}

describe("buildViewRows without grouping", () => {
  it("keeps outline order and uses task keys", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "none", "none");
    expect(shape(rows)).toEqual(["1", "2", "3", "4", "5", "6", "7", "8"]);
    expect(rows.map((row) => row.key)).toEqual([1, 2, 3, 4, 5, 6, 7, 8].map((uid) => `task-${uid}`));
  });

  it("returns no rows for an empty project", () => {
    expect(buildViewRows(makeProject(), "none", null, "summary", "none")).toEqual([]);
  });

  it("highlights critical or late tasks without hiding the rest", () => {
    const project = outlineProject();
    expect(shape(buildViewRows(project, "none", null, "none", "critical"))).toEqual([
      "1", "2*", "3", "4", "5", "6", "7", "8",
    ]);
    expect(shape(buildViewRows(project, "none", null, "none", "late"))).toEqual([
      "1", "2", "3*", "4", "5", "6", "7", "8",
    ]);
  });
});

describe("buildViewRows filters", () => {
  it("keeps urgent tasks with the interrupt filter and highlights them", () => {
    const project = outlineProject();
    project.tasks = project.tasks.map((task) => (task.uid === 7 ? { ...task, interrupts: true } : task));
    expect(shape(buildViewRows(project, "interrupts", null, "none", "none"))).toEqual(["6", "7"]);
    expect(shape(buildViewRows(project, "none", null, "none", "interrupts"))).toEqual([
      "1", "2", "3", "4", "5", "6", "7*", "8",
    ]);
  });

  it("keeps a health criterion's tasks, and does nothing before its report is in", () => {
    const project = outlineProject();
    expect(shape(buildViewRows(project, "health:1", null, "none", "none", "", null, new Set([5, 8])))).toEqual([
      "1", "4", "5", "8",
    ]);
    expect(buildViewRows(project, "health:1", null, "none", "none", "", null, null)).toHaveLength(8);
    expect(shape(buildViewRows(project, "health:3", null, "none", "none", "", null, new Set()))).toEqual([]);
    expect(healthCriterionOf("health:14")).toBe(14);
    expect(healthCriterionOf("health:15")).toBeNull();
    expect(healthCriterionOf("late")).toBeNull();
  });

  it("critical filter keeps matching tasks and their summary ancestors", () => {
    expect(shape(buildViewRows(outlineProject(), "critical", null, "none", "none"))).toEqual(["1", "2"]);
  });

  it("late filter keeps matching tasks and their summary ancestors", () => {
    expect(shape(buildViewRows(outlineProject(), "late", null, "none", "none"))).toEqual(["1", "3"]);
  });

  it("keeps every summary up the chain for a deep match", () => {
    const project = outlineProject();
    project.tasks[4].critical = true;
    expect(shape(buildViewRows(project, "critical", null, "none", "none"))).toEqual(["1", "2", "4", "5"]);
  });

  it("resource filter shows tasks assigned to that resource", () => {
    const project = outlineProject();
    expect(shape(buildViewRows(project, "resource", 11, "none", "none"))).toEqual(["1", "3", "6", "7"]);
    expect(shape(buildViewRows(project, "resource", 12, "none", "none"))).toEqual([]);
  });

  it("resource filter without a chosen resource hides everything", () => {
    expect(buildViewRows(outlineProject(), "resource", null, "none", "none")).toEqual([]);
  });

  it("does not mutate the project or its task order", () => {
    const project = outlineProject();
    const before = structuredClone(project);
    buildViewRows(project, "late", null, "summary", "critical");
    buildViewRows(project, "none", null, "resource", "late");
    expect(project).toEqual(before);
  });
});

describe("buildViewRows grouped by person", () => {
  it("follows resource order, lists a task under every assignee and puts unassigned last", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "resource", "none");
    expect(shape(rows)).toEqual([
      "# An", "2", "3",
      "# NL 11", "3", "7",
      "# Chưa gán", "1", "4", "5", "6", "8",
    ]);
  });

  it("lists a task once per person and leaves assignments to unknown resources out", () => {
    const project = makeProject({
      tasks: [makeTask({ uid: 1 }), makeTask({ uid: 2 })],
      resources: [makeResource({ uid: 10, name: "An" })],
      assignments: [
        makeAssignment({ uid: 1, task_uid: 1, resource_uid: 10 }),
        makeAssignment({ uid: 2, task_uid: 1, resource_uid: 10 }),
        makeAssignment({ uid: 3, task_uid: 2, resource_uid: 99 }),
      ],
    });
    expect(shape(buildViewRows(project, "none", null, "resource", "none"))).toEqual(["# An", "1", "# Chưa gán", "2"]);
  });

  it("groups 5,000 tasks over 500 people quickly", () => {
    const tasks = Array.from({ length: 5000 }, (_, i) => makeTask({ uid: i + 1 }));
    const resources = Array.from({ length: 500 }, (_, i) => makeResource({ uid: i + 1 }));
    const assignments = tasks.map((task, i) => makeAssignment({ uid: i + 1, task_uid: task.uid, resource_uid: (i % 500) + 1 }));
    const project = makeProject({ tasks, resources, assignments });
    const started = performance.now();
    const rows = buildViewRows(project, "resource", 7, "resource", "none");
    const grouped = buildViewRows(project, "none", null, "resource", "none");
    // Scanning every assignment for every task and person took seconds here.
    expect(performance.now() - started).toBeLessThan(2000);
    expect(rows).toHaveLength(11);
    expect(grouped).toHaveLength(5500);
  });

  it("skips resources without tasks", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "resource", "none");
    expect(rows.some((row) => row.kind === "group" && row.label === "Chi")).toBe(false);
  });

  it("keeps co-assignees when combined with the resource filter", () => {
    const rows = buildViewRows(outlineProject(), "resource", 10, "resource", "none");
    // Summary 1 is kept by the filter but has no assignment, so it lands in "Chưa gán".
    expect(shape(rows)).toEqual(["# An", "2", "3", "# NL 11", "3", "# Chưa gán", "1"]);
  });

  it("omits the unassigned group when everything is assigned", () => {
    const flat = makeProject({
      tasks: [makeTask({ uid: 1 })],
      resources: outlineProject().resources,
      assignments: [outlineProject().assignments[0]].map((item) => ({ ...item, task_uid: 1 })),
    });
    expect(shape(buildViewRows(flat, "none", null, "resource", "none"))).toEqual(["# An", "1"]);
  });

  it("uses group-prefixed keys so a task under two people stays unique", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "resource", "none");
    const keys = rows.map((row) => row.key);
    expect(keys).toContain("group-10-task-3");
    expect(keys).toContain("group-11-task-3");
    expect(keys).toContain("group-unassigned");
    expect(keys).toContain("group-unassigned-task-8");
  });
});

describe("buildViewRows grouped by summary", () => {
  it("puts leaves under their nearest summary in outline order and orphans last", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "summary", "none");
    expect(shape(rows)).toEqual([
      "# 1 Thiết kế", "2", "3",
      "# 1.3 Kiểm thử", "5",
      "# 2 Thi công", "7",
      "# Không có summary", "8",
    ]);
  });

  it("orders groups by outline position, not by when a leaf is seen", () => {
    const project = outlineProject();
    // Move "Đổ móng" ahead in the list; its summary (uid 6) still follows uid 1 and uid 4.
    const [leaf] = project.tasks.splice(6, 1);
    project.tasks.splice(1, 0, leaf);
    const labels = buildViewRows(project, "none", null, "summary", "none")
      .filter((row) => row.kind === "group")
      .map((row) => (row.kind === "group" ? row.label : ""));
    expect(labels).toEqual(["1 Thiết kế", "1.3 Kiểm thử", "2 Thi công", "Không có summary"]);
  });

  it("labels summaries without WBS or name", () => {
    const project = makeProject({
      tasks: [
        makeTask({ uid: 1, name: "  ", wbs: "", summary: true }),
        makeTask({ uid: 2, parent_uid: 1, outline_level: 2 }),
        makeTask({ uid: 3, name: "Gói B", wbs: "", summary: true }),
        makeTask({ uid: 4, parent_uid: 3, outline_level: 2 }),
      ],
    });
    expect(shape(buildViewRows(project, "none", null, "summary", "none"))).toEqual([
      "# Task 1", "2", "# Gói B", "4",
    ]);
  });

  it("omits the no-summary group when every leaf has a parent", () => {
    const project = outlineProject();
    project.tasks = project.tasks.filter((task) => task.uid !== 8);
    const labels = shape(buildViewRows(project, "none", null, "summary", "none"));
    expect(labels).not.toContain("# Không có summary");
  });
});

describe("buildViewRows combined filter, group and highlight", () => {
  it("filters first, then groups, then highlights", () => {
    const rows = buildViewRows(outlineProject(), "resource", 11, "summary", "late");
    expect(shape(rows)).toEqual(["# 1 Thiết kế", "3*", "# 2 Thi công", "7"]);
  });

  it("highlights inside person groups", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "resource", "critical");
    expect(shape(rows).slice(0, 3)).toEqual(["# An", "2*", "3"]);
  });
});

describe("buildViewRows keys", () => {
  const filters: FilterMode[] = ["none", "critical", "resource", "late"];
  const groups: GroupMode[] = ["none", "resource", "summary"];
  const highlights: HighlightMode[] = ["none", "critical", "late"];

  it("are unique for every combination of modes", () => {
    const project = outlineProject();
    for (const filter of filters) {
      for (const group of groups) {
        for (const highlight of highlights) {
          for (const resource of [null, 10, 11]) {
            const keys = buildViewRows(project, filter, resource, group, highlight).map((row) => row.key);
            expect(new Set(keys).size, `${filter}/${group}/${highlight}/${resource}`).toBe(keys.length);
          }
        }
      }
    }
  });
});

describe("buildViewRows group headers in English", () => {
  beforeEach(() => setCurrentLang("en"));
  afterEach(() => setCurrentLang("vi"));

  it("names unassigned tasks and unnamed resources", () => {
    const rows = buildViewRows(outlineProject(), "none", null, "resource", "none");
    expect(shape(rows)).toEqual([
      "# An", "2", "3",
      "# Resource 11", "3", "7",
      "# Unassigned", "1", "4", "5", "6", "8",
    ]);
  });

  it("names top-level leaves that have no summary", () => {
    const labels = shape(buildViewRows(outlineProject(), "none", null, "summary", "none")).filter((item) =>
      item.startsWith("# "),
    );
    expect(labels).toEqual(["# 1 Thiết kế", "# 1.3 Kiểm thử", "# 2 Thi công", "# No summary task"]);
  });
});

describe("buildViewRows by custom field", () => {
  /** The outline project with a list field "Mảng": BE on 2 and 5, FE on 3 and on summary 6. */
  function fieldProject() {
    const project = outlineProject();
    const values: Record<number, string> = { 2: "BE", 3: "FE", 5: "BE", 6: "FE" };
    return {
      ...project,
      fields: [{ id: 5, name: "Mảng", kind: { type: "choice" as const, options: ["FE", "BE"] } }],
      tasks: project.tasks.map((task) => (values[task.uid] ? { ...task, values: { 5: values[task.uid] } } : task)),
    };
  }

  it("filters on one value and keeps summaries above the matches", () => {
    expect(shape(buildViewRows(fieldProject(), "field:5", null, "none", "none", "BE"))).toEqual(["1", "2", "4", "5"]);
  });

  it("filters on no value with the empty key", () => {
    // Summaries 1 and 4 have no value but only show when a task under them has none.
    expect(shape(buildViewRows(fieldProject(), "field:5", null, "none", "none", ""))).toEqual(["6", "7", "8"]);
  });

  it("groups by value in the list's order, then the tasks with none", () => {
    expect(shape(buildViewRows(fieldProject(), "none", null, "field:5", "none"))).toEqual([
      "# FE",
      "3",
      "6",
      "# BE",
      "2",
      "5",
      "# Chưa có Mảng",
      "7",
      "8",
    ]);
  });

  it("ignores a mode whose field is gone", () => {
    const all = shape(buildViewRows(fieldProject(), "none", null, "none", "none"));
    expect(shape(buildViewRows(fieldProject(), "field:99", null, "field:99", "none", "BE"))).toEqual(all);
  });
});

describe("buildViewRows group keys", () => {
  it("stay unique whatever the values look like", () => {
    const project = outlineProject();
    const values: Record<number, string> = { 2: "x", 3: "x-task-2", 5: "-task-7" };
    const rows = buildViewRows(
      {
        ...project,
        fields: [{ id: 1, name: "Mã", kind: { type: "text" as const } }],
        tasks: project.tasks.map((task) => (values[task.uid] ? { ...task, values: { 1: values[task.uid] } } : task)),
      },
      "none",
      null,
      "field:1",
      "none",
    );
    const keys = rows.map((row) => row.key);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
