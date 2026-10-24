import {
  compactHistory,
  copyTasks,
  exportMspdi,
  importMspdi,
  newProject,
  openProject,
  openRecent,
  redo,
  saveProject,
  saveProjectAs,
  undo,
} from "./api";
import type { Lang } from "./i18n";
import { commitFocusedField } from "./shortcuts";
import { isThemeChoice, type ThemeChoice } from "./theme";
import { toolbarStates } from "./toolbarState";
import type { Command, FillField, Scale, Snapshot, Task, TaskClipboard, ViewId } from "./types";

type Work = () => Promise<Snapshot | void | null>;

/** What a menu item or shortcut can act on; App supplies its state and setters. */
export interface MenuContext {
  snap: Snapshot | null;
  task: Task | null;
  view: ViewId;
  fileDialogBusy: boolean;
  /** History cursor when Thông tin dự án or Lịch làm việc opened; null while both are closed. */
  infoStart: number | null;
  run: (work: Work) => Promise<Snapshot | void>;
  send: (command: Command) => Promise<Snapshot | void>;
  startFileDialog: (work: Work) => void;
  exportPdf: () => void;
  setView: (update: ViewId | ((current: ViewId) => ViewId)) => void;
  setScale: (scale: Scale) => void;
  /** One zoom step of the Gantt timeline in (1) or out (-1), or back to the scale (0). */
  zoomTimeline: (direction: 1 | -1 | 0) => void;
  setEvmOpen: (open: boolean) => void;
  openProjectInfo: () => void;
  /** Thông tin dự án, on its custom fields tab. */
  openCustomFields: () => void;
  openLevelingOrder: () => void;
  openAssignmentRules: () => void;
  /** Preview the suggestions of the assignment rules. */
  openAssignByRules: () => void;
  /** Tệp → Nhập bảng: pick a CSV or workbook, then map its columns. */
  openImportTable: () => void;
  /** Tệp → Nhập → Kéo từ task tracker. */
  openTracker: () => void;
  /** Tệp → Xuất → Đẩy ngày về task tracker. */
  openTrackerPush: () => void;
  /** Dự án → Đồng bộ task tracker: how each field syncs. */
  openTrackerSettings: () => void;
  /** Trợ giúp → License Sheafly Pro. */
  openLicense: () => void;
  /** Lịch làm việc, on the project calendar. */
  openWorkingTime: () => void;
  openBaseline: () => void;
  openRecurring: () => void;
  openSplit: (uid: number) => void;
  changeLang: (lang: Lang) => void;
  changeTheme: (theme: ThemeChoice) => void;
  /** Selected task rows in screen order; the first is `task` unless a range runs upward. */
  selection: number[];
  clipboard: TaskClipboard | null;
  setClipboard: (clipboard: TaskClipboard) => void;
  /** The column fill-down copies: the sheet column whose cell had focus last. */
  fillField: FillField | null;
}

const VIEWS: Record<string, ViewId> = {
  "view-gantt": "gantt",
  "view-tracking": "tracking",
  "view-resources": "resources",
  "view-usage": "usage",
  "view-load": "load",
  "view-forecast": "forecast",
  "view-network": "network",
};

const SCALES: Record<string, Scale> = { "scale-day": "day", "scale-week": "week", "scale-month": "month" };
const ZOOMS: Record<string, 1 | -1 | 0> = { "zoom-in": 1, "zoom-out": -1, "zoom-reset": 0 };

/** Commands on the whole project or document; true when `id` was one of them. */
function documentAction(id: string, ctx: MenuContext): boolean {
  switch (id) {
    case "new":
      ctx.startFileDialog(newProject);
      return true;
    case "open":
      ctx.startFileDialog(openProject);
      return true;
    case "save":
    case "save-as":
      commitFocusedField();
      ctx.startFileDialog(id === "save" ? saveProject : saveProjectAs);
      return true;
    case "import-xml":
      ctx.startFileDialog(importMspdi);
      return true;
    case "import-table":
      ctx.openImportTable();
      return true;
    case "tracker-settings":
      if (ctx.snap && !ctx.fileDialogBusy) ctx.openTrackerSettings();
      return true;
    case "push-tracker":
      if (ctx.snap && !ctx.fileDialogBusy) ctx.openTrackerPush();
      return true;
    case "pull-tracker":
      if (ctx.snap && !ctx.fileDialogBusy) ctx.openTracker();
      return true;
    case "export-xml":
      ctx.startFileDialog(async () => {
        await exportMspdi();
      });
      return true;
    case "export-pdf":
      ctx.exportPdf();
      return true;
    case "undo":
      // Inside Thông tin dự án or Lịch làm việc, undo stops at the state the dialog opened with, so Hủy
      // can still restore exactly that.
      if (!(ctx.infoStart != null && (ctx.snap?.history_cursor ?? 0) <= ctx.infoStart)) void ctx.run(undo);
      return true;
    case "redo":
      void ctx.run(redo);
      return true;
    case "level":
      void ctx.send({ type: "level_resources" });
      return true;
    case "clear-level":
      void ctx.send({ type: "clear_leveling" });
      return true;
    case "reschedule-uncompleted":
      void ctx.send({ type: "reschedule_uncompleted" });
      return true;
    case "compact-history":
      ctx.startFileDialog(compactHistory);
      return true;
    default:
      return false;
  }
}

