import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeResource, makeTask, MONDAY_8AM } from "../../test/fixtures";
import type { CurvesReport } from "../../types";
import { CurvesView } from "./CurvesView";

const sampleReport: CurvesReport = {
  measure: "cost",
  bucket: "week",
  filter: { type: "project" },
  baseline: 0,
  status_date: Date.parse("2026-01-12T00:00:00Z"),
  baseline_finish: Date.parse("2026-01-26T17:00:00Z"),
  scheduled_finish: Date.parse("2026-01-26T17:00:00Z"),
  total_baseline: 10_000_000,
  total_scheduled: 12_000_000,
  total_actual: 4_000_000,
  total_earned: 4_500_000,
  points: [
    {
      date: Date.parse("2026-01-05T00:00:00Z"),
      label: "W01",
      baseline_period: 3_000_000,
      scheduled_period: 3_000_000,
      actual_period: 2_000_000,
      earned_period: 2_200_000,
      baseline_cumulative: 3_000_000,
      scheduled_cumulative: 3_000_000,
      actual_cumulative: 2_000_000,
      earned_cumulative: 2_200_000,
    },
    {
      date: Date.parse("2026-01-12T00:00:00Z"),
      label: "W02",
      baseline_period: 4_000_000,
      scheduled_period: 5_000_000,
      actual_period: 2_000_000,
      earned_period: 2_300_000,
      baseline_cumulative: 7_000_000,
      scheduled_cumulative: 8_000_000,
      actual_cumulative: 4_000_000,
      earned_cumulative: 4_500_000,
    },
    {
      date: Date.parse("2026-01-19T00:00:00Z"),
      label: "W03",
      baseline_period: 3_000_000,
      scheduled_period: 4_000_000,
      actual_period: null,
      earned_period: null,
      baseline_cumulative: 10_000_000,
      scheduled_cumulative: 12_000_000,
      actual_cumulative: null,
      earned_cumulative: null,
    },
  ],
};

const project = makeProject({
  start: MONDAY_8AM,
  resources: [
    makeResource({ uid: 1, name: "Alice", group: "Dev" }),
    makeResource({ uid: 2, name: "Bob", group: "QA" }),
  ],
  skills: [
    { id: 1, name: "Rust" },
    { id: 2, name: "React" },
  ],
  tasks: [
    makeTask({
      uid: 1,
      id: 1,
      name: "Task 1",
      start: MONDAY_8AM,
      finish: MONDAY_8AM + 5 * 24 * 3600 * 1000,
      work: 2880,
      duration: 2880,
      cost: 5_000_000,
      actual_cost: 2_000_000,
      baselines: [
        {
          number: 0,
          start: MONDAY_8AM,
          finish: MONDAY_8AM + 5 * 24 * 3600 * 1000,
          work: 2880,
          cost: 5_000_000,
          segments: [],
        },
      ],
    }),
  ],
});

function renderView(lang: "vi" | "en" = "vi", proj = project) {
  fakeBackend.curvesReport = structuredClone(sampleReport);
  return render(
    <I18nProvider lang={lang}>
      <CurvesView project={proj} />
    </I18nProvider>,
  );
}

