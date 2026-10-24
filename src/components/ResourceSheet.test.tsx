import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { I18nProvider, setCurrentLang, type Lang } from "../i18n";
import { makeResource, standardCalendar } from "../test/fixtures";
import type { Overallocation, Resource } from "../types";
import { ResourceSheet } from "./ResourceSheet";

function renderSheet(
  resources: Resource[],
  overallocations: Overallocation[] = [],
  lang: Lang = "vi",
  capacityPro = true,
) {
  const handlers = {
    onAdd: vi.fn(),
    onChange: vi.fn(),
    onChangeEmail: vi.fn(),
    onDelete: vi.fn(),
    onEditCalendar: vi.fn(),
    onCreateCalendar: vi.fn(),
    onEditAvailability: vi.fn(),
    onChangeGroup: vi.fn(),
    onChangeFocus: vi.fn(),
  };
  render(
    <I18nProvider lang={lang}>
      <ResourceSheet
        resources={resources}
        calendars={[standardCalendar(1)]}
        skills={SKILLS}
        overallocations={overallocations}
        capacityPro={capacityPro}
        {...handlers}
      />
    </I18nProvider>,
  );
  return { handlers, user: userEvent.setup() };
}

const SKILLS = [
  { id: 1, name: "Thiết kế" },
  { id: 2, name: "Kiểm thử" },
];
const an = makeResource({ uid: 10, name: "An" });
const crane = makeResource({ uid: 11, name: "Cẩu", kind: "machine" });
const over: Overallocation[] = [{ resource_uid: 10, date: "2026-01-05", units: 1.5, max_units: 1 }];

describe("ResourceSheet", () => {
  it("edits a resource's role on leaving it and filters by role", async () => {
    const { handlers, user } = renderSheet([makeResource({ uid: 12, name: "Bình", group: "Kiểm thử" }), an]);
    const role = screen.getByRole("textbox", { name: "Vai trò của An" });
    await user.type(role, "  Backend ");
    await user.tab();
    expect(handlers.onChangeGroup).toHaveBeenCalledWith(10, "Backend");
    await user.selectOptions(screen.getByRole("combobox", { name: "Vai trò" }), "Kiểm thử");
    expect(screen.queryByRole("textbox", { name: "Vai trò của An" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Vai trò của Bình" })).toHaveValue("Kiểm thử");
    await user.selectOptions(screen.getByRole("combobox", { name: "Vai trò" }), "Chưa có vai trò");
    expect(screen.getByRole("textbox", { name: "Vai trò của An" })).toBeInTheDocument();
  });

  it("sets a resource's own productivity and clears it back to the project's", async () => {
    const { handlers, user } = renderSheet([makeResource({ uid: 12, name: "Bình", focus: 0.9 }), an, crane]);
    const own = screen.getByRole("textbox", { name: "Năng suất của Bình" });
    expect(own).toHaveValue("90%");
    // Empty fields show what they follow: the project for a person, 100% for a machine.
    expect(screen.getByRole("textbox", { name: "Năng suất của An" })).toHaveAttribute("placeholder", "100%");
    await user.clear(own);
    await user.tab();
    expect(handlers.onChangeFocus).toHaveBeenLastCalledWith(12, null);
    const an_ = screen.getByRole("textbox", { name: "Năng suất của An" });
    await user.type(an_, "0,75");
    await user.tab();
    expect(handlers.onChangeFocus).toHaveBeenLastCalledWith(10, 0.75);
  });

  it("only clears an own productivity without Sheafly Pro", async () => {
    const { handlers, user } = renderSheet([makeResource({ uid: 12, name: "Bình", focus: 0.9 }), an], [], "vi", false);
    const an_ = screen.getByRole("textbox", { name: "Năng suất của An" });
    await user.type(an_, "80");
    await user.tab();
    expect(handlers.onChangeFocus).not.toHaveBeenCalled();
    expect(an_).toHaveAttribute("aria-invalid", "true");
    expect(an_).toHaveAttribute("title", expect.stringContaining("Cần Sheafly Pro"));
    await user.clear(screen.getByRole("textbox", { name: "Năng suất của Bình" }));
    await user.tab();
    expect(handlers.onChangeFocus).toHaveBeenLastCalledWith(12, null);
  });

  it("shows each resource's skills by name, in the project's order", () => {
    renderSheet([makeResource({ uid: 12, name: "Bình", skills: [1, 2] }), an]);
    expect(screen.getByRole("columnheader", { name: "Kỹ năng" })).toBeInTheDocument();
    expect(screen.getByText("Thiết kế, Kiểm thử")).toBeInTheDocument();
  });

  it("edits the email that matches people at a task tracker", async () => {
    const { handlers, user } = renderSheet([makeResource({ uid: 10, name: "An", email: "an@example.com" }), crane]);
    const email = screen.getByRole("textbox", { name: "Email của An" });
    expect(email).toHaveValue("an@example.com");
    expect(screen.getByRole("textbox", { name: "Email của Cẩu" })).toHaveValue("");
    await user.clear(email);
    await user.type(email, "  an.nguyen@example.com ");
    await user.tab();
    expect(handlers.onChangeEmail).toHaveBeenCalledExactlyOnceWith(10, "an.nguyen@example.com");
    // Leaving a field as it was sends nothing.
    handlers.onChangeEmail.mockClear();
    await user.click(screen.getByRole("textbox", { name: "Email của Cẩu" }));
    await user.tab();
    expect(handlers.onChangeEmail).not.toHaveBeenCalled();
  });

  it("reads max units with a comma or as hours per day", async () => {
    const { handlers, user } = renderSheet([an]);
    const units = screen.getByRole("textbox", { name: "Đơn vị tối đa của An" });
    await user.clear(units);
    await user.type(units, "0,5");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, max_units: 0.5 });
    await user.clear(units);
    await user.type(units, "4h/ngày");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, max_units: 0.5 });
    await user.clear(units);
    await user.type(units, "nhiều");
    await user.tab();
    expect(handlers.onChange).toHaveBeenCalledTimes(2);
    expect(units).toHaveValue("1");
  });

  it("opens a resource's availability and marks one that has periods", async () => {
    const away = makeResource({ uid: 12, name: "Bình", availability: [{ from: "2026-01-12", to: null, units: 0 }] });
    const { handlers, user } = renderSheet([an, away]);
    expect(screen.getAllByText("theo ngày")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Khả dụng của Bình" }));
    expect(handlers.onEditAvailability).toHaveBeenCalledWith(12);
  });

  it("shows the Vietnamese headers and flags overallocated resources", () => {
    renderSheet([an, crane], over);
    const headers = screen.getAllByRole("columnheader").map((cell) => cell.textContent);
    expect(headers).toEqual([
      "Tên", "Loại", "Vai trò", "Đơn vị tối đa", "Năng suất Pro", "Lịch", "Giá chuẩn / giờ", "Giá tăng ca", "Chi phí mỗi lần", "Email", "Kỹ năng", "Quá tải", "",
    ]);
    expect(screen.getByText("Có")).toBeInTheDocument();
    expect(screen.getByText("Không")).toBeInTheDocument();
    expect(screen.getByLabelText("Lịch An")).toBeInTheDocument();
  });

  it("says when there are no resources", () => {
    renderSheet([]);
    expect(screen.getByText("Chưa có nguồn lực.")).toBeInTheDocument();
  });

  it("sends the whole resource when a cell is committed", async () => {
    const { handlers, user } = renderSheet([an]);
    const [name, , units, , rate, overtime, perUse] = screen.getAllByRole("textbox");
    await user.clear(name);
    await user.type(name, "Bình");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, name: "Bình" });
    await user.clear(units);
    await user.type(units, "0.5");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, max_units: 0.5 });
    await user.clear(rate);
    await user.type(rate, "120000");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, standard_rate: 120000 });
    // Text that is not an amount is refused, not saved as 0.
    const calls = handlers.onChange.mock.calls.length;
    await user.clear(overtime);
    await user.type(overtime, "abc");
    await user.tab();
    expect(handlers.onChange).toHaveBeenCalledTimes(calls);
    expect(overtime).toHaveAttribute("aria-invalid", "true");
    await user.clear(overtime);
    await user.type(overtime, "1.500.000");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, overtime_rate: 1_500_000 });
    await user.clear(perUse);
    await user.type(perUse, "50000");
    await user.tab();
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, cost_per_use: 50000 });
  });

  it("does not send an unchanged name", async () => {
    const { handlers, user } = renderSheet([an]);
    await user.click(screen.getAllByRole("textbox")[0]);
    await user.tab();
    expect(handlers.onChange).not.toHaveBeenCalled();
  });

  it("changes the kind and the calendar, and opens the calendar editors", async () => {
    const own = makeResource({ uid: 12, name: "Chi", calendar_id: 1 });
    const { handlers, user } = renderSheet([an, own]);
    await user.selectOptions(screen.getAllByRole("combobox")[0], "machine");
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, kind: "machine" });
    await user.selectOptions(screen.getByLabelText("Lịch An"), "1");
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...an, calendar_id: 1 });
    await user.selectOptions(screen.getByLabelText("Lịch Chi"), "");
    expect(handlers.onChange).toHaveBeenLastCalledWith({ ...own, calendar_id: null });
    const [editAn, editChi] = screen.getAllByRole("button", { name: "Sửa" });
    // A resource on the project calendar has no calendar of its own to edit.
    expect(editAn).toBeDisabled();
    await user.click(editChi);
    expect(handlers.onEditCalendar).toHaveBeenCalledExactlyOnceWith(1);
    await user.click(screen.getAllByRole("button", { name: "Tạo lịch" })[0]);
    expect(handlers.onCreateCalendar).toHaveBeenCalledExactlyOnceWith(an);
  });
});

