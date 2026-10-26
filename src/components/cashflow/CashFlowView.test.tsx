import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider, setCurrentLang } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeCashFlowReport, makeProject, makeSnapshot, makeTask } from "../../test/fixtures";
import type { CashFlowPeriod, Flows, PaymentForecast } from "../../types";
import { periodLabel, ticks, valueRange } from "./cashFlowData";
import { CashFlowView } from "./CashFlowView";

const flows = (forecast: number, actual: number | null = null, baseline: number | null = null): Flows => ({ forecast, actual, baseline });

function period(label: string, inflow: number, outflow: number, balance: number): CashFlowPeriod {
  return {
    date: Date.parse(`${label}T00:00:00Z`),
    label,
    inflow: flows(inflow),
    outflow: flows(outflow),
    net: flows(inflow - outflow),
    balance: flows(balance),
    deficit: balance < 0,
  };
}

const project = makeProject({
  tasks: [makeTask({ uid: 1, id: 1, name: "Thiết kế" }), makeTask({ uid: 2, id: 2, name: "Nghiệm thu", milestone: true })],
  payments: [
    { id: 1, name: "Đợt 1", amount: { fixed: 500 }, anchor: { type: "finish", task: 2 }, lag_days: 30, retained: false, received: [] },
  ],
});

const forecast: PaymentForecast = {
  id: 1,
  name: "Đợt 1",
  amount: 500,
  retained: 0,
  received: 0,
  scheduled: { invoice: Date.UTC(2026, 1, 6), due: Date.UTC(2026, 2, 8), actual: false },
  baseline: { invoice: Date.UTC(2026, 0, 30), due: Date.UTC(2026, 2, 1), actual: false },
  expected: Date.UTC(2026, 2, 8),
  overdue: false,
  late_days: 7,
  driving: [1],
};

function renderView({ pro = true, onPickTask = vi.fn(), onOpenLicense = vi.fn(), onOpenPayments = vi.fn() } = {}) {
  render(
    <I18nProvider lang="vi">
      <CashFlowView project={project} cashflowPro={pro} onOpenLicense={onOpenLicense} onOpenPayments={onOpenPayments} onPickTask={onPickTask} />
    </I18nProvider>,
  );
  return { onPickTask, onOpenLicense, onOpenPayments, user: userEvent.setup() };
}

describe("CashFlowView", () => {
  beforeEach(() => {
    fakeBackend.reset(makeSnapshot(project));
    fakeBackend.cashFlow = {
      ...makeCashFlowReport([period("2026-01-01", 0, 300, -300), period("2026-02-01", 0, 200, -500), period("2026-03-01", 500, 100, -100)]),
      payments: [forecast],
      baseline: 0,
    };
  });
  afterEach(() => setCurrentLang("vi"));

  it("introduces the feature without the license and never asks for the numbers", async () => {
    const { user, onOpenLicense, onOpenPayments } = renderView({ pro: false });
    expect(screen.getByText("Dòng tiền theo kỳ là tính năng Sheafly Pro")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Kích hoạt license" }));
    await user.click(screen.getByRole("button", { name: "Lịch thanh toán…" }));
    expect(onOpenLicense).toHaveBeenCalled();
    expect(onOpenPayments).toHaveBeenCalled();
    expect(fakeBackend.calls.some((call) => call.cmd === "cash_flow")).toBe(false);
  });

  it("marks the periods short of money in the chart, the table and the summary", async () => {
    renderView();
    const table = await screen.findByRole("region", { name: "Số liệu theo kỳ" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows.map((row) => row.classList.contains("deficit"))).toEqual([true, true, true]);
    expect(within(rows[0]).getByText("thiếu tiền")).toBeInTheDocument();
    expect(document.querySelectorAll(".cash-deficit")).toHaveLength(3);
    expect(screen.getByText("Thiếu tiền từ kỳ (dự báo)").nextElementSibling).toHaveTextContent("01/2026");
    expect(screen.getByText("Số dư thấp nhất (dự báo)").nextElementSibling).toHaveTextContent("-500");
  });

  it("marks short periods only while the forecast is shown, since they are the forecast's", async () => {
    const { user } = renderView();
    await screen.findByRole("region", { name: "Số liệu theo kỳ" });
    await user.click(screen.getByRole("button", { name: "Thực tế" }));
    expect(document.querySelectorAll(".cash-deficit")).toHaveLength(0);
    expect(screen.queryByText("thiếu tiền")).not.toBeInTheDocument();
  });

  it("asks again by quarter and lists each payment against the baseline", async () => {
    const { user, onPickTask } = renderView();
    await screen.findByRole("region", { name: "Các khoản thu" });
    expect(screen.getByText("7 ngày")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Nghiệm thu" }));
    expect(onPickTask).toHaveBeenCalledWith(2);
    await user.click(screen.getByRole("button", { name: "Thiết kế" }));
    expect(onPickTask).toHaveBeenCalledWith(1);
    await user.click(screen.getByRole("button", { name: "Quý" }));
    await waitFor(() => {
      const asked = fakeBackend.calls.filter((call) => call.cmd === "cash_flow");
      expect(asked[asked.length - 1]?.args).toEqual({ options: { bucket: "quarter", baseline: null } });
    });
  });

  it("shows what the engine refused instead of an empty chart", async () => {
    fakeBackend.failNext("cash_flow", "Khoảng thời gian quá dài");
    renderView();
    expect(await screen.findByRole("alert")).toHaveTextContent("Không tính được dòng tiền: Khoảng thời gian quá dài");
  });
});

describe("cashFlowData", () => {
  afterEach(() => setCurrentLang("vi"));

  it("names periods by their bucket", () => {
    expect(periodLabel("2026-04-01", "quarter")).toBe("Quý 2/2026");
    expect(periodLabel("2026-02-01", "month")).toBe("02/2026");
    expect(periodLabel("2026-01-05", "week")).toBe("Tuần 05/01/2026");
    setCurrentLang("en");
    expect(periodLabel("2026-10-01", "quarter")).toBe("Q4 2026");
    expect(periodLabel("2026-01-05", "day")).toBe("2026-01-05");
  });

  it("holds zero, the money out below it and the lowest balance", () => {
    const report = makeCashFlowReport([period("2026-01-01", 100, 300, -200), period("2026-02-01", 400, 0, 200)]);
    expect(valueRange(report, "forecast")).toEqual({ low: -300, high: 400 });
    expect(ticks(-300, 400)).toContain(0);
  });
});