describe("CurvesView", () => {
  it("renders filter controls and SVG S-curve chart", async () => {
    renderView("vi");

    // Check dropdown controls
    expect(screen.getByLabelText("Chỉ số:")).toBeDefined();
    expect(screen.getByLabelText("Kỳ:")).toBeDefined();
    expect(screen.getByLabelText("Bộ lọc:")).toBeDefined();
    expect(screen.getByLabelText("Baseline:")).toBeDefined();
    expect(screen.getByLabelText("Hiển thị:")).toBeDefined();

    // Wait for curves report to load and render chart
    await waitFor(() => {
      const chart = screen.getByRole("img", { name: "Biểu đồ đường cong S và phân bổ theo kỳ" });
      expect(chart).toBeDefined();
    });

    // Check legend items
    expect(screen.getByText("Cơ sở (Baseline)")).toBeDefined();
    expect(screen.getByText("Lịch hiện tại (Scheduled)")).toBeDefined();
    expect(screen.getByText("Thực tế (Actual)")).toBeDefined();
    expect(screen.getByText("Giá trị đạt được (EV)")).toBeDefined();

    // Check detailed table
    expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined();
    expect(screen.getAllByText("W01").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("W02").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("W03").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("Tổng cộng")).toBeDefined();
  });

  it("switches between cumulative S-curve and periodic mode", async () => {
    renderView("vi");

    await waitFor(() => {
      expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined();
    });

    const periodicBtn = screen.getByRole("button", { name: "Theo kỳ" });
    fireEvent.click(periodicBtn);

    // In periodic mode, the bars should be rendered in the chart
    await waitFor(() => {
      const chart = screen.getByRole("img", { name: "Biểu đồ đường cong S và phân bổ theo kỳ" });
      expect(chart.querySelectorAll(".curves-bar").length).toBeGreaterThan(0);
    });
  });

  it("changes measure and invokes curves with correct arguments", async () => {
    renderView("vi");

    await waitFor(() => {
      expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined();
    });

    const measureSelect = screen.getByLabelText("Chỉ số:") as HTMLSelectElement;
    fireEvent.change(measureSelect, { target: { value: "work" } });

    await waitFor(() => {
      const calls = fakeBackend.calls.filter((c) => c.cmd === "curves");
      const lastCall = calls[calls.length - 1];
      expect(lastCall.args?.measure).toBe("work");
    });
  });

  it("changes filter type to person, role, and skill", async () => {
    renderView("vi");

    await waitFor(() => {
      expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined();
    });

    const filterSelect = screen.getByLabelText("Bộ lọc:") as HTMLSelectElement;

    // Filter by person
    fireEvent.change(filterSelect, { target: { value: "person" } });
    await waitFor(() => {
      const personSelect = screen.getByLabelText("Chọn người…");
      expect(personSelect).toBeDefined();
    });

    // Filter by role / group
    fireEvent.change(filterSelect, { target: { value: "group" } });
    await waitFor(() => {
      const groupSelect = screen.getByLabelText("Chọn vai trò…");
      expect(groupSelect).toBeDefined();
    });

    // Filter by skill
    fireEvent.change(filterSelect, { target: { value: "skill" } });
    await waitFor(() => {
      const skillSelect = screen.getByLabelText("Chọn kỹ năng…");
      expect(skillSelect).toBeDefined();
    });
  });

  it("renders in English when locale is en", async () => {
    renderView("en");

    expect(screen.getByLabelText("Measure:")).toBeDefined();
    expect(screen.getByLabelText("Period:")).toBeDefined();
    expect(screen.getByLabelText("Filter:")).toBeDefined();
    expect(screen.getByLabelText("Baseline:")).toBeDefined();
    expect(screen.getByLabelText("Mode:")).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText("Detailed Figures Table")).toBeDefined();
      expect(screen.getByText("Total")).toBeDefined();
    });
  });

  it("asks for the curves again when the project changes", async () => {
    const view = renderView("vi");
    await waitFor(() => expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined());
    const before = fakeBackend.calls.filter((c) => c.cmd === "curves").length;

    const edited = { ...project, tasks: project.tasks.map((t) => ({ ...t, name: `${t.name}!` })) };
    view.rerender(
      <I18nProvider lang="vi">
        <CurvesView project={edited} />
      </I18nProvider>,
    );

    await waitFor(() =>
      expect(fakeBackend.calls.filter((c) => c.cmd === "curves").length).toBe(before + 1),
    );
    // The chart stays on screen while the new numbers come.
    expect(screen.getByRole("img", { name: "Biểu đồ đường cong S và phân bổ theo kỳ" })).toBeDefined();
  });

  it("shows why the curves could not be computed", async () => {
    fakeBackend.failNext("curves", "Khoảng thời gian quá dài");
    render(
      <I18nProvider lang="vi">
        <CurvesView project={project} />
      </I18nProvider>,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toBe("Không tính được đường cong: Khoảng thời gian quá dài");
    expect(screen.queryByText("Không có dữ liệu cho cấu hình lọc này.")).toBeNull();
  });

  it("sends no baseline when the baseline is set to none", async () => {
    renderView("vi");
    await waitFor(() => expect(screen.getByText("Bảng chi tiết số liệu")).toBeDefined());
    fireEvent.change(screen.getByLabelText("Baseline:"), { target: { value: "" } });
    await waitFor(() => {
      const calls = fakeBackend.calls.filter((c) => c.cmd === "curves");
      expect(calls[calls.length - 1].args?.baseline).toBeUndefined();
    });
  });
});
