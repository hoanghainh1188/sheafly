import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../i18n";
import { makeProject, makeResource } from "../test/fixtures";
import type { AssignmentRule, FieldDef, Project } from "../types";
import { parseValues, RulesTab } from "./RulesTab";

function renderTab(project: Project, send = vi.fn().mockResolvedValue({}), lang: "vi" | "en" = "vi") {
  render(
    <I18nProvider lang={lang}>
      <RulesTab project={project} send={send} />
    </I18nProvider>,
  );
  return { send, user: userEvent.setup() };
}

const AREA: FieldDef = { id: 1, name: "Mảng", kind: { type: "choice", options: ["Web", "App", "Dữ liệu"] } };
const SIZE: FieldDef = { id: 2, name: "Cỡ", kind: { type: "number" } };
const TAG: FieldDef = { id: 3, name: "Nhãn", kind: { type: "text" } };
const WEB_TO_DESIGN: AssignmentRule = {
  conditions: [{ field: 1, values: ["Web"] }],
  target: { type: "skill", id: 1 },
  units: 1,
};
const REST_TO_AN: AssignmentRule = { conditions: [], target: { type: "resource", uid: 10 }, units: 0.5 };

const team = (rules: AssignmentRule[] = [], fields: FieldDef[] = [AREA, SIZE, TAG]) =>
  makeProject({
    fields,
    skills: [{ id: 1, name: "Thiết kế" }],
    resources: [makeResource({ uid: 10, name: "An" })],
    assignment_rules: rules,
  });

const sent = (send: ReturnType<typeof vi.fn>) => (send.mock.lastCall?.[0] as { rules: AssignmentRule[] }).rules;