describe("ResourceSheet in English", () => {
  afterEach(() => setCurrentLang("vi"));

  it("uses Project's English column names", () => {
    renderSheet([an, crane], over, "en");
    const headers = screen.getAllByRole("columnheader").map((cell) => cell.textContent);
    expect(headers).toEqual([
      "Name", "Type", "Role", "Max units", "Productivity Pro", "Calendar", "Std. rate / hr", "Ovt. rate", "Cost/use", "Email", "Skills", "Overallocated", "",
    ]);
    expect(screen.getByText("Yes")).toBeInTheDocument();
    expect(screen.getByText("No")).toBeInTheDocument();
    expect(screen.getAllByRole("option", { name: "Person" })).toHaveLength(2);
    expect(screen.getAllByRole("option", { name: "Equipment" })).toHaveLength(2);
    expect(screen.getByLabelText("An calendar")).toBeInTheDocument();
    expect(screen.getAllByRole("option", { name: "Project calendar" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Edit" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "New calendar" })).toHaveLength(2);
  });

  it("adds and deletes resources from English buttons", async () => {
    const { handlers, user } = renderSheet([an], [], "en");
    await user.click(screen.getByRole("button", { name: "Add person" }));
    await user.click(screen.getByRole("button", { name: "Add equipment" }));
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(handlers.onAdd.mock.calls).toEqual([["person"], ["machine"]]);
    expect(handlers.onDelete).toHaveBeenCalledExactlyOnceWith(10);
  });

  it("says when there are no resources", () => {
    renderSheet([], [], "en");
    expect(screen.getByText("No resources yet.")).toBeInTheDocument();
  });
});
