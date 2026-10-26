import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeSnapshot, makeTask } from "../../test/fixtures";
import type { Command, Payment, Project, Snapshot } from "../../types";
import { PaymentScheduleDialog } from "./PaymentScheduleDialog";

const DAY = 86_400_000;
// Monday 2026-01-05 and the Friday after, in project wall-clock time.
const MONDAY = Date.UTC(2026, 0, 5);

function milestone(uid: number, name: string, day: number) {
  const at = MONDAY + day * DAY + 17 * 3_600_000;
  return makeTask({ uid, id: uid, name, milestone: true, start: at, finish: at });
}

function payment(overrides: Partial<Payment> = {}): Payment {
  return {
    id: 1,
    name: "Tạm ứng",
    amount: { percent: 30 },
    anchor: { type: "finish", task: 2 },
    lag_days: 30,
    retained: false,
    received: [],
    ...overrides,
  };
}

function setup(project: Project, { error = null }: { error?: string | null } = {}) {
  fakeBackend.reset(makeSnapshot(project));
  const send = vi.fn(async (_command: Command): Promise<Snapshot | void> => makeSnapshot(project));
  const handlers = { onSave: vi.fn(), onCancel: vi.fn() };
  render(
    <I18nProvider lang="vi">
      <PaymentScheduleDialog
        project={project}
        changed={false}
        error={error}
        today={MONDAY}
        onSave={handlers.onSave}
        onCancel={handlers.onCancel}
        send={send}
      />
    </I18nProvider>,
  );
  return { send, ...handlers, user: userEvent.setup() };
}

const tasks = [makeTask({ uid: 1, id: 1, name: "Thiết kế" }), milestone(2, "Nghiệm thu thiết kế", 4)];

