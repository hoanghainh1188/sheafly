import type { FieldDef } from "./types";

// Columns of an imported table and what each one feeds. The kinds of a task table follow
// `ImportTask` (plus `units`, the units of the people named on the row); a resource table
// follows `ImportResource`. A custom field is `field:<id>`.

export type ImportTarget = "tasks" | "resources";

export const TASK_COLUMNS = [
  "name",
  "level",
  "work",
  "work_low",
  "work_high",
  "duration",
  "priority",
  "deadline",
  "start_no_earlier",
  "assignees",
  "units",
  "after",
] as const;
export const RESOURCE_COLUMNS = ["name", "kind", "max_units", "rate", "skills", "group"] as const;

export type TaskColumn = (typeof TASK_COLUMNS)[number];
export type ResourceColumn = (typeof RESOURCE_COLUMNS)[number];
/** What a column of the table is: a built-in kind, a custom field, or left out. */
export type ColumnKind = TaskColumn | ResourceColumn | `field:${number}` | "skip";

/** A bare number in an effort or duration cell: hours or days. */
export type BareUnit = "hours" | "days";

export interface ImportSetup {
  target: ImportTarget;
  /** Index in the sheet's rows of the header row; the data starts below it. */
  headerRow: number;
  /** One kind per column of the sheet. */
  columns: string[];
  bareWork: BareUnit;
  bareDuration: BareUnit;
}

/** Header texts each kind is known by, in both languages, folded as `fold` does. */
const TASK_HEADERS: Record<TaskColumn, string[]> = {
  name: ["ten", "ten viec", "ten cong viec", "cong viec", "viec", "tieu de", "name", "task", "task name", "title", "summary"],
  level: ["cap", "cap do", "level", "outline level"],
  work: ["effort", "cong", "khoi luong", "uoc luong", "gio cong", "work", "estimate"],
  work_low: ["effort thap", "uoc luong thap", "thap nhat", "toi thieu", "low effort", "effort low", "minimum", "optimistic", "best case"],
  work_high: ["effort cao", "uoc luong cao", "cao nhat", "high effort", "effort high", "maximum", "pessimistic", "worst case"],
  duration: ["thoi luong", "thoi gian", "duration"],
  priority: ["uu tien", "do uu tien", "priority"],
  deadline: ["han", "han chot", "ngay het han", "deadline", "due", "due date"],
  start_no_earlier: ["bat dau", "ngay bat dau", "bat dau tu", "khong som hon", "start", "start date", "start no earlier"],
  assignees: ["nguoi", "nguoi lam", "phu trach", "nguoi phu trach", "pic", "nguon luc", "assignee", "assignees", "assigned to", "owner", "resource", "resources"],
  units: ["don vi", "ti le", "ty le", "units", "allocation"],
  after: ["cho", "cho task", "cho viec", "phu thuoc", "tien nhiem", "after", "waits for", "depends on", "predecessors"],
};
const RESOURCE_HEADERS: Record<ResourceColumn, string[]> = {
  name: ["ten", "ho ten", "ho va ten", "nguoi", "nguon luc", "name", "resource", "resource name", "person"],
  kind: ["loai", "kieu", "kind", "type"],
  max_units: ["don vi toi da", "toi da", "nang luc", "max units", "maximum units", "capacity"],
  rate: ["don gia", "gia", "luong gio", "rate", "standard rate", "cost"],
  skills: ["ky nang", "skills", "skill"],
  group: ["vai tro", "nhom", "chuc danh", "role", "group", "team"],
};

/** Lower case without accents (đ as d) or extra spaces, so headers match however typed. */
export function fold(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

/**
 * The kind each header suggests: a custom field by its name, else a built-in kind, else
 * left out. A kind goes to the first column that suggests it; the rest are left out.
 */
export function guessColumns(headers: string[], target: ImportTarget, fields: FieldDef[]): ColumnKind[] {
  const known: [string, string[]][] = Object.entries(target === "tasks" ? TASK_HEADERS : RESOURCE_HEADERS);
  const taken = new Set<string>();
  return headers.map((header) => {
    const folded = fold(header);
    if (!folded) return "skip";
    const field = target === "tasks" ? fields.find((item) => fold(item.name) === folded) : undefined;
    const kind = field ? `field:${field.id}` : known.find(([, names]) => names.includes(folded))?.[0];
    if (!kind || taken.has(kind)) return "skip";
    taken.add(kind);
    return kind as ColumnKind;
  });
}

/** The field a `field:<id>` kind names, if the project still has it. */
export function fieldOfColumn(kind: string, fields: FieldDef[]): FieldDef | undefined {
  if (!kind.startsWith("field:")) return undefined;
  const id = Number(kind.slice("field:".length));
  return fields.find((field) => field.id === id);
}