/** Views, dialogs, language and theme; true when `id` was one of them. */
function viewAction(id: string, ctx: MenuContext): boolean {
  if (id.startsWith("recent:")) {
    ctx.startFileDialog(() => openRecent(Number(id.slice("recent:".length))));
    return true;
  }
  if (VIEWS[id]) {
    ctx.setView(VIEWS[id]);
    return true;
  }
  if (SCALES[id]) {
    ctx.setScale(SCALES[id]);
    return true;
  }
  if (ZOOMS[id] !== undefined) {
    ctx.zoomTimeline(ZOOMS[id]);
    return true;
  }
  if (id.startsWith("theme-")) {
    const theme = id.slice("theme-".length);
    if (isThemeChoice(theme)) ctx.changeTheme(theme);
    return true;
  }
  switch (id) {
    case "view-evm":
      ctx.setView((current) => (current === "gantt" || current === "tracking" ? current : "gantt"));
      ctx.setEvmOpen(true);
      return true;
    case "project-info":
      ctx.openProjectInfo();
      return true;
    case "custom-fields":
      ctx.openCustomFields();
      return true;
    case "leveling-order":
      ctx.openLevelingOrder();
      return true;
    case "assignment-rules":
      ctx.openAssignmentRules();
      return true;
    case "assign-by-rules":
      if (toolbarStates({ snap: ctx.snap, task: ctx.task, view: ctx.view, fileDialogBusy: ctx.fileDialogBusy }).assignByRules.enabled) {
        ctx.openAssignByRules();
      }
      return true;
    case "working-time":
      ctx.openWorkingTime();
      return true;
    case "license":
      ctx.openLicense();
      return true;
    case "lang-vi":
    case "lang-en":
      ctx.changeLang(id === "lang-vi" ? "vi" : "en");
      return true;
    case "baseline":
      ctx.openBaseline();
      return true;
    case "recurring":
      ctx.openRecurring();
      return true;
    default:
      return false;
  }
}

/** Copy, paste and fill-down on the selected rows; true when `id` was one of them. */
function clipboardAction(id: string, ctx: MenuContext): boolean {
  if (id !== "copy-task" && id !== "paste-task" && id !== "fill-down") return false;
  const states = toolbarStates({
    snap: ctx.snap,
    task: ctx.task,
    view: ctx.view,
    fileDialogBusy: ctx.fileDialogBusy,
    selection: ctx.selection,
    hasClipboard: ctx.clipboard != null,
    fillField: ctx.fillField,
  });
  const rows = ctx.selection;
  if (id === "copy-task" && states.copyTask.enabled) {
    void ctx.run(async () => {
      ctx.setClipboard(await copyTasks(rows));
    });
  }
  if (id === "paste-task" && states.pasteTask.enabled && ctx.clipboard) {
    void ctx.send({ type: "paste_tasks", after_uid: rows.length > 0 ? rows[rows.length - 1] : null, clipboard: ctx.clipboard });
  }
  if (id === "fill-down" && states.fillDown.enabled && ctx.fillField) {
    // A cell being typed in commits first, so the fill copies what it now shows.
    commitFocusedField();
    void ctx.send({ type: "fill_down", from_uid: rows[0], to_uids: rows.slice(1), field: ctx.fillField });
  }
  return true;
}

/** Commands on the selected task, under the same rules as the toolbar buttons. */
function taskAction(id: string, ctx: MenuContext) {
  const { task } = ctx;
  if (!task) return;
  const states = toolbarStates({ snap: ctx.snap, task, view: ctx.view, fileDialogBusy: ctx.fileDialogBusy });
  if (id === "split-task" && states.split.enabled) ctx.openSplit(task.uid);
  if (id === "indent" && states.indent.enabled) void ctx.send({ type: "indent", uid: task.uid });
  if (id === "outdent" && states.outdent.enabled) void ctx.send({ type: "outdent", uid: task.uid });
  if (id === "delete" && states.remove.enabled) void ctx.send({ type: "delete_task", uid: task.uid });
}

/** Run the native menu item or shortcut `id` (the menu ids of src-tauri/src/i18n.rs). */
export function handleMenuAction(id: string, ctx: MenuContext) {
  if (documentAction(id, ctx) || viewAction(id, ctx) || clipboardAction(id, ctx)) return;
  taskAction(id, ctx);
}