describe("PaymentScheduleDialog", () => {
  beforeEach(() => fakeBackend.reset());

  it("adds a payment at the last milestone with Net 30 and sends the whole list", async () => {
    const { send, user } = setup(makeProject({ tasks }));
    expect(screen.getByText(/Chưa có khoản thu nào/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Thêm khoản thu" }));
    expect(send).toHaveBeenCalledWith({
      type: "set_payments",
      payments: [
        { name: "Khoản thu 1", amount: { fixed: 0 }, anchor: { type: "finish", task: 2 }, lag_days: 30, retained: false },
      ],
    });
  });

  it("shows the invoice and due dates the engine gives, and edits the terms of a row", async () => {
    const { send, user } = setup(makeProject({ tasks, payments: [payment()], next_payment_id: 2, cash_flow: { contract_value: 1_000_000, opening_balance: 0, retention: null } }));
    const row = screen.getByRole("textbox", { name: "Tên" }).closest("tr") as HTMLElement;
    // Invoiced on the milestone's Friday, due 30 calendar days later.
    expect(await within(row).findByText("09/01/2026")).toBeInTheDocument();
    expect(within(row).getByText("08/02/2026")).toBeInTheDocument();
    expect(screen.getByText("Tổng các khoản theo phần trăm là 30%, chưa bằng 100%.")).toBeInTheDocument();

    const amount = screen.getByRole("textbox", { name: "Số tiền: Tạm ứng" });
    await user.clear(amount);
    await user.type(amount, "12,5{Tab}");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_payments",
      payments: [{ id: 1, name: "Tạm ứng", amount: { percent: 12.5 }, anchor: { type: "finish", task: 2 }, lag_days: 30, retained: false }],
    });

    const lag = screen.getByRole("textbox", { name: "Net (ngày): Tạm ứng" });
    await user.clear(lag);
    await user.type(lag, "400{Tab}");
    expect(lag).toHaveAttribute("aria-invalid", "true");
    expect(send).toHaveBeenCalledTimes(1);

    await user.selectOptions(screen.getByRole("combobox", { name: "Gắn với: Tạm ứng: Lúc" }), "start");
    expect(send).toHaveBeenLastCalledWith(
      expect.objectContaining({ payments: [expect.objectContaining({ anchor: { type: "start", task: 2 } })] }),
    );
  });

  it("turns the anchor into a fixed date on the task's day, so the money does not move", async () => {
    const { send, user } = setup(makeProject({ tasks, payments: [payment()], next_payment_id: 2 }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Gắn với: Tạm ứng" }), "date");
    expect(send).toHaveBeenLastCalledWith(
      expect.objectContaining({ payments: [expect.objectContaining({ anchor: { type: "date", at: MONDAY + 4 * DAY } })] }),
    );
  });

  it("warns about a percent payment without a contract value and a detached milestone", () => {
    setup(makeProject({ tasks, payments: [payment({ detached: true, anchor: { type: "date", at: MONDAY } })], next_payment_id: 2 }));
    expect(screen.getByText(/chưa có giá trị hợp đồng/)).toBeInTheDocument();
    expect(screen.getByText(/Mốc đã xóa/)).toBeInTheDocument();
  });

  it("sets the contract, a negative opening balance and the retention", async () => {
    const { send, user } = setup(makeProject({ tasks }));
    await user.type(screen.getByRole("textbox", { name: "Giá trị hợp đồng" }), "1.000.000{Tab}");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_cash_flow_settings",
      settings: { contract_value: 1_000_000, opening_balance: 0, retention: null },
    });
    const opening = screen.getByRole("textbox", { name: "Số dư đầu kỳ" });
    await user.clear(opening);
    await user.type(opening, "-50.000{Tab}");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_cash_flow_settings",
      settings: { contract_value: 1_000_000, opening_balance: -50_000, retention: null },
    });
    await user.click(screen.getByRole("checkbox", { name: "Giữ lại bảo hành" }));
    expect(send).toHaveBeenLastCalledWith({
      type: "set_cash_flow_settings",
      settings: {
        contract_value: 1_000_000,
        opening_balance: -50_000,
        retention: { percent: 5, release: { type: "finish", task: 2 }, lag_days: 30 },
      },
    });
  });

  it("records a receipt apart from the terms", async () => {
    const { send, user } = setup(makeProject({ tasks, payments: [payment({ amount: { fixed: 300_000 } })], next_payment_id: 2 }));
    await user.click(screen.getByRole("button", { name: "Ghi nhận thu tiền cho Tạm ứng" }));
    const region = screen.getByRole("region", { name: "Tiền đã thu: Tạm ứng" });
    const add = within(region).getByRole("button", { name: "Thêm lần thu" });
    expect(add).toBeDisabled();
    await user.type(within(region).getByRole("textbox", { name: "Số tiền thu" }), "100.000{Tab}");
    await user.click(add);
    expect(send).toHaveBeenLastCalledWith({ type: "set_payment_receipts", id: 1, receipts: [{ date: MONDAY, amount: 100_000 }] });
  });

  it("builds each receipt edit on the one sent last, before the engine answers", async () => {
    const project = makeProject({ tasks, payments: [payment({ amount: { fixed: 300_000 } })], next_payment_id: 2 });
    fakeBackend.reset(makeSnapshot(project));
    // The engine never answers, so the drawn list stays the old one throughout.
    const send = vi.fn((_command: Command) => new Promise<Snapshot | void>(() => {}));
    render(
      <I18nProvider lang="vi">
        <PaymentScheduleDialog project={project} changed={false} error={null} today={MONDAY} onSave={vi.fn()} onCancel={vi.fn()} send={send} />
      </I18nProvider>,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Ghi nhận thu tiền cho Tạm ứng" }));
    const amount = screen.getByRole("textbox", { name: "Số tiền thu" });
    await user.type(amount, "100{Tab}");
    await user.click(screen.getByRole("button", { name: "Thêm lần thu" }));
    await user.type(amount, "50{Tab}");
    await user.click(screen.getByRole("button", { name: "Thêm lần thu" }));
    expect(send).toHaveBeenLastCalledWith({
      type: "set_payment_receipts",
      id: 1,
      receipts: [
        { date: MONDAY, amount: 100 },
        { date: MONDAY, amount: 50 },
      ],
    });
  });

  it("saves once the engine answered, shows a refusal in its footer, and cancels", async () => {
    const { onSave, onCancel, user } = setup(makeProject({ tasks }), { error: "Một dự án có tối đa 500 khoản thu" });
    expect(screen.getByRole("alert")).toHaveTextContent("tối đa 500 khoản thu");
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
