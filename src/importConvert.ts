import { parseUnits } from "./assignmentUnits";
import { parseFieldValue } from "./fieldValues";
import { MINUTES_PER_DAY, parseDuration, parseInstant, parseMoney } from "./format";
import { pick } from "./i18n";
import { fieldOfColumn, fold, type BareUnit, type ImportSetup } from "./importColumns";
import { importConvertMessages } from "./importConvert.i18n";
import type { FieldDef, ImportAssignee, ImportProblem, ImportResource, ImportSheet, ImportTask, Project } from "./types";

// Turning the text cells of an imported table into the typed rows `import_rows` takes,
// with the cells that cannot be read. Cells are read the way the rest of the UI reads
// typed text (either decimal mark, both date forms), and all text is put in Unicode
// composed form first: files from macOS can hold decomposed Vietnamese, which would not
// match the names in the project.

/** A cell that cannot be read, or a problem with the setup when `line` is null. */
export interface ImportIssue {
  /** The file's line or row number (from 1), as `ImportSheet.lines` gives it. */
  line: number | null;
  /** Index of the column in the sheet. */
  column: number | null;
  message: string;
}

export interface Converted {
  resources: ImportResource[];
  tasks: ImportTask[];
  /** The file line of each converted row, in order. */
  lines: number[];
  issues: ImportIssue[];
}

/** Several names or skills in one cell. */
const LIST_SEPARATOR = /[;,]/;
const WHOLE = /^-?\d+$/;

function text(cell: string | undefined): string {
  return (cell ?? "").normalize("NFC").trim();
}

function list(cell: string): string[] {
  return cell.split(LIST_SEPARATOR).map((item) => item.trim()).filter(Boolean);
}

/** Thousands grouped the way the language writes money: "3.480.000", "3,480,000". */
/** A decimal with either mark; a spreadsheet's "12.5" stays 12.5. */
function decimal(cell: string): number | null {
  const cleaned = cell.replace(/\s/g, "");
  if (!/^-?(\d+([.,]\d+)?|[.,]\d+)$/.test(cleaned)) return null;
  return Number(cleaned.replace(",", "."));
}

/** Working minutes; a bare number is in `bare`. */
function minutes(cell: string, bare: BareUnit): number | null {
  const number = decimal(cell);
  if (number != null) return number < 0 ? null : Math.round(number * (bare === "hours" ? 60 : MINUTES_PER_DAY));
  const parsed = parseDuration(cell);
  return parsed != null && parsed >= 0 ? parsed : null;
}

/** Units as typed for an assignment, or a percentage ("50%"). */
function units(cell: string): number | null {
  const percent = /^(.+?)\s*%$/.exec(cell);
  if (percent) {
    const number = decimal(percent[1]);
    return number != null && number >= 0 ? number / 100 : null;
  }
  return parseUnits(cell);
}

/** A date with or without a time; without one (or at midnight), `defaultHour`. */
function instant(cell: string, defaultHour: number): number | null {
  if (parseInstant(cell) == null) return null;
  // A date cell formatted with a time shows midnight when it holds only a day.
  const day = cell.replace(/\s+0?0:00$/, "");
  if (/\d:\d{2}$/.test(day)) return parseInstant(day);
  const morning = parseInstant(day);
  return morning == null ? null : morning + (defaultHour - 8) * 3_600_000;
}

const PERSON = new Set(["nguoi", "nhan su", "nhan vien", "person", "people", "work", ""]);
const MACHINE = new Set(["may", "may moc", "thiet bi", "machine", "equipment"]);

interface Context {
  setup: ImportSetup;
  fields: FieldDef[];
  issues: ImportIssue[];
  line: number;
}

function issue(context: Context, column: number, message: string): void {
  context.issues.push({ line: context.line, column, message });
}

/** Rows of `sheet` below the header that have anything in a column the setup uses. */
function dataRows(sheet: ImportSheet, setup: ImportSetup): { cells: string[]; line: number }[] {
  const used = setup.columns.map((kind) => kind !== "skip");
  return sheet.rows.flatMap((cells, index) => {
    if (index <= setup.headerRow) return [];
    const normalized = cells.map(text);
    if (!normalized.some((cell, column) => used[column] && cell)) return [];
    return [{ cells: normalized, line: sheet.lines[index] ?? index + 1 }];
  });
}

export function convertTable(sheet: ImportSheet, setup: ImportSetup, project: Project): Converted {
  const m = pick(importConvertMessages);
  const converted: Converted = { resources: [], tasks: [], lines: [], issues: [] };
  if (!setup.columns.includes("name")) {
    converted.issues.push({ line: null, column: null, message: m.needsName });
    return converted;
  }
  const rows = dataRows(sheet, setup);
  converted.lines = rows.map((row) => row.line);
  const context: Context = { setup, fields: project.fields, issues: converted.issues, line: 0 };
  if (setup.target === "resources") {
    converted.resources = rows.map((row) => {
      context.line = row.line;
      return resourceRow(row.cells, context);
    });
  } else {
    const index = new Map(rows.map((row, position) => [row.line, position]));
    converted.tasks = rows.map((row) => {
      context.line = row.line;
      return taskRow(row.cells, context, index);
    });
  }
  return converted;
}

