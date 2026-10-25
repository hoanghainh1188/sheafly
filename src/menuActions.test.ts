import { describe, expect, it, vi } from "vitest";

import { handleMenuAction, type MenuContext } from "./menuActions";
import { makeProject, makeSnapshot, makeTask } from "./test/fixtures";

function context(overrides: Partial<MenuContext> = {}): MenuContext {
  const project = makeProject({ tasks: [makeTask({ uid: 1 }), makeTask({ uid: 2 })] });
  return {
    snap: makeSnapshot(project),
    task: project.tasks[1],
    view: "gantt",
    fileDialogBusy: false,
    infoStart: null,
    run: vi.fn(async () => undefined),
    send: vi.fn(async () => undefined),
    startFileDialog: vi.fn(),
    exportPdf: vi.fn(),
    setView: vi.fn(),
    setScale: vi.fn(),
    zoomTimeline: vi.fn(),
    setEvmOpen: vi.fn(),
    openProjectInfo: vi.fn(),
    openCustomFields: vi.fn(),
    openLevelingOrder: vi.fn(),
    openAssignmentRules: vi.fn(),
    openAssignByRules: vi.fn(),
    openImportTable: vi.fn(),
    openTracker: vi.fn(),
    openTrackerPush: vi.fn(),
    openTrackerSettings: vi.fn(),
    openWorkingTime: vi.fn(),
    openBaseline: vi.fn(),
    openLicense: vi.fn(),
    openAiSettings: vi.fn(),
    openRecurring: vi.fn(),
    openSplit: vi.fn(),
    changeLang: vi.fn(),
    changeTheme: vi.fn(),
    selection: [2],
    clipboard: null,
    setClipboard: vi.fn(),
    fillField: "duration",
    reportingPro: true,
    ...overrides,
  };
}

const CLIPBOARD = { tasks: [], links: [] };

describe("copy, paste and fill-down", () => {
  it("pastes the copied tasks after the last selected row", () => {
    const ctx = context({ clipboard: CLIPBOARD, selection: [1, 2] });
    handleMenuAction("paste-task", ctx);
    expect(ctx.send).toHaveBeenCalledWith({ type: "paste_tasks", after_uid: 2, clipboard: CLIPBOARD });
  });

  it("does not paste before anything is copied", () => {
    const ctx = context();
    handleMenuAction("paste-task", ctx);
    expect(ctx.send).not.toHaveBeenCalled();
  });

  it("fills the focused column from the first row down", () => {
    const ctx = context({ selection: [1, 2], fillField: "name" });
    handleMenuAction("fill-down", ctx);
    expect(ctx.send).toHaveBeenCalledWith({ type: "fill_down", from_uid: 1, to_uids: [2], field: "name" });
  });

  it("does not fill down from a column it cannot fill", () => {
    const ctx = context({ selection: [1, 2], fillField: null });
    handleMenuAction("fill-down", ctx);
    expect(ctx.send).not.toHaveBeenCalled();
  });

  it("needs more than one row to fill down", () => {
    const ctx = context();
    handleMenuAction("fill-down", ctx);
    expect(ctx.send).not.toHaveBeenCalled();
  });

  it("copies through the backend", () => {
    const ctx = context();
    handleMenuAction("copy-task", ctx);
    expect(ctx.run).toHaveBeenCalledTimes(1);
  });
});

