import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { makeProject, makeTask } from "../test/fixtures";
import type { Project, ViewId } from "../types";
import { ViewBar } from "./ViewBar";

function renderBar(view: ViewId, project: Project = makeProject(), overrides: Partial<Parameters<typeof ViewBar>[0]> = {}) {
  const props = {
    project,
    view,
    onView: vi.fn(),
    scale: "day" as const,
    onScale: vi.fn(),
    filter: "none" as const,
    onFilter: vi.fn(),
    filterResource: null,
    onFilterResource: vi.fn(),
    filterValue: "",
    onFilterValue: vi.fn(),
    group: "none" as const,
    onGroup: vi.fn(),
    highlight: "none" as const,
    onHighlight: vi.fn(),
    networkSummaries: false,
    onNetworkSummaries: vi.fn(),
    interruptsColumn: false,
    onInterruptsColumn: vi.fn(),
    spanEnd: { kind: "date" as const, date: "" },
    onSpanEnd: vi.fn(),
    ...overrides,
  };
  render(<ViewBar {...props} />);
  return { user: userEvent.setup(), props };
}

describe("ViewBar", () => {
  it("shows the views as tabs with one tab stop on the current one", async () => {
    const { user, props } = renderBar("gantt");
    expect(screen.getByRole("tablist", { name: "View" })).toBeInTheDocument();
    const gantt = screen.getByRole("tab", { name: "Gantt" });
    expect(gantt).toHaveAttribute("aria-selected", "true");
    expect(gantt).toHaveAttribute("aria-controls", "view-panel");
    expect(gantt).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "Theo dõi" })).toHaveAttribute("tabindex", "-1");
    await user.click(screen.getByRole("tab", { name: "Sơ đồ mạng" }));
    expect(props.onView).toHaveBeenCalledWith("network");
  });

  it("marks the Pro views with a badge that joins the tab's name", () => {
    renderBar("gantt");
    expect(screen.getByRole("tab", { name: "Ma trận Pro" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Dòng tiền Pro" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "So sánh Pro" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Tải" })).toBeInTheDocument();
    expect(screen.getAllByTitle("Tính năng Sheafly Pro")).toHaveLength(3);
  });

  it("moves between tabs with the arrow keys, Home and End", async () => {
    const { user, props } = renderBar("gantt");
    screen.getByRole("tab", { name: "Gantt" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(props.onView).toHaveBeenLastCalledWith("tracking");
    await user.keyboard("{ArrowLeft}");
    expect(props.onView).toHaveBeenLastCalledWith("network");
    await user.keyboard("{End}");
    expect(props.onView).toHaveBeenLastCalledWith("network");
    await user.keyboard("{Home}");
    expect(props.onView).toHaveBeenLastCalledWith("gantt");
  });

  it("shows the time scale and row controls on the chart views only", async () => {
    const { user, props } = renderBar("tracking");
    await user.click(screen.getByRole("button", { name: "Tuần" }));
    expect(props.onScale).toHaveBeenCalledWith("week");
    expect(screen.getByLabelText("Nhóm")).toBeInTheDocument();
  });

  it("swaps grouping for summaries on the network and hides controls elsewhere", () => {
    renderBar("network");
    expect(screen.queryByRole("group", { name: "Thang thời gian" })).toBeNull();
    expect(screen.getByLabelText("Hiện công việc tóm tắt")).toBeInTheDocument();
  });

  it("filters, highlights and shows a column for urgent tasks", async () => {
    const { user, props } = renderBar("gantt");
    await user.selectOptions(screen.getByLabelText("Lọc"), "interrupts");
    expect(props.onFilter).toHaveBeenCalledWith("interrupts");
    await user.selectOptions(screen.getByLabelText("Tô sáng"), "Việc chen ngang");
    expect(props.onHighlight).toHaveBeenCalledWith("interrupts");
    await user.click(screen.getByRole("checkbox", { name: "Cột chen ngang" }));
    expect(props.onInterruptsColumn).toHaveBeenCalledWith(true);
  });

  it("has no row controls on the resource sheet", () => {
    renderBar("resources");
    expect(screen.queryByLabelText("Lọc")).toBeNull();
  });
});

