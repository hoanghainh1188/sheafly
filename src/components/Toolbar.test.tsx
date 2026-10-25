import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { makeProject, makeSnapshot, makeTask } from "../test/fixtures";
import { toolbarStates } from "../toolbarState";
import { Toolbar, type ToolbarActions } from "./Toolbar";

function actions(): ToolbarActions {
  return {
    onUndo: vi.fn(),
    onRedo: vi.fn(),
    onAddTask: vi.fn(),
    onAddMilestone: vi.fn(),
    onAddRecurring: vi.fn(),
    onIndent: vi.fn(),
    onOutdent: vi.fn(),
    onLink: vi.fn(),
    onUnlink: vi.fn(),
    onSplit: vi.fn(),
    onDelete: vi.fn(),
    onProjectInfo: vi.fn(),
    onAssignByRules: vi.fn(),
    onToggleEvm: vi.fn(),
    onToggleAi: vi.fn(),
    onExportPdf: vi.fn(),
  };
}

const first = makeTask({ uid: 1 });
const second = makeTask({ uid: 2 });
const snap = { ...makeSnapshot(makeProject({ tasks: [first, second] })), can_undo: true };

describe("Toolbar", () => {
  it("assigns by rules from the project group, or says why it cannot", async () => {
    const handlers = actions();
    const rule = { conditions: [], target: { type: "resource" as const, uid: 1 }, units: 1 };
    const ruled = makeSnapshot(makeProject({ tasks: [first], assignment_rules: [rule] }));
    const { rerender } = render(
      <Toolbar states={toolbarStates({ snap, task: null, view: "gantt", fileDialogBusy: false })} evmOpen={false} aiOpen={false} {...handlers} />,
    );
    const button = screen.getByRole("button", { name: "Giao theo quy tắc…" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Dự án chưa có quy tắc giao việc; tạo ở Dự án → Quy tắc giao việc…");
    rerender(<Toolbar states={toolbarStates({ snap: ruled, task: null, view: "gantt", fileDialogBusy: false })} evmOpen={false} aiOpen={false} {...handlers} />);
    await userEvent.setup().click(screen.getByRole("button", { name: "Giao theo quy tắc…" }));
    expect(handlers.onAssignByRules).toHaveBeenCalled();
  });

  it("groups the commands and disables those that cannot run, saying why", () => {
    render(<Toolbar states={toolbarStates({ snap, task: null, view: "gantt", fileDialogBusy: false })} evmOpen={false} aiOpen={false} {...actions()} />);
    for (const group of ["Lịch sử", "Tạo công việc", "Công việc đang chọn", "Dự án", "Báo cáo"]) {
      expect(screen.getByRole("group", { name: group })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "Hoàn tác" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Làm lại" })).toHaveAttribute("title", "Không còn gì để làm lại");
    expect(screen.getByRole("button", { name: "Xóa" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Xóa" })).toHaveAttribute("title", "Chọn một công việc trước");
  });

  it("links the selected task to the one above with the chosen type", async () => {
    const user = userEvent.setup();
    const handlers = actions();
    render(<Toolbar states={toolbarStates({ snap, task: second, view: "gantt", fileDialogBusy: false })} evmOpen aiOpen={false} {...handlers} />);
    await user.click(screen.getByRole("button", { name: /Liên kết/ }));
    expect(screen.getByRole("menuitem", { name: "Gỡ liên kết với việc liền trước" })).toBeDisabled();
    await user.click(screen.getByRole("menuitem", { name: "Bắt đầu → bắt đầu (SS)" }));
    expect(handlers.onLink).toHaveBeenCalledWith("ss");
    expect(screen.getByRole("button", { name: "EVM" })).toHaveAttribute("aria-pressed", "true");
  });

  it("adds a milestone from the Add menu", async () => {
    const user = userEvent.setup();
    const handlers = actions();
    render(<Toolbar states={toolbarStates({ snap, task: null, view: "gantt", fileDialogBusy: false })} evmOpen={false} aiOpen={false} {...handlers} />);
    await user.click(screen.getByRole("button", { name: /Thêm việc/ }));
    await user.click(screen.getByRole("menuitem", { name: "Mốc" }));
    expect(handlers.onAddMilestone).toHaveBeenCalledOnce();
  });

  it("toggles the AI panel from the reports group, marked Pro", async () => {
    const handlers = actions();
    render(<Toolbar states={toolbarStates({ snap, task: null, view: "gantt", fileDialogBusy: false })} evmOpen={false} aiOpen {...handlers} />);
    const button = screen.getByRole("button", { name: "Gợi ý AI Pro" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    await userEvent.setup().click(button);
    expect(handlers.onToggleAi).toHaveBeenCalledOnce();
  });
});
