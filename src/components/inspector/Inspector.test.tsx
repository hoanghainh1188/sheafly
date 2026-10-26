import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider, setCurrentLang } from "../../i18n";
import { at, makeAssignment, makeProject, makeResource, makeTask } from "../../test/fixtures";
import type { Project, Task } from "../../types";
import { Inspector } from "./Inspector";

const OPEN_KEY = "sheafly.inspector.open";

function renderInspector(task: Task, project: Project = makeProject({ tasks: [task] })) {
  const send = vi.fn();
  const utils = render(
    <Inspector
      project={project}
      task={task}
      send={send}
      onEditSeries={vi.fn()}
      onSplit={vi.fn()}
      onEditCalendar={vi.fn()}
      onCreateCalendar={vi.fn()}
      onOpenPayments={vi.fn()}
    />,
  );
  return { ...utils, send, user: userEvent.setup() };
}

const leaf = makeTask({ uid: 2, name: "Thiết kế giao diện", wbs: "2.1", duration: 960 });
const summary = makeTask({ uid: 1, name: "Thiết kế", wbs: "2", summary: true });

describe("Inspector", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("says which task it edits", () => {
    renderInspector(leaf);
    const panel = screen.getByRole("region", { name: "Thuộc tính công việc" });
    expect(within(panel).getByRole("heading", { name: "Thiết kế giao diện" })).toBeInTheDocument();
    expect(within(panel).getByText(/ID 2 · WBS 2\.1/)).toBeInTheDocument();
  });

  it("groups the fields", () => {
    renderInspector(leaf);
    for (const name of ["Lịch", "Theo dõi", "Chi phí", "Nguồn lực", "Ghi chú"]) {
      expect(screen.getByRole("group", { name })).toBeInTheDocument();
    }
    expect(within(screen.getByRole("group", { name: "Theo dõi" })).getByLabelText("Bắt đầu thực")).toBeInTheDocument();
  });

  it("marks a leaf as urgent with a note on what that does", async () => {
    const { send, user } = renderInspector(leaf);
    const group = screen.getByRole("group", { name: "Lịch" });
    const box = within(group).getByRole("checkbox", { name: "Chen ngang" });
    expect(box).not.toBeChecked();
    expect(box).toHaveAccessibleDescription(/làm trước việc đang dở/);
    await user.click(box);
    expect(send).toHaveBeenCalledWith({ type: "set_interrupts", uid: 2, interrupts: true });
  });

  it("offers the interrupt flag only on leaves that are not milestones", () => {
    renderInspector(summary);
    expect(screen.queryByRole("checkbox", { name: "Chen ngang" })).not.toBeInTheDocument();
    renderInspector(makeTask({ uid: 3, name: "Mốc", milestone: true, duration: 0 }));
    expect(screen.queryByRole("checkbox", { name: "Chen ngang" })).not.toBeInTheDocument();
  });

  it("hides fields that do not apply to a summary task", () => {
    renderInspector(summary);
    expect(screen.queryByLabelText("Bắt đầu thực")).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Theo dõi" })).not.toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Nguồn lực" })).not.toBeInTheDocument();
    expect(screen.queryByText("Kiểu công việc")).not.toBeInTheDocument();
    expect(screen.getByText(/Công việc tóm tắt/)).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Lịch" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Chi phí" })).toBeInTheDocument();
  });

  it("collapses to its header and remembers it", async () => {
    const { user, unmount } = renderInspector(leaf);
    const toggle = screen.getByRole("button", { name: "Chi tiết thuộc tính" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    await user.click(toggle);
    expect(screen.queryByRole("group", { name: "Lịch" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Thiết kế giao diện" })).toBeInTheDocument();
    expect(window.localStorage.getItem(OPEN_KEY)).toBe("false");
    unmount();

    renderInspector(leaf);
    expect(screen.getByRole("button", { name: "Chi tiết thuộc tính" })).toHaveAttribute("aria-expanded", "false");
  });

  it("still edits assignments in the resources group", () => {
    const project = makeProject({
      tasks: [leaf],
      resources: [makeResource({ uid: 10, name: "An" })],
      assignments: [makeAssignment({ uid: 1, task_uid: 2, resource_uid: 10 })],
    });
    renderInspector(leaf, project);
    expect(within(screen.getByRole("group", { name: "Nguồn lực" })).getByLabelText("Đơn vị An")).toBeInTheDocument();
  });

  it("takes assignment units as hours per day and shows them as hours", async () => {
    const project = makeProject({
      tasks: [leaf],
      resources: [makeResource({ uid: 10, name: "An" }), makeResource({ uid: 11, name: "Bình" })],
      assignments: [makeAssignment({ uid: 1, task_uid: 2, resource_uid: 10, units: 0.5 })],
    });
    const { send, user } = renderInspector(leaf, project);
    const units = screen.getByLabelText("Đơn vị An");
    expect(units).toHaveValue("0,5");
    expect(screen.getByText("4 giờ/ngày")).toBeInTheDocument();
    await user.clear(units);
    await user.type(units, "2h/ngày");
    await user.tab();
    expect(send).toHaveBeenLastCalledWith({ type: "update_assignment", uid: 1, units: 0.25, work: null, overtime_work: null });

    // Text that is not units goes back to what the assignment has.
    await user.clear(units);
    await user.type(units, "nửa");
    await user.tab();
    expect(send).toHaveBeenCalledTimes(1);
    expect(units).toHaveValue("0,5");

    const add = screen.getByLabelText("Đơn vị gán");
    await user.clear(add);
    await user.type(add, "1,5 giờ/ngày");
    expect(screen.getByText("= 1,5 giờ/ngày")).toBeInTheDocument();
    await user.selectOptions(within(screen.getByRole("group", { name: "Nguồn lực" })).getByRole("combobox"), "11");
    expect(send).toHaveBeenLastCalledWith({ type: "assign_resource", task_uid: 2, resource_uid: 11, units: 0.1875 });
  });

  it("does not send anything when a duration field is left untouched", () => {
    for (const work of [0, 25, 45, 90, 540, 768, 12060]) {
      const { send, unmount } = renderInspector({ ...leaf, work });
      fireEvent.blur(screen.getByLabelText("Công việc"));
      expect(send).not.toHaveBeenCalled();
      unmount();
    }
  });

  it("shows the computed actual cost, locked, while costs are not editable", () => {
    renderInspector({ ...leaf, actual_cost: 1250 });
    const field = screen.getByLabelText("Chi phí thực tế");
    expect(field).toBeDisabled();
    expect(field).toHaveValue("1.250");
  });

  it("sends, and clears, a hand-entered actual cost when costs are editable", () => {
    const task = { ...leaf, manual_actual_cost: 300 };
    const { send } = renderInspector(task, makeProject({ tasks: [task], editable_actual_costs: true }));
    const field = screen.getByLabelText("Chi phí thực tế");
    expect(field).toBeEnabled();
    expect(field).toHaveValue("300");
    fireEvent.change(field, { target: { value: "450" } });
    fireEvent.blur(field);
    expect(send).toHaveBeenLastCalledWith({ type: "set_task_actual_cost", uid: 2, cost: 450 });
    fireEvent.change(field, { target: { value: "" } });
    fireEvent.blur(field);
    expect(send).toHaveBeenLastCalledWith({ type: "set_task_actual_cost", uid: 2, cost: null });
  });

  it("enters an assignment's actual cost only while costs are editable", () => {
    const task = { ...leaf };
    const resource = makeResource({ uid: 7, name: "An" });
    const assignment = makeAssignment({ uid: 3, task_uid: 2, resource_uid: 7, actual_cost: 400 });
    const off = makeProject({ tasks: [task], resources: [resource], assignments: [assignment] });
    const { unmount } = renderInspector(task, off);
    expect(screen.queryByLabelText("Chi phí thực tế An")).not.toBeInTheDocument();
    unmount();
    const { send } = renderInspector(task, { ...off, editable_actual_costs: true });
    const field = screen.getByLabelText("Chi phí thực tế An");
    expect(field).toHaveAttribute("placeholder", "400");
    fireEvent.change(field, { target: { value: "650" } });
    fireEvent.blur(field);
    expect(send).toHaveBeenLastCalledWith({ type: "set_assignment_actual_cost", uid: 3, cost: 650 });
    fireEvent.change(field, { target: { value: "abc" } });
    fireEvent.blur(field);
    expect(field).toHaveValue("");
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("keeps a summary task's actual cost computed even when costs are editable", () => {
    renderInspector(summary, makeProject({ tasks: [summary], editable_actual_costs: true }));
    expect(screen.getByLabelText("Chi phí thực tế")).toBeDisabled();
  });

  it("sends the same constraint command as before the move", () => {
    const { send } = renderInspector(leaf);
    fireEvent.change(screen.getByLabelText("Ràng buộc"), { target: { value: "snet" } });
    expect(send).toHaveBeenCalledWith({
      type: "set_constraint",
      uid: 2,
      constraint_type: "snet",
      date: leaf.constraint_date,
    });
  });
});

describe("Inspector in English", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });
  afterEach(() => setCurrentLang("vi"));

  function renderEnglish(task: Task, project: Project = makeProject({ tasks: [task] })) {
    const send = vi.fn();
    render(
      <I18nProvider lang="en">
        <Inspector
          project={project}
          task={task}
          send={send}
          onEditSeries={vi.fn()}
          onSplit={vi.fn()}
          onEditCalendar={vi.fn()}
          onCreateCalendar={vi.fn()}
          onOpenPayments={vi.fn()}
        />
      </I18nProvider>,
    );
    return { send };
  }

  it("groups and labels the fields in English", () => {
    renderEnglish(leaf);
    expect(screen.getByRole("region", { name: "Task information" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Task details" })).toHaveAttribute("title", "Collapse");
    for (const name of ["Schedule", "Tracking", "Cost", "Resources", "Notes"]) {
      expect(screen.getByRole("group", { name })).toBeInTheDocument();
    }
    for (const label of ["Actual start", "Actual finish", "Work", "Remaining work", "Overtime work", "Deadline", "Priority"]) {
      expect(screen.getByLabelText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("Manually scheduled")).toBeInTheDocument();
    expect(screen.getByText("Effort driven")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New task calendar" })).toBeInTheDocument();
  });

  it("uses Project's names for constraints, task types and accrual", () => {
    renderEnglish(leaf);
    const constraint = within(screen.getByLabelText("Constraint"));
    for (const name of [
      "As Soon As Possible",
      "As Late As Possible",
      "Start No Earlier Than",
      "Start No Later Than",
      "Finish No Earlier Than",
      "Finish No Later Than",
      "Must Start On",
      "Must Finish On",
    ]) {
      expect(constraint.getByRole("option", { name })).toBeInTheDocument();
    }
    const taskType = within(screen.getByLabelText("Task type"));
    for (const name of ["Fixed Units", "Fixed Duration", "Fixed Work"]) {
      expect(taskType.getByRole("option", { name })).toBeInTheDocument();
    }
    const accrual = within(screen.getByLabelText("Accrue at"));
    for (const name of ["Start", "Prorated", "End"]) {
      expect(accrual.getByRole("option", { name })).toBeInTheDocument();
    }
    expect(within(screen.getByLabelText("Task calendar")).getByRole("option", { name: "Project calendar" })).toBeInTheDocument();
  });

  it("writes the header facts in English", () => {
    renderEnglish({ ...summary, late: true, name: "" });
    expect(screen.getByRole("heading", { name: "(Untitled)" })).toBeInTheDocument();
    expect(screen.getByText(/ID 1 · WBS 2 · Summary task/)).toBeInTheDocument();
    expect(screen.getByText(/^Cost .* · Actual .* · Late$/)).toBeInTheDocument();
  });

  it("edits assignments and splits in English", () => {
    const split = {
      ...leaf,
      splits: [{ offset: 480, gap: 480 }],
      segments: [
        { start: at(2026, 1, 5, 8), finish: at(2026, 1, 5, 17) },
        { start: at(2026, 1, 7, 8), finish: at(2026, 1, 7, 17) },
      ],
    };
    const project = makeProject({
      tasks: [split],
      resources: [makeResource({ uid: 10, name: "An" })],
      assignments: [makeAssignment({ uid: 1, task_uid: 2, resource_uid: 10 })],
    });
    renderEnglish(split, project);
    const resources = within(screen.getByRole("group", { name: "Resources" }));
    expect(resources.getByText("Assign resources")).toBeInTheDocument();
    expect(resources.getByLabelText("Units for An")).toBeInTheDocument();
    expect(resources.getByLabelText("Units to assign")).toBeInTheDocument();
    expect(resources.getByRole("button", { name: "Remove" })).toBeInTheDocument();
    expect(resources.getByRole("option", { name: "Choose…" })).toBeInTheDocument();
    expect(screen.getByText("Split task")).toBeInTheDocument();
    expect(screen.getByText("Part 1: 2026-01-05 08:00 – 2026-01-05 17:00")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Join with part 2" })).toHaveAttribute(
      "title",
      "Gap of 1 day of working time",
    );
    expect(screen.getByRole("button", { name: "Join all parts" })).toBeInTheDocument();
  });
});

describe("Inspector custom fields", () => {
  beforeEach(() => window.localStorage.clear());

  it("lists the project's fields and sets a value, on a summary task too", async () => {
    const group = makeTask({ uid: 1, name: "Nhóm", summary: true, values: { 3: "BE" } });
    const project = makeProject({
      tasks: [group],
      fields: [
        { id: 3, name: "Mảng", kind: { type: "choice", options: ["BE", "FE"] } },
        { id: 4, name: "Khách", kind: { type: "text" } },
      ],
    });
    const { send, user } = renderInspector(group, project);
    const fields = screen.getByRole("group", { name: "Trường tùy chỉnh" });
    expect(within(fields).getByLabelText("Mảng")).toHaveValue("BE");
    await user.selectOptions(within(fields).getByLabelText("Mảng"), "");
    expect(send).toHaveBeenCalledWith({ type: "set_field_value", uid: 1, field_id: 3, value: null });
    await user.type(within(fields).getByLabelText("Khách"), "ACME");
    await user.tab();
    expect(send).toHaveBeenCalledWith({ type: "set_field_value", uid: 1, field_id: 4, value: "ACME" });
  });

  it("has no custom fields group without fields", () => {
    renderInspector(leaf);
    expect(screen.queryByRole("group", { name: "Trường tùy chỉnh" })).not.toBeInTheDocument();
  });
});