describe("ViewBar custom fields", () => {
  const project = makeProject({
    fields: [{ id: 2, name: "Mảng", kind: { type: "choice", options: ["FE", "BE"] } }],
    tasks: [makeTask({ uid: 1, values: { 2: "BE" } }), makeTask({ uid: 2, values: { 2: "FE" } })],
  });

  it("filters by a field, starting on the first value tasks have", async () => {
    const { user, props } = renderBar("gantt", project);
    await user.selectOptions(screen.getByRole("combobox", { name: "Lọc" }), "Theo “Mảng”");
    expect(props.onFilter).toHaveBeenCalledWith("field:2");
    expect(props.onFilterValue).toHaveBeenCalledWith("FE");
  });

  it("picks the value to keep, or no value", async () => {
    const { user, props } = renderBar("gantt", project, { filter: "field:2", filterValue: "FE" });
    const values = screen.getByRole("combobox", { name: "Giá trị" });
    expect([...values.querySelectorAll("option")].map((option) => option.textContent)).toEqual(["FE", "BE", "(trống)"]);
    await user.selectOptions(values, "(trống)");
    expect(props.onFilterValue).toHaveBeenCalledWith("");
  });

  it("groups by a field", async () => {
    const { user, props } = renderBar("gantt", project);
    await user.selectOptions(screen.getByRole("combobox", { name: "Nhóm" }), "Theo “Mảng”");
    expect(props.onGroup).toHaveBeenCalledWith("field:2");
  });

  it("asks where the span ends once it filters or highlights by it", async () => {
    const project = makeProject({
      tasks: [
        makeTask({ uid: 1, finish: Date.UTC(2026, 0, 9, 17) }),
        makeTask({ uid: 2, name: "Bàn giao", milestone: true, finish: Date.UTC(2026, 0, 20, 17) }),
      ],
    });
    const { user, props } = renderBar("gantt", project);
    expect(screen.queryByRole("combobox", { name: "Điểm cuối" })).not.toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Lọc" }), "Không vừa khoảng");
    expect(props.onFilter).toHaveBeenCalledWith("span");
    // An empty end starts at the last finish, so the filter shows something sensible.
    expect(props.onSpanEnd).toHaveBeenCalledWith({ kind: "date", date: "2026-01-20" });
  });

  it("ends the span at a date or a milestone", async () => {
    const project = makeProject({
      tasks: [makeTask({ uid: 2, name: "Bàn giao", milestone: true, finish: Date.UTC(2026, 0, 20, 17) })],
    });
    const { user, props } = renderBar("gantt", project, { highlight: "span", spanEnd: { kind: "date", date: "2026-01-16" } });
    expect(screen.getByLabelText("Tới ngày")).toHaveValue("2026-01-16");
    await user.selectOptions(screen.getByRole("combobox", { name: "Điểm cuối" }), "Bàn giao");
    expect(props.onSpanEnd).toHaveBeenCalledWith({ kind: "milestone", uid: 2 });
    // Back to a date keeps the one typed before.
    await user.selectOptions(screen.getByRole("combobox", { name: "Điểm cuối" }), "Một ngày");
    expect(props.onSpanEnd).toHaveBeenLastCalledWith({ kind: "date", date: "2026-01-16" });
  });

  it("shows a zoomed timeline as a percentage that resets it", async () => {
    const onZoomReset = vi.fn();
    const { user } = renderBar("gantt", makeProject(), { zoom: 1.5625, onZoomReset });
    await user.click(screen.getByRole("button", { name: "156% · về cỡ gốc" }));
    expect(onZoomReset).toHaveBeenCalled();
  });

  it("shows no zoom control at the scale's own size", () => {
    renderBar("gantt", makeProject(), { zoom: 1, onZoomReset: vi.fn() });
    expect(screen.queryByRole("button", { name: /về cỡ gốc/ })).not.toBeInTheDocument();
  });
});

describe("ViewBar field columns", () => {
  const withFields = () =>
    makeProject({
      fields: [
        { id: 1, name: "Khách hàng", kind: { type: "text" } },
        { id: 2, name: "Điểm", kind: { type: "number" } },
      ] as Project["fields"],
    });

  it("lists each custom field with a checkbox and reports the toggle", async () => {
    const onFieldColumn = vi.fn();
    const { user } = renderBar("gantt", withFields(), { hiddenFields: new Set([2]), onFieldColumn });
    await user.click(screen.getByText("Cột trường (1/2)"));
    expect(screen.getByRole("checkbox", { name: "Khách hàng" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Điểm" })).not.toBeChecked();
    await user.click(screen.getByRole("checkbox", { name: "Điểm" }));
    expect(onFieldColumn).toHaveBeenCalledWith(2, true);
    await user.click(screen.getByRole("checkbox", { name: "Khách hàng" }));
    expect(onFieldColumn).toHaveBeenCalledWith(1, false);
  });

  it("has no picker without custom fields or outside the Gantt views", () => {
    renderBar("gantt", makeProject(), { onFieldColumn: vi.fn() });
    expect(screen.queryByText(/Cột trường/)).not.toBeInTheDocument();
  });
});
