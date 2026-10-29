import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import type { ProjectDiffReport } from "../../types";
import { VarianceAuditView } from "./VarianceAuditView";

const mockDiffReport: ProjectDiffReport = {
  base_name: "Archive-v1.sheafly",
  current_name: "Active-v2.sheafly",
  base_finish: 1714500000000,
  current_finish: 1714800000000,
  finish_delta_days: 3.5,
  cost_delta: 1200,
  work_delta_hours: 40,
  summary: {
    total_current_tasks: 3,
    total_baseline_tasks: 3,
    added_count: 1,
    removed_count: 1,
    modified_count: 1,
    unchanged_count: 0,
    slipped_count: 1,
  },
  tasks: [
    {
      uid: 1,
      id: 1,
      wbs: "1.1",
      name: "Khảo sát mặt bằng",
      kind: "Modified",
      old_start: 1714000000000,
      new_start: 1714000000000,
      old_finish: 1714200000000,
      new_finish: 1714500000000,
      finish_slip_days: 3.5,
      old_duration: 16,
      new_duration: 40,
      duration_delta: 24,
      old_work: 960,
      new_work: 2400,
      work_delta: 1440,
      old_cost: 500,
      new_cost: 1200,
      cost_delta: 700,
      link_diffs: [
        {
          other_task_uid: 2,
          other_task_name: "Thiết kế móng",
          is_predecessor: false,
          change: "added",
          old_detail: null,
          new_detail: "FS",
        },
      ],
      assignment_diffs: [
        {
          resource_uid: 10,
          resource_name: "Kỹ sư An",
          change: "added",
          old_units: null,
          new_units: 100,
        },
      ],
    },
    {
      uid: 2,
      id: 2,
      wbs: "1.2",
      name: "Thi công ngầm",
      kind: "Added",
      old_start: null,
      new_start: 1714500000000,
      old_finish: null,
      new_finish: 1714800000000,
      finish_slip_days: 0,
      old_duration: null,
      new_duration: 24,
      duration_delta: 24,
      old_work: null,
      new_work: 1440,
      work_delta: 1440,
      old_cost: null,
      new_cost: 500,
      cost_delta: 500,
      link_diffs: [],
      assignment_diffs: [],
    },
    {
      uid: 3,
      id: 3,
      wbs: "1.3",
      name: "Giai đoạn cũ",
      kind: "Removed",
      old_start: 1713500000000,
      new_start: null,
      old_finish: 1713800000000,
      new_finish: null,
      finish_slip_days: 0,
      old_duration: 16,
      new_duration: null,
      duration_delta: -16,
      old_work: 960,
      new_work: null,
      work_delta: -960,
      old_cost: 400,
      new_cost: null,
      cost_delta: -400,
      link_diffs: [],
      assignment_diffs: [],
    },
  ],
};

function renderComponent({
  diffPro = true,
  activeReport = mockDiffReport as ProjectDiffReport | null,
  onOpenLicense = vi.fn(),
  onReportChange = vi.fn(),
  onPickTask = vi.fn(),
} = {}) {
  const user = userEvent.setup();
  render(
    <I18nProvider lang="vi">
      <VarianceAuditView
        diffPro={diffPro}
        onOpenLicense={onOpenLicense}
        activeReport={activeReport}
        onReportChange={onReportChange}
        onPickTask={onPickTask}
      />
    </I18nProvider>,
  );
  return { user, onOpenLicense, onReportChange, onPickTask };
}

describe("VarianceAuditView", () => {
  it("renders Pro paywall when diffPro is false", async () => {
    const { user, onOpenLicense } = renderComponent({ diffPro: false });
    expect(screen.getByText("So sánh phiên bản là tính năng Sheafly Pro")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Kích hoạt license" }));
    expect(onOpenLicense).toHaveBeenCalledOnce();
  });

  it("renders diff summary KPIs and table when report is loaded", () => {
    renderComponent({ diffPro: true, activeReport: mockDiffReport });
    expect(screen.getByText("Thêm mới: 1")).toBeInTheDocument();
    expect(screen.getByText("Bị xóa: 1")).toBeInTheDocument();
    expect(screen.getByText("Thay đổi: 1")).toBeInTheDocument();
    expect(screen.getByText("Trượt hạn: 1")).toBeInTheDocument();
    expect(screen.getByText("Khảo sát mặt bằng")).toBeInTheDocument();
    expect(screen.getByText("Thi công ngầm")).toBeInTheDocument();
    expect(screen.getByText("Giai đoạn cũ")).toBeInTheDocument();
  });

  it("calls onPickTask when clicking task link in diff table", async () => {
    const { user, onPickTask } = renderComponent({ diffPro: true, activeReport: mockDiffReport });
    await user.click(screen.getByRole("button", { name: "Khảo sát mặt bằng" }));
    expect(onPickTask).toHaveBeenCalledWith(1);
  });

  it("filters tasks by search input", async () => {
    const { user } = renderComponent({ diffPro: true, activeReport: mockDiffReport });
    const searchInput = screen.getByPlaceholderText("Lọc theo tên hoặc WBS…");
    await user.type(searchInput, "ngầm");
    expect(screen.getByText("Thi công ngầm")).toBeInTheDocument();
    expect(screen.queryByText("Khảo sát mặt bằng")).not.toBeInTheDocument();
  });

  it("filters tasks by 'only slipped'", async () => {
    const { user } = renderComponent({ diffPro: true, activeReport: mockDiffReport });
    await user.click(screen.getByRole("button", { name: "Chỉ việc trễ hạn" }));
    expect(screen.getByText("Khảo sát mặt bằng")).toBeInTheDocument();
    expect(screen.queryByText("Thi công ngầm")).not.toBeInTheDocument();
    expect(screen.queryByText("Giai đoạn cũ")).not.toBeInTheDocument();
  });
});
