import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { at, makeProject, makeStatistics, standardCalendar } from "../test/fixtures";
import { I18nProvider } from "../i18n";
import { ProjectDialog } from "./ProjectDialog";

function renderDialog(editable: boolean, capacityPro = true, focus_factor = 1) {
  const send = vi.fn().mockResolvedValue(undefined);
  render(
    <I18nProvider lang="vi">
      <ProjectDialog
        project={makeProject({ editable_actual_costs: editable, focus_factor })}
        capacityPro={capacityPro}
        changed={false}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        send={send}
        statistics={null}
        onOpenWorkingTime={vi.fn()}
      />
    </I18nProvider>,
  );
  return send;
}

describe("ProjectDialog productivity", () => {
  it("sets the useful productivity as a percentage and refuses what does not read", async () => {
    const send = renderDialog(false);
    const user = userEvent.setup();
    const field = screen.getByRole("textbox", { name: "Năng suất hữu ích" });
    expect(field).toHaveValue("100%");
    await user.clear(field);
    await user.type(field, "80");
    await user.tab();
    expect(send).toHaveBeenCalledWith({ type: "set_focus_factor", factor: 0.8 });
    expect(field).toHaveValue("80%");
    send.mockClear();
    await user.clear(field);
    await user.type(field, "40");
    await user.tab();
    expect(send).not.toHaveBeenCalled();
    expect(field).toHaveAttribute("aria-invalid", "true");
  });
});

describe("ProjectDialog productivity without Sheafly Pro", () => {
  it("shows the factor, refuses a new one and still sets it back to 100%", async () => {
    const send = renderDialog(false, false, 0.8);
    const user = userEvent.setup();
    expect(screen.getByText("Cần Sheafly Pro để đổi; bản Free chỉ đặt lại được 100%")).toBeInTheDocument();
    const field = screen.getByRole("textbox", { name: "Năng suất hữu ích" });
    expect(field).toHaveValue("80%");
    await user.clear(field);
    await user.type(field, "70");
    await user.tab();
    expect(send).not.toHaveBeenCalled();
    expect(field).toHaveAttribute("aria-invalid", "true");
    await user.clear(field);
    await user.type(field, "100");
    await user.tab();
    expect(send).toHaveBeenCalledWith({ type: "set_focus_factor", factor: 1 });
  });
});

describe("ProjectDialog actual costs", () => {
  it("turns hand-entered actual costs on", async () => {
    const send = renderDialog(false);
    const box = screen.getByRole("checkbox", { name: "Nhập chi phí thực tế bằng tay" });
    expect(box).not.toBeChecked();
    await userEvent.setup().click(box);
    expect(send).toHaveBeenCalledWith({ type: "set_editable_actual_costs", editable: true });
  });

  it("shows the switch on and turns it off", async () => {
    const send = renderDialog(true);
    const box = screen.getByRole("checkbox", { name: "Nhập chi phí thực tế bằng tay" });
    expect(box).toBeChecked();
    await userEvent.setup().click(box);
    expect(send).toHaveBeenCalledWith({ type: "set_editable_actual_costs", editable: false });
  });
});

describe("ProjectDialog sections", () => {
  function renderWith(statistics: Parameters<typeof ProjectDialog>[0]["statistics"]) {
    const send = vi.fn().mockResolvedValue(undefined);
    const onOpenWorkingTime = vi.fn();
    const project = makeProject({
      calendars: [standardCalendar(1, "Chuẩn"), standardCalendar(2, "Ca đêm")],
    });
    render(
      <I18nProvider lang="vi">
        <ProjectDialog
          project={project}
          statistics={statistics}
          changed={false}
          onSave={vi.fn()}
          onCancel={vi.fn()}
          send={send}
          onOpenWorkingTime={onOpenWorkingTime}
        />
      </I18nProvider>,
    );
    return { send, onOpenWorkingTime, user: userEvent.setup() };
  }

  it("names the name field, picks the project calendar and opens working time", async () => {
    const { send, onOpenWorkingTime, user } = renderWith(null);
    expect(screen.getByRole("textbox", { name: "Tên" })).toHaveValue("Dự án thử");
    await user.selectOptions(screen.getByLabelText("Lịch dự án"), "2");
    expect(send).toHaveBeenCalledWith({ type: "set_project_calendar", calendar_id: 2 });
    await user.click(screen.getByRole("button", { name: "Lịch làm việc…" }));
    expect(onOpenWorkingTime).toHaveBeenCalled();
    expect(screen.queryByText("Thứ Hai")).not.toBeInTheDocument();
  });

  it("shows the statistics with the baseline and its variance", async () => {
    const { user } = renderWith(
      makeStatistics({
        duration: 960,
        work: 960,
        actual_work: 240,
        remaining_work: 720,
        cost: 1500,
        actual_cost: 500,
        remaining_cost: 1000,
        percent_complete: 25,
        baseline: { start: at(2026, 1, 5, 8), finish: at(2026, 1, 5, 17), duration: 480, work: 480, cost: 1000 },
      }),
    );
    const tab = screen.getByRole("tab", { name: "Thống kê" });
    await user.click(tab);
    expect(tab).toHaveAttribute("aria-selected", "true");
    const cost = screen.getAllByRole("row").find((row) => within(row).queryByRole("rowheader", { name: "Chi phí" }));
    expect(within(cost as HTMLElement).getByText("+500")).toBeInTheDocument();
    expect(screen.getByText("Hoàn thành 25 %")).toBeInTheDocument();
    expect(screen.queryByText("Chưa đặt đường cơ sở 0.")).not.toBeInTheDocument();
  });

  it("moves between tabs with the arrow keys and says when there is no baseline", async () => {
    const { user } = renderWith(makeStatistics());
    screen.getByRole("tab", { name: "Chung" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Thống kê" })).toHaveFocus();
    expect(screen.getByText("Chưa đặt đường cơ sở 0.")).toBeInTheDocument();
  });
});