function taskRow(cells: string[], context: Context, index: Map<number, number>): ImportTask {
  const m = pick(importConvertMessages);
  const task: ImportTask = { name: "", level: 0, values: [], assignees: [], after: [] };
  let names: string[] = [];
  let share = 1;
  context.setup.columns.forEach((kind, column) => {
    const cell = cells[column] ?? "";
    if (kind === "name") task.name = cell;
    if (!cell) return;
    switch (kind) {
      case "level": {
        const level = WHOLE.test(cell) ? Number(cell) : 0;
        if (level >= 1) task.level = level - 1;
        else issue(context, column, m.level);
        return;
      }
      case "work_low":
      case "work_high": {
        const value = minutes(cell, context.setup.bareWork);
        if (value == null) issue(context, column, m.work);
        else task[kind] = value;
        return;
      }
      case "work":
      case "duration": {
        const value = minutes(cell, kind === "work" ? context.setup.bareWork : context.setup.bareDuration);
        if (value == null) issue(context, column, kind === "work" ? m.work : m.duration);
        else task[kind] = value;
        return;
      }
      case "priority": {
        const value = WHOLE.test(cell) ? Number(cell) : NaN;
        if (value >= 0 && value <= 1000) task.priority = value;
        else issue(context, column, m.priority);
        return;
      }
      case "deadline":
      case "start_no_earlier": {
        // A deadline on a day is met by the end of that working day.
        const value = instant(cell, kind === "deadline" ? 17 : 8);
        if (value == null) issue(context, column, m.date);
        else task[kind] = value;
        return;
      }
      case "assignees":
        names = list(cell);
        return;
      case "units": {
        const value = units(cell);
        if (value == null || value <= 0) issue(context, column, m.units);
        else share = value;
        return;
      }
      case "after": {
        const rows: number[] = [];
        for (const item of list(cell)) {
          const row = WHOLE.test(item) ? index.get(Number(item)) : undefined;
          if (row == null) {
            issue(context, column, m.noLine(item));
            return;
          }
          rows.push(row);
        }
        task.after = rows;
        return;
      }
      default: {
        const field = fieldOfColumn(kind, context.fields);
        if (!field) return;
        const value = parseFieldValue(field, cell);
        if (value === undefined) issue(context, column, m.field(field.name));
        else if (value !== null) task.values?.push([field.name, value]);
      }
    }
  });
  task.assignees = names.map((name): ImportAssignee => ({ name, units: share }));
  return task;
}

function resourceRow(cells: string[], context: Context): ImportResource {
  const m = pick(importConvertMessages);
  const resource: ImportResource = { name: "", kind: "person", skills: [] };
  context.setup.columns.forEach((kind, column) => {
    const cell = cells[column] ?? "";
    if (kind === "name") resource.name = cell;
    if (!cell) return;
    if (kind === "kind") {
      const folded = fold(cell);
      if (MACHINE.has(folded)) resource.kind = "machine";
      else if (!PERSON.has(folded)) issue(context, column, m.kind);
    } else if (kind === "max_units") {
      const value = units(cell);
      if (value == null || value <= 0) issue(context, column, m.units);
      else resource.max_units = value;
    } else if (kind === "rate") {
      // Grouped the way the language writes money ("3.480.000"), as the money fields read it.
      const value = parseMoney(cell);
      if (value == null || value < 0) issue(context, column, m.rate);
      else resource.standard_rate = value;
    } else if (kind === "skills") {
      // `Skill:Level` gives the level; the engine splits it.
      resource.skills = list(cell);
    } else if (kind === "group") {
      resource.group = cell.trim();
    }
  });
  return resource;
}

/** The engine's kinds of cell and the setup kinds that hold them. */
const ENGINE_COLUMNS: Record<string, string[]> = {
  name: ["name"],
  kind: ["kind"],
  max_units: ["max_units"],
  rate: ["rate"],
  skills: ["skills"],
  group: ["group"],
  level: ["level"],
  work: ["work"],
  work_low: ["work_low"],
  work_high: ["work_high", "work_low"],
  duration: ["duration", "work"],
  priority: ["priority"],
  deadline: ["deadline"],
  start_no_earlier: ["start_no_earlier"],
  assignees: ["assignees", "units"],
  after: ["after"],
};

/** `check_import`'s problems at the file line and sheet column they belong to. */
export function engineIssues(problems: ImportProblem[], converted: Converted, setup: ImportSetup, fields: FieldDef[]): ImportIssue[] {
  return problems.map((problem) => {
    const line = problem.row == null ? null : (converted.lines[problem.row] ?? null);
    let column: number | null = null;
    if (typeof problem.column === "object") {
      const name = fold(problem.column.field);
      const found = setup.columns.findIndex((kind) => {
        const field = fieldOfColumn(kind, fields);
        return field != null && fold(field.name) === name;
      });
      column = found >= 0 ? found : null;
    } else {
      for (const kind of ENGINE_COLUMNS[problem.column] ?? []) {
        const found = setup.columns.indexOf(kind);
        if (found >= 0) {
          column = found;
          break;
        }
      }
    }
    return { line, column, message: problem.message };
  });
}
