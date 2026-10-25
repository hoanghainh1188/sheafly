import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { makeHealthCriterion, makeHealthReport } from "../../test/fixtures";
import { HealthDialog } from "./HealthDialog";

const REPORT = makeHealthReport([
  makeHealthCriterion(1, {
    status: "fail",
    count: 2,
    total: 20,
    value: 0.1,
    tasks: [
      { uid: 4, reason: "no_successor" },
      { uid: 9, reason: "no_links" },
    ],
  }),
  makeHealthCriterion(11, { status: "not_applicable", not_applicable: "no_baseline", value: null, count: 0, total: 0 }),
  makeHealthCriterion(10, { status: "info", limit: null, value: 0.25, count: 1, total: 4, tasks: [{ uid: 3, reason: "no_resource" }] }),
]);

describe("HealthDialog", () => {
  it("lists the 14 criteria with their result, threshold and status, and the rating", () => {
    render(<HealthDialog report={REPORT} current error={null} onPick={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getAllByRole("row")).toHaveLength(15);
    expect(screen.getByText("Đạt 11 / 12 tiêu chí áp dụng")).toBeInTheDocument();
    expect(screen.getByText("Rủi ro")).toBeInTheDocument();
    const logic = screen.getByRole("row", { name: /Logic/ });
    expect(within(logic).getByText("Không đạt")).toBeInTheDocument();
    expect(within(logic).getByText("1 thiếu việc sau, 1 không có liên kết")).toBeInTheDocument();
    expect(within(logic).getByText(/^≤ 5\s?%$/)).toBeInTheDocument();
    const missed = screen.getByRole("row", { name: /Việc lỡ hạn/ });
    expect(within(missed).getByText("Chưa đặt baseline 0")).toBeInTheDocument();
    expect(within(missed).getByText("Không áp dụng")).toBeInTheDocument();
    expect(within(screen.getByRole("row", { name: /Nguồn lực/ })).getByText("Chỉ báo")).toBeInTheDocument();
  });

  it("filters by a criterion's tasks, offered only where there are some", () => {
    const onPick = vi.fn();
    render(<HealthDialog report={REPORT} current error={null} onPick={onPick} onClose={vi.fn()} />);
    expect(screen.getAllByRole("button", { name: /Lọc bảng việc/ })).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Lọc bảng việc theo 2 việc của tiêu chí Logic" }));
    expect(onPick).toHaveBeenCalledWith(1);
  });

  it("says when it is checking, and shows an error", () => {
    const { rerender } = render(<HealthDialog report={null} current={false} error={null} onPick={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Đang kiểm tra…");
    rerender(<HealthDialog report={null} current={false} error="Cần Sheafly Pro" onPick={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Cần Sheafly Pro");
  });

  it("speaks English", () => {
    render(
      <I18nProvider lang="en">
        <HealthDialog report={REPORT} current error={null} onPick={vi.fn()} onClose={vi.fn()} />
      </I18nProvider>,
    );
    expect(screen.getByText("Passed 11 of 12 applicable criteria")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Filter the task sheet to the 2 tasks of Logic" })).toHaveTextContent("Show 2 tasks");
  });
});
