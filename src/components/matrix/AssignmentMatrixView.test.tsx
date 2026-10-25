import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { describe, expect, it, vi } from "vitest";

import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeResource } from "../../test/fixtures";
import type { AssignmentMatrix } from "../../types";
import { AssignmentMatrixView } from "./AssignmentMatrixView";

function makeMatrixData(): AssignmentMatrix {
  return {
    columns: [
      { id: "10", label: "An", group: "Backend", resource_uid: 10, total_work: 480 },
      { id: "11", label: "Bình", group: "Backend", resource_uid: 11, total_work: 480 },
    ],
    roles: [
      { name: "Backend", resource_uids: [10, 11], total_work: 960 },
    ],
    rows: [
      {
        task_uid: 1,
        task_id: 1,
        task_name: "Phase 1",
        outline_level: 0,
        summary: true,
        cells: [
          { units: 1.0, work: 480 },
          { units: 0.5, work: 480 },
        ],
        role_cells: [
          { units: 1.5, work: 960 },
        ],
        unassigned: undefined,
        total_work: 960,
      },
      {
        task_uid: 2,
        task_id: 2,
        task_name: "Subtask 1",
        outline_level: 1,
        summary: false,
        cells: [
          { assignment_uid: 101, units: 1.0, work: 480 },
          null,
        ],
        role_cells: [
          { units: 1.0, work: 480 },
        ],
        unassigned: undefined,
        total_work: 480,
      },
      {
        task_uid: 3,
        task_id: 3,
        task_name: "Subtask 2",
        outline_level: 1,
        summary: false,
        cells: [
          null,
          null,
        ],
        role_cells: [
          null,
        ],
        unassigned: {
          rule_index: 0,
          target: { type: "skill", id: 1, min_level: 1 },
        },
        total_work: 0,
      },
    ],
    total_work: 960,
  };
}