describe("RulesTab", () => {
  it("adds a rule for every task to the first skill", async () => {
    const { send, user } = renderTab(team());
    expect(screen.getByText("Chưa có quy tắc nào.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Thêm quy tắc" }));
    expect(send).toHaveBeenCalledWith({
      type: "set_assignment_rules",
      rules: [{ conditions: [], target: { type: "skill", id: 1 }, units: 1 }],
    });
  });

  it("asks for a minimum level once the project has a scale", async () => {
    const project = makeProject({
      ...team([WEB_TO_DESIGN]),
      skill_levels: [
        { id: 4, name: "Junior" },
        { id: 5, name: "Senior" },
      ],
    });
    const { send, user } = renderTab(project);
    const level = screen.getByRole("combobox", { name: "Cấp tối thiểu, quy tắc 1" });
    expect(level).toHaveValue("");
    await user.selectOptions(level, "Senior");
    expect(sent(send)[0].target).toEqual({ type: "skill", id: 1, min_level: 5 });
  });

  it("asks for no level when the project has no scale", () => {
    renderTab(team([WEB_TO_DESIGN]));
    expect(screen.queryByRole("combobox", { name: "Cấp tối thiểu, quy tắc 1" })).not.toBeInTheDocument();
  });

  it("cannot add a rule with nobody to point at", () => {
    renderTab(makeProject());
    expect(screen.getByRole("button", { name: "Thêm quy tắc" })).toBeDisabled();
    expect(screen.getByText(/Chưa có trường tùy chỉnh/)).toBeInTheDocument();
  });

  it("shows a rule's conditions, target and units", () => {
    renderTab(team([WEB_TO_DESIGN, REST_TO_AN]));
    const first = screen.getByRole("listitem", { name: "Quy tắc 1" });
    expect(within(first).getByRole("checkbox", { name: "Mảng: Web" })).toBeChecked();
    // The last value stays: a condition needs one.
    expect(within(first).getByRole("checkbox", { name: "Mảng: Web" })).toBeDisabled();
    expect(within(first).getByRole("combobox", { name: "Giao cho, quy tắc 1" })).toHaveValue("skill:1");
    const second = screen.getByRole("listitem", { name: "Quy tắc 2" });
    expect(within(second).getByText("mọi công việc")).toBeInTheDocument();
    expect(within(second).getByRole("textbox", { name: "Đơn vị, quy tắc 2" })).toHaveValue("0,5");
  });

  it("ticks choices and changes the target", async () => {
    const { send, user } = renderTab(team([WEB_TO_DESIGN]));
    await user.click(screen.getByRole("checkbox", { name: "Mảng: App" }));
    expect(sent(send)).toEqual([{ ...WEB_TO_DESIGN, conditions: [{ field: 1, values: ["Web", "App"] }] }]);
    await user.selectOptions(screen.getByRole("combobox", { name: "Giao cho, quy tắc 1" }), "An");
    // Built on the list sent last, so the ticked choice stays.
    expect(sent(send)).toEqual([
      { ...WEB_TO_DESIGN, conditions: [{ field: 1, values: ["Web", "App"] }], target: { type: "resource", uid: 10 } },
    ]);
  });

  it("adds a condition on a choice field and on a number field", async () => {
    const { send, user } = renderTab(team([REST_TO_AN]));
    const rule = screen.getByRole("listitem", { name: "Quy tắc 1" });
    await user.selectOptions(within(rule).getByRole("combobox", { name: "Giá trị của điều kiện mới, quy tắc 1" }), "App");
    await user.click(within(rule).getByRole("button", { name: "Thêm điều kiện" }));
    expect(sent(send)).toEqual([{ ...REST_TO_AN, conditions: [{ field: 1, values: ["App"] }] }]);

    send.mockClear();
    await user.selectOptions(within(rule).getByRole("combobox", { name: "Trường của điều kiện mới, quy tắc 1" }), "Cỡ");
    const value = within(rule).getByRole("textbox", { name: "Giá trị của điều kiện mới, quy tắc 1" });
    await user.type(value, "nhiều");
    expect(within(rule).getByRole("button", { name: "Thêm điều kiện" })).toBeDisabled();
    await user.clear(value);
    await user.type(value, "1,5; 3");
    await user.click(within(rule).getByRole("button", { name: "Thêm điều kiện" }));
    expect(sent(send)).toEqual([
      { ...REST_TO_AN, conditions: [{ field: 1, values: ["App"] }, { field: 2, values: [1.5, 3] }] },
    ]);
  });

  it("edits typed values on leaving them and puts back what does not read", async () => {
    const rule: AssignmentRule = { ...REST_TO_AN, conditions: [{ field: 2, values: [1.5] }] };
    const { send, user } = renderTab(team([rule]));
    const values = screen.getByRole("textbox", { name: "Giá trị của Cỡ" });
    expect(values).toHaveValue("1,5");
    await user.clear(values);
    await user.type(values, "lớn");
    await user.tab();
    expect(send).not.toHaveBeenCalled();
    expect(values).toHaveValue("1,5");
    await user.clear(values);
    await user.type(values, "2; 2; 4");
    await user.tab();
    expect(sent(send)).toEqual([{ ...rule, conditions: [{ field: 2, values: [2, 4] }] }]);
  });

  it("removes a condition, edits units as hours per day, and refuses zero units", async () => {
    const { send, user } = renderTab(team([WEB_TO_DESIGN]));
    await user.click(screen.getByRole("button", { name: "Bỏ điều kiện Mảng" }));
    expect(sent(send)).toEqual([{ ...WEB_TO_DESIGN, conditions: [] }]);
    const units = screen.getByRole("textbox", { name: "Đơn vị, quy tắc 1" });
    await user.clear(units);
    await user.type(units, "0");
    await user.tab();
    expect(send).toHaveBeenCalledTimes(1);
    expect(units).toHaveValue("1");
    await user.clear(units);
    await user.type(units, "4h/ngày");
    await user.tab();
    expect(sent(send)).toEqual([{ ...WEB_TO_DESIGN, conditions: [], units: 0.5 }]);
  });

  it("moves and removes rules", async () => {
    const { send, user } = renderTab(team([WEB_TO_DESIGN, REST_TO_AN]));
    expect(screen.getByRole("button", { name: "Đưa quy tắc 1 lên" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Đưa quy tắc 2 xuống" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Đưa quy tắc 2 lên" }));
    expect(sent(send)).toEqual([REST_TO_AN, WEB_TO_DESIGN]);
    // The cards show the order sent: rule 1 is now the rule for An.
    await user.click(screen.getByRole("button", { name: "Bỏ quy tắc 1" }));
    expect(sent(send)).toEqual([WEB_TO_DESIGN]);
  });

  it("speaks English", () => {
    renderTab(team([WEB_TO_DESIGN]), undefined, "en");
    expect(screen.getByRole("list", { name: "Assignment rules" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Whoever has Thiết kế" })).toBeInTheDocument();
  });
});

describe("RulesTab with the engine slow or refusing", () => {
  it("edits the moved rule when clicked before the engine answers the move", async () => {
    const { send, user } = renderTab(team([WEB_TO_DESIGN, REST_TO_AN]), vi.fn().mockReturnValue(new Promise(() => {})));
    await user.click(screen.getByRole("button", { name: "Đưa quy tắc 2 lên" }));
    // The cards already show the order sent; rule 1 is now the one for An.
    expect(screen.getByRole("combobox", { name: "Giao cho, quy tắc 1" })).toHaveValue("resource:10");
    await user.selectOptions(screen.getByRole("combobox", { name: "Giao cho, quy tắc 2" }), "An");
    expect(sent(send)).toEqual([REST_TO_AN, { ...WEB_TO_DESIGN, target: { type: "resource", uid: 10 } }]);
  });

  it("goes back after a refused edit and takes the next one", async () => {
    const send = vi.fn().mockResolvedValueOnce(undefined).mockResolvedValue({});
    const { user } = renderTab(team([WEB_TO_DESIGN]), send);
    const units = () => screen.getByRole("textbox", { name: "Đơn vị, quy tắc 1" });
    await user.clear(units());
    await user.type(units(), "2");
    await user.tab();
    // Refused: the units the engine holds come back.
    await waitFor(() => expect(units()).toHaveValue("1"));
    await user.click(screen.getByRole("checkbox", { name: "Mảng: App" }));
    expect(sent(send)).toEqual([{ ...WEB_TO_DESIGN, conditions: [{ field: 1, values: ["Web", "App"] }] }]);
  });

  it("sends nothing when the units or values read the same", async () => {
    const rule: AssignmentRule = { ...REST_TO_AN, conditions: [{ field: 3, values: ["a;b"] }] };
    const { send, user } = renderTab(team([rule]));
    const units = screen.getByRole("textbox", { name: "Đơn vị, quy tắc 1" });
    await user.clear(units);
    await user.type(units, "0,50");
    await user.tab();
    // A value with a semicolon is left as it is when its box is only passed through.
    await user.click(screen.getByRole("textbox", { name: "Giá trị của Nhãn" }));
    await user.tab();
    expect(send).not.toHaveBeenCalled();
  });
});

describe("parseValues", () => {
  it("reads numbers with either decimal mark, apart by semicolons", () => {
    expect(parseValues(SIZE, "1,5; 2.25;;")).toEqual([1.5, 2.25]);
    expect(parseValues(SIZE, "1; x")).toBeNull();
    expect(parseValues(SIZE, " ; ")).toBeNull();
  });

  it("keeps text trimmed and each value once", () => {
    expect(parseValues(TAG, " a, b ; a ; a, b")).toEqual(["a, b", "a"]);
  });
});