describe("handleMenuAction", () => {
  it("switches views, scales, language and theme", () => {
    const ctx = context();
    handleMenuAction("view-network", ctx);
    handleMenuAction("scale-week", ctx);
    handleMenuAction("lang-en", ctx);
    handleMenuAction("theme-dark", ctx);
    handleMenuAction("theme-system", ctx);
    handleMenuAction("theme-sepia", ctx);
    expect(ctx.setView).toHaveBeenCalledWith("network");
    expect(ctx.setScale).toHaveBeenCalledWith("week");
    expect(ctx.changeLang).toHaveBeenCalledWith("en");
    expect(vi.mocked(ctx.changeTheme).mock.calls).toEqual([["dark"], ["system"]]);
  });

  it("zooms the timeline in, out and back", () => {
    const ctx = context();
    handleMenuAction("zoom-in", ctx);
    handleMenuAction("zoom-out", ctx);
    handleMenuAction("zoom-reset", ctx);
    expect(vi.mocked(ctx.zoomTimeline).mock.calls).toEqual([[1], [-1], [0]]);
  });

  it("opens EVM beside a Gantt view", () => {
    const ctx = context({ view: "resources" });
    handleMenuAction("view-evm", ctx);
    const update = vi.mocked(ctx.setView).mock.calls[0][0] as (view: string) => string;
    expect([update("resources"), update("tracking")]).toEqual(["gantt", "tracking"]);
    expect(ctx.setEvmOpen).toHaveBeenCalledWith(true);
  });

  it("runs file commands through the file dialog and recent files by index", () => {
    const ctx = context();
    for (const id of ["new", "open", "save", "save-as", "import-xml", "export-xml", "export-excel", "compact-history", "recent:3"]) {
      handleMenuAction(id, ctx);
    }
    expect(ctx.startFileDialog).toHaveBeenCalledTimes(9);
  });

  it("opens the license dialog when exporting excel without pro license", () => {
    const ctx = context({ reportingPro: false });
    handleMenuAction("export-excel", ctx);
    expect(ctx.openLicense).toHaveBeenCalledOnce();
    expect(ctx.startFileDialog).not.toHaveBeenCalled();
  });

  it("opens the file dialog when exporting excel with pro license", () => {
    const ctx = context({ reportingPro: true });
    handleMenuAction("export-excel", ctx);
    expect(ctx.openLicense).not.toHaveBeenCalled();
    expect(ctx.startFileDialog).toHaveBeenCalledOnce();
  });

  it("opens the table import", () => {
    const ctx = context();
    handleMenuAction("import-table", ctx);
    expect(ctx.openImportTable).toHaveBeenCalledOnce();
  });

  it("opens the tracker pull only with a project and no file dialog open", () => {
    const ctx = context();
    handleMenuAction("pull-tracker", ctx);
    expect(ctx.openTracker).toHaveBeenCalledOnce();
    const busy = context({ fileDialogBusy: true });
    handleMenuAction("pull-tracker", busy);
    expect(busy.openTracker).not.toHaveBeenCalled();
    const empty = context({ snap: null });
    handleMenuAction("pull-tracker", empty);
    expect(empty.openTracker).not.toHaveBeenCalled();
  });

  it("opens the tracker push only with a project and no file dialog open", () => {
    const ctx = context();
    handleMenuAction("push-tracker", ctx);
    expect(ctx.openTrackerPush).toHaveBeenCalledOnce();
    const busy = context({ fileDialogBusy: true });
    handleMenuAction("push-tracker", busy);
    expect(busy.openTrackerPush).not.toHaveBeenCalled();
    const empty = context({ snap: null });
    handleMenuAction("push-tracker", empty);
    expect(empty.openTrackerPush).not.toHaveBeenCalled();
  });

  it("opens the tracker sync settings only with a project and no file dialog open", () => {
    const ctx = context();
    handleMenuAction("tracker-settings", ctx);
    expect(ctx.openTrackerSettings).toHaveBeenCalledOnce();
    const busy = context({ fileDialogBusy: true });
    handleMenuAction("tracker-settings", busy);
    expect(busy.openTrackerSettings).not.toHaveBeenCalled();
    const empty = context({ snap: null });
    handleMenuAction("tracker-settings", empty);
    expect(empty.openTrackerSettings).not.toHaveBeenCalled();
  });

  it("stops undo at the state Thông tin dự án opened with", () => {
    const at = context({ infoStart: 0 });
    handleMenuAction("undo", at);
    expect(at.run).not.toHaveBeenCalled();
    const snap = { ...makeSnapshot(), history_cursor: 3 };
    const past = context({ infoStart: 1, snap });
    handleMenuAction("undo", past);
    expect(past.run).toHaveBeenCalledOnce();
  });

  it("acts on the selected task only when the toolbar would allow it", () => {
    const ctx = context();
    handleMenuAction("indent", ctx);
    handleMenuAction("delete", ctx);
    handleMenuAction("split-task", ctx);
    expect(vi.mocked(ctx.send).mock.calls).toEqual([[{ type: "indent", uid: 2 }], [{ type: "delete_task", uid: 2 }]]);
    expect(ctx.openSplit).toHaveBeenCalledWith(2);
    // The first task has nothing above it to indent under.
    const first = context({ task: makeTask({ uid: 1 }) });
    handleMenuAction("indent", first);
    expect(first.send).not.toHaveBeenCalled();
    const none = context({ task: null });
    handleMenuAction("delete", none);
    expect(none.send).not.toHaveBeenCalled();
  });
});

describe("working time", () => {
  it("opens Lịch làm việc from the menu", () => {
    const ctx = context();
    handleMenuAction("working-time", ctx);
    expect(ctx.openWorkingTime).toHaveBeenCalled();
  });
});

describe("custom fields", () => {
  it("opens Thông tin dự án on its custom fields tab", () => {
    const ctx = context();
    handleMenuAction("custom-fields", ctx);
    expect(ctx.openCustomFields).toHaveBeenCalled();
    expect(ctx.openProjectInfo).not.toHaveBeenCalled();
  });

  it("opens Thông tin dự án on its leveling tab", () => {
    const ctx = context();
    handleMenuAction("leveling-order", ctx);
    expect(ctx.openLevelingOrder).toHaveBeenCalled();
    expect(ctx.openProjectInfo).not.toHaveBeenCalled();
  });

  it("previews the suggestions only when the project has rules", () => {
    const ctx = context();
    handleMenuAction("assign-by-rules", ctx);
    expect(ctx.openAssignByRules).not.toHaveBeenCalled();
    const rule = { conditions: [], target: { type: "resource" as const, uid: 1 }, units: 1 };
    const ruled = context({ snap: makeSnapshot(makeProject({ assignment_rules: [rule] })) });
    handleMenuAction("assign-by-rules", ruled);
    expect(ruled.openAssignByRules).toHaveBeenCalled();
  });

  it("opens the assignment rules", () => {
    const ctx = context();
    handleMenuAction("assignment-rules", ctx);
    expect(ctx.openAssignmentRules).toHaveBeenCalled();
  });

  it("fills a custom field column down", () => {
    const ctx = context({ selection: [1, 2, 3], fillField: { field: 4 } });
    handleMenuAction("fill-down", ctx);
    expect(ctx.send).toHaveBeenCalledWith({ type: "fill_down", from_uid: 1, to_uids: [2, 3], field: { field: 4 } });
  });
});

describe("Help menu", () => {
  it("opens the license dialog", () => {
    const ctx = context();
    handleMenuAction("license", ctx);
    expect(ctx.openLicense).toHaveBeenCalled();
  });

  it("opens the AI recommendations settings", () => {
    const ctx = context();
    handleMenuAction("ai-settings", ctx);
    expect(ctx.openAiSettings).toHaveBeenCalledOnce();
  });
});