describe("AssignmentMatrixView", () => {
  it("renders tasks, headers, role groups and summary rollups", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
      skills: [{ id: 1, name: "Backend" }],
      skill_levels: [{ id: 1, name: "Senior" }],
    });

    render(
      <AssignmentMatrixView
        project={project}
        group={false}
        onAssign={vi.fn()}
        onUpdate={vi.fn()}
        onRemove={vi.fn()}
      />
    );

    // Tasks rendered
    expect(await screen.findByText("Phase 1")).toBeInTheDocument();
    expect(screen.getByText("Subtask 1")).toBeInTheDocument();
    expect(screen.getByText("Subtask 2")).toBeInTheDocument();

    // Headers
    expect(screen.getByText("An")).toBeInTheDocument();
    expect(screen.getByText("Bình")).toBeInTheDocument();

    // Unassigned rule tag
    expect(screen.getByText("Backend:Senior")).toBeInTheDocument();

    // Role group header
    expect(screen.getByText(/Vai trò: Backend \(2\)/)).toBeInTheDocument();
  });

  it("collapses and expands a role group in person mode", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
    });

    render(
      <AssignmentMatrixView
        project={project}
        group={false}
        onAssign={vi.fn()}
        onUpdate={vi.fn()}
        onRemove={vi.fn()}
      />
    );

    const collapseBtn = await screen.findByRole("button", { name: "Thu gọn" });
    await userEvent.click(collapseBtn);

    // An and Bình person headers are now hidden in favor of single collapsed Backend header
    expect(screen.queryByText("An")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mở rộng" })).toBeInTheDocument();

    // Expand again
    await userEvent.click(screen.getByRole("button", { name: "Mở rộng" }));
    expect(screen.getByText("An")).toBeInTheDocument();
  });

  it("edits an assignment in place and updates units", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onUpdate = vi.fn();
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
    });

    render(
      <AssignmentMatrixView
        project={project}
        group={false}
        onAssign={vi.fn()}
        onUpdate={onUpdate}
        onRemove={vi.fn()}
      />
    );

    // Find An's cell in Subtask 1 (has 1.0 (8h))
    const cellWork = await screen.findByText("(8h)");
    await userEvent.click(cellWork);

    const input = screen.getByRole("textbox");
    expect(input).toHaveValue("1");

    await userEvent.clear(input);
    await userEvent.type(input, "0,5{Enter}");

    expect(onUpdate).toHaveBeenCalledWith(101, 0.5);
  });

  it("removes assignment when cleared or set to 0", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onRemove = vi.fn();
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
    });

    render(
      <AssignmentMatrixView
        project={project}
        group={false}
        onAssign={vi.fn()}
        onUpdate={vi.fn()}
        onRemove={onRemove}
      />
    );

    const cellWork = await screen.findByText("(8h)");
    await userEvent.click(cellWork);

    const input = screen.getByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, "{Enter}");

    expect(onRemove).toHaveBeenCalledWith(101);
  });

  it("assigns resource when clicking on unassigned leaf cell", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onAssign = vi.fn();
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
    });

    render(
      <AssignmentMatrixView
        project={project}
        group={false}
        onAssign={onAssign}
        onUpdate={vi.fn()}
        onRemove={vi.fn()}
      />
    );

    // Find the blank cell '+' in Subtask 1 for Bình
    const blanks = await screen.findAllByText("+");
    await userEvent.click(blanks[0]);

    const input = screen.getByRole("textbox");
    await userEvent.type(input, "0,75{Enter}");

    expect(onAssign).toHaveBeenCalledWith(2, 11, 0.75);
  });

  it("renders read-only role mode when group is true", async () => {
    fakeBackend.assignmentMatrix = {
      columns: [
        { id: "Backend", label: "Backend", group: "Backend", resource_uid: undefined, total_work: 960 },
      ],
      roles: [
        { name: "Backend", resource_uids: [10, 11], total_work: 960 },
      ],
      rows: [
        {
          task_uid: 1,
          task_id: 1,
          task_name: "Phase 1",
          outline_level: 0,
          summary: true,
          cells: [{ units: 1.5, work: 960 }],
          role_cells: [{ units: 1.5, work: 960 }],
          unassigned: undefined,
          total_work: 960,
        },
      ],
      total_work: 960,
    };

    const project = makeProject();

    render(
      <AssignmentMatrixView
        project={project}
        group={true}
        onAssign={vi.fn()}
        onUpdate={vi.fn()}
        onRemove={vi.fn()}
      />
    );

    expect(await screen.findByText("Phase 1")).toBeInTheDocument();
    expect(screen.getByText("Backend")).toBeInTheDocument();
    expect(screen.getByText("1,5u")).toBeInTheDocument();
    expect(screen.getAllByText("16h").length).toBeGreaterThanOrEqual(1);
  });

  type Handlers = Pick<ComponentProps<typeof AssignmentMatrixView>, "onAssign" | "onUpdate" | "onRemove">;

  function renderPersonMode(handlers: Partial<Handlers> = {}) {
    const project = makeProject({
      resources: [
        makeResource({ uid: 10, name: "An", group: "Backend" }),
        makeResource({ uid: 11, name: "Bình", group: "Backend" }),
      ],
    });
    const props = {
      project,
      group: false,
      onAssign: handlers.onAssign ?? vi.fn(),
      onUpdate: handlers.onUpdate ?? vi.fn(),
      onRemove: handlers.onRemove ?? vi.fn(),
    };
    return { ...render(<AssignmentMatrixView {...props} />), props };
  }

  it("moves between cells with the arrow keys and edits with Enter and Escape", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onUpdate = vi.fn();
    renderPersonMode({ onUpdate });
    await screen.findByText("Phase 1");

    const cells = screen.getAllByRole("gridcell").filter((cell) => cell.hasAttribute("data-cell"));
    // One tab stop: the first value cell.
    expect(cells.filter((cell) => cell.tabIndex === 0)).toEqual([cells[0]]);
    cells[0].focus();

    await userEvent.keyboard("{ArrowDown}");
    const an = document.querySelector<HTMLElement>('[data-cell="1:0"]')!;
    expect(an).toHaveFocus();
    expect(an.tabIndex).toBe(0);
    await userEvent.keyboard("{ArrowRight}");
    expect(document.querySelector('[data-cell="1:1"]')).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(an).toHaveFocus();

    await userEvent.keyboard("{Enter}");
    const input = screen.getByRole("textbox");
    expect(input).toHaveValue("1");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(an).toHaveFocus();
    expect(onUpdate).not.toHaveBeenCalled();

    await userEvent.keyboard("{Enter}");
    await userEvent.clear(screen.getByRole("textbox"));
    await userEvent.type(screen.getByRole("textbox"), "50%{Enter}");
    // Enter commits once; the blur as the field goes away does not send it again.
    expect(onUpdate).toHaveBeenCalledTimes(1);
    expect(onUpdate).toHaveBeenCalledWith(101, 0.5);
    expect(document.querySelector('[data-cell="1:0"]')).toHaveFocus();
  });

  it.each(["0", "0,0", "0.0", "0%", ""])("removes the assignment for %j", async (typed) => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onRemove = vi.fn();
    const onUpdate = vi.fn();
    renderPersonMode({ onRemove, onUpdate });
    await userEvent.click(await screen.findByText("(8h)"));
    const input = screen.getByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, `${typed}{Enter}`);
    expect(onRemove).toHaveBeenCalledTimes(1);
    expect(onRemove).toHaveBeenCalledWith(101);
    expect(onUpdate).not.toHaveBeenCalled();
  });

  it("keeps text that does not read open and marked instead of dropping it", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const handlers = { onAssign: vi.fn(), onUpdate: vi.fn(), onRemove: vi.fn() };
    renderPersonMode(handlers);
    await userEvent.click(await screen.findByText("(8h)"));
    const input = screen.getByRole("textbox");
    await userEvent.clear(input);
    await userEvent.type(input, "abc{Enter}");

    expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Không đọc được số đơn vị");
    expect(handlers.onUpdate).not.toHaveBeenCalled();
    expect(handlers.onRemove).not.toHaveBeenCalled();
    expect(handlers.onAssign).not.toHaveBeenCalled();

    await userEvent.type(screen.getByRole("textbox"), "{Backspace}{Backspace}{Backspace}2");
    expect(screen.getByRole("textbox")).toHaveAttribute("aria-invalid", "false");
    await userEvent.keyboard("{Enter}");
    expect(handlers.onUpdate).toHaveBeenCalledWith(101, 2);
  });

  it("removes a summary row's own assignment with its button or Delete", async () => {
    const data = makeMatrixData();
    data.rows[0].cells[0] = { assignment_uid: 900, units: 1, work: 960 };
    fakeBackend.assignmentMatrix = data;
    const onRemove = vi.fn();
    renderPersonMode({ onRemove });

    await userEvent.click(await screen.findByRole("button", { name: "Gỡ phân công riêng của “Phase 1”" }));
    expect(onRemove).toHaveBeenCalledWith(900);

    const summaryAn = document.querySelector<HTMLElement>('[data-cell="0:0"]')!;
    summaryAn.focus();
    await userEvent.keyboard("{Delete}");
    expect(onRemove).toHaveBeenCalledTimes(2);
    // Enter does not edit a summary row.
    await userEvent.keyboard("{Enter}");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    // A summary without an assignment of its own has no button.
    expect(screen.getAllByRole("button", { name: /Gỡ phân công riêng/ })).toHaveLength(1);
  });

  it("draws nothing of the old mode while the other mode loads", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    const onAssign = vi.fn();
    const { rerender, props } = renderPersonMode({ onAssign });
    await screen.findByText("Phase 1");

    fakeBackend.assignmentMatrix = {
      columns: [{ id: "Backend", label: "Backend", group: "Backend", total_work: 960 }],
      roles: [{ name: "Backend", resource_uids: [10, 11], total_work: 960 }],
      rows: [],
      total_work: 960,
    };

    rerender(<AssignmentMatrixView {...props} group={true} />);
    expect(screen.getByRole("status")).toHaveTextContent("Đang tải ma trận");
    expect(screen.queryByText("+")).not.toBeInTheDocument();
    expect(screen.queryByText("Phase 1")).not.toBeInTheDocument();

    // The read answers after the switch was drawn.
    expect(await screen.findByRole("columnheader", { name: /Backend/ })).toBeInTheDocument();
    expect(onAssign).not.toHaveBeenCalled();
  });

  it("says why the matrix did not load", async () => {
    fakeBackend.failNext("assignment_matrix", "Hỏng rồi");
    renderPersonMode();
    expect(await screen.findByRole("alert")).toHaveTextContent("Không tải được ma trận phân công: Hỏng rồi");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("starts an edit from a click without losing the cell's grid position", async () => {
    fakeBackend.assignmentMatrix = makeMatrixData();
    renderPersonMode();
    const blanks = await screen.findAllByText("+");
    fireEvent.click(blanks[0]);
    expect(screen.getByRole("textbox")).toBeInTheDocument();
    expect(document.querySelector('[data-cell="1:1"]')).toHaveAttribute("tabindex", "0");
  });
});
