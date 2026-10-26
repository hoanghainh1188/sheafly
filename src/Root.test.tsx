import { act, render, screen, waitFor } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { setCurrentLang } from "./i18n";
import { Root } from "./Root";
import { applyTheme } from "./theme";
import { fakeBackend } from "./test/fakeBackend";
import { makeProject, makeResource, makeSnapshot, makeTask } from "./test/fixtures";

describe("Root", () => {
  beforeEach(() => {
    // inTauri() looks for this; the fake backend answers the commands.
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      // The window plugin (setTitle) calls this directly rather than the mocked core invoke.
      invoke: (cmd: string, args?: Record<string, unknown>) => fakeBackend.invoke(cmd, args),
    };
  });
  afterEach(() => {
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    vi.restoreAllMocks();
    vi.mocked(listen).mockClear();
    setCurrentLang("vi");
    applyTheme("system");
  });

  it("uses the saved language and tells the backend to keep it", async () => {
    fakeBackend.language = "en";
    render(<Root />);
    expect(await screen.findByRole("button", { name: "Undo" })).toBeInTheDocument();
    expect(fakeBackend.calls).toContainEqual({ cmd: "set_language", args: { language: "en", remember: true } });
  });

  it("follows the system language without remembering it", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    expect(await screen.findByRole("button", { name: "Hoàn tác" })).toBeInTheDocument();
    await waitFor(() =>
      expect(fakeBackend.calls).toContainEqual({ cmd: "set_language", args: { language: "vi", remember: false } }),
    );
    expect(fakeBackend.language).toBeNull();
  });

  it("falls back to the system language when reading the saved choice fails", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-GB"]);
    fakeBackend.failNext("get_language", "settings unreadable");
    render(<Root />);
    expect(await screen.findByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("switches from the language menu, remembers it, and works with a dialog open", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    // A dialog covers the window: the language switch is still allowed.
    const dialog = document.createElement("div");
    dialog.setAttribute("aria-modal", "true");
    document.body.append(dialog);
    await act(async () => menu("lang-en"));
    expect(await screen.findByRole("button", { name: "Undo" })).toBeInTheDocument();
    expect(fakeBackend.calls).toContainEqual({ cmd: "set_language", args: { language: "en", remember: true } });
    expect(fakeBackend.language).toBe("en");
    dialog.remove();
  });

  it("shows an error when the choice cannot be stored", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    // Startup loads clear the banner when they succeed; let them finish first.
    await waitFor(() => expect(fakeBackend.calls.some((call) => call.cmd === "check_autosave")).toBe(true));
    await act(async () => {});
    fakeBackend.failNext("set_language", "Không ghi được cài đặt");
    await act(async () => menu("lang-en"));
    expect(await screen.findByText("Không ghi được cài đặt")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("starts with the saved theme", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.theme = "dark";
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("switches the theme from the menu, even with a dialog open, and remembers it", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    expect(document.documentElement.dataset.theme).toBe("light");
    const menu = await menuHandler();
    const dialog = document.createElement("div");
    dialog.setAttribute("aria-modal", "true");
    document.body.append(dialog);
    try {
      await act(async () => menu("theme-dark"));
      expect(document.documentElement.dataset.theme).toBe("dark");
      expect(fakeBackend.calls).toContainEqual({ cmd: "set_theme", args: { theme: "dark" } });
      expect(fakeBackend.theme).toBe("dark");
      // One key press can fire both a shortcut and a menu item; App acts once per 40 ms.
      await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
      await act(async () => menu("theme-system"));
      // jsdom has no dark system setting.
      expect(document.documentElement.dataset.theme).toBe("light");
      expect(fakeBackend.theme).toBe("system");
    } finally {
      dialog.remove();
    }
  });

  it("shows an error and goes back to the previous theme when it cannot be stored", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await waitFor(() => expect(fakeBackend.calls.some((call) => call.cmd === "check_autosave")).toBe(true));
    await act(async () => {});
    fakeBackend.failNext("set_theme", "Không ghi được cài đặt");
    await act(async () => menu("theme-dark"));
    expect(await screen.findByText("Không ghi được cài đặt")).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("switches views from the menu", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("view-resources"));
    expect(await screen.findByRole("button", { name: "Thêm người" })).toBeInTheDocument();
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    await act(async () => menu("view-gantt"));
    expect(await screen.findByRole("group", { name: /^Biểu đồ Gantt/ })).toBeInTheDocument();
  });

  it("opens the leveling order from the Project menu and sets it", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("leveling-order"));
    expect(await screen.findByRole("tab", { name: "Cân bằng" })).toHaveAttribute("aria-selected", "true");
    await act(async () => screen.getByRole("button", { name: "Theo ID" }).click());
    await waitFor(() =>
      expect(fakeBackend.commands()).toContainEqual({
        type: "set_leveling_order",
        order: [{ criterion: { type: "id" }, descending: false }],
      }),
    );
    expect(await screen.findByRole("combobox", { name: "Tiêu chí 1" })).toHaveValue("id");
  });

  it("edits the payment schedule from the Project menu and shows the cash flow view", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.reset(makeSnapshot(makeProject({ tasks: [makeTask({ uid: 1, id: 1, name: "Bàn giao", milestone: true })] })));
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("payment-schedule"));
    expect(await screen.findByRole("dialog", { name: "Lịch thanh toán" })).toBeInTheDocument();
    await act(async () => screen.getByRole("button", { name: "Thêm khoản thu" }).click());
    await waitFor(() => expect(fakeBackend.commands().map((command) => command.type)).toEqual(["set_payments"]));
    expect(await screen.findByRole("textbox", { name: "Tên" })).toHaveValue("Khoản thu 1");
    await act(async () => screen.getByRole("button", { name: "Lưu" }).click());
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Lịch thanh toán" })).not.toBeInTheDocument());
    // The inspector of the milestone lists the payment that follows it.
    expect(await screen.findByText("Khoản thu 1: Khi xong, Net 30")).toBeInTheDocument();
    // Free: the view introduces the feature and never asks for the numbers.
    await act(async () => menu("view-cashflow"));
    expect(await screen.findByText("Dòng tiền theo kỳ là tính năng Sheafly Pro")).toBeInTheDocument();
    expect(fakeBackend.calls.some((call) => call.cmd === "cash_flow")).toBe(false);
  });

  it("edits a resource's availability as one transaction", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.reset(makeSnapshot(makeProject({ resources: [makeResource({ uid: 1, name: "An" })] })));
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("view-resources"));
    await act(async () => (await screen.findByRole("button", { name: "Khả dụng của An" })).click());
    expect(await screen.findByRole("dialog", { name: "Khả dụng: An" })).toBeInTheDocument();
    await act(async () => screen.getByRole("button", { name: "Thêm khoảng" }).click());
    await waitFor(() =>
      expect(fakeBackend.commands().some((command) => command.type === "set_resource_availability")).toBe(true),
    );
    expect(await screen.findByRole("textbox", { name: "Đơn vị, khoảng 1" })).toHaveValue("0,5");
    await act(async () => screen.getByRole("button", { name: "Lưu" }).click());
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Khả dụng: An" })).not.toBeInTheDocument());
    expect(await screen.findByText("theo ngày")).toBeInTheDocument();
  });

  it("gives people skills and adds a rule in the Giao việc dialog", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.reset(makeSnapshot(makeProject({ resources: [makeResource({ uid: 1, name: "An" })] })));
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("assignment-rules"));
    expect(await screen.findByRole("dialog", { name: "Giao việc" })).toBeInTheDocument();
    await act(async () => screen.getByRole("tab", { name: "Kỹ năng" }).click());
    const name = screen.getByRole("textbox", { name: "Kỹ năng mới" });
    const user = userEvent.setup();
    await user.type(name, "Kiểm thử{Enter}");
    await user.click(await screen.findByRole("checkbox", { name: "An có Kiểm thử" }));
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "An có Kiểm thử" })).toBeChecked());
    await user.click(screen.getByRole("tab", { name: "Quy tắc" }));
    await user.click(screen.getByRole("button", { name: "Thêm quy tắc" }));
    expect(await screen.findByRole("combobox", { name: "Giao cho, quy tắc 1" })).toHaveValue("skill:1");
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Giao việc" })).not.toBeInTheDocument());
    await act(async () => menu("view-resources"));
    expect(await screen.findByRole("cell", { name: "Kiểm thử" })).toBeInTheDocument();
    expect(fakeBackend.commands().map((command) => command.type)).toEqual([
      "add_skill",
      "set_resource_skills",
      "set_assignment_rules",
    ]);
  });

  it("previews the plan for the suggestions and applies it in one step", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    const rule = { conditions: [], target: { type: "resource" as const, uid: 1 }, units: 1 };
    fakeBackend.reset(
      makeSnapshot(
        makeProject({
          tasks: [makeTask({ uid: 1, name: "Viết API" })],
          resources: [makeResource({ uid: 1, name: "An" })],
          assignment_rules: [rule],
        }),
      ),
    );
    fakeBackend.suggestions = [
      { task_uid: 1, rule: 0, units: 1, picks: [{ resource_uid: 1, start: 0, finish: 0 }], reason: null },
    ];
    render(<Root />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Giao theo quy tắc…" }));
    expect(await screen.findByRole("dialog", { name: "Giao việc theo quy tắc" })).toBeInTheDocument();
    expect(fakeBackend.calls.some((call) => call.cmd === "suggest_assignments")).toBe(true);
    const apply = await screen.findByRole("button", { name: "Áp dụng kế hoạch" });
    await waitFor(() => expect(apply).toBeEnabled());
    await user.click(apply);
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Giao việc theo quy tắc" })).not.toBeInTheDocument());
    const applied = fakeBackend.calls.filter((call) => call.cmd === "apply_plan");
    expect(applied).toHaveLength(1);
    expect(applied[0].args?.assignments).toEqual([{ task_uid: 1, resource_uid: 1, units: 1 }]);
    expect(fakeBackend.snapshot.project.assignments).toHaveLength(1);
  });

  it("bases a resource's new calendar on the project calendar", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.reset(makeSnapshot(makeProject({ resources: [makeResource({ uid: 1, name: "An" })] })));
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    const menu = await menuHandler();
    await act(async () => menu("view-resources"));
    await act(async () => (await screen.findByRole("button", { name: "Tạo lịch" })).click());
    await waitFor(() =>
      expect(fakeBackend.commands()).toContainEqual({ type: "create_calendar", name: "Lịch An", base_id: 1 }),
    );
    await waitFor(() =>
      expect(fakeBackend.commands().some((command) => command.type === "update_resource" && command.calendar_id === 2)).toBe(
        true,
      ),
    );
  });

  it("opens a file the app was launched with after the autosave offer", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await waitFor(() => expect(fakeBackend.calls.some((call) => call.cmd === "open_pending")).toBe(true));
    const order = fakeBackend.calls.map((call) => call.cmd);
    expect(order.indexOf("check_autosave")).toBeLessThan(order.indexOf("open_pending"));
  });

  it("still opens a queued file when the autosave cannot be restored", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    fakeBackend.failNext("check_autosave", "Không khôi phục được bản tự lưu");
    render(<Root />);
    await waitFor(() => expect(fakeBackend.calls.some((call) => call.cmd === "open_pending")).toBe(true));
    expect(await screen.findByText("Không khôi phục được bản tự lưu")).toBeInTheDocument();
  });

  it("opens a recent file from the menu and a file the system sends", async () => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["vi-VN"]);
    render(<Root />);
    await screen.findByRole("button", { name: "Hoàn tác" });
    await waitFor(() => expect(fakeBackend.calls.some((call) => call.cmd === "open_pending")).toBe(true));
    // Wait until the startup dialog work has finished, or the menu click is ignored as busy.
    await waitFor(() => expect(screen.getByRole("button", { name: /Xuất PDF/ })).toBeEnabled());
    const menu = await menuHandler();
    await act(async () => menu("recent:2"));
    await waitFor(() => expect(fakeBackend.calls).toContainEqual({ cmd: "open_recent", args: { index: 2 } }));
    const before = fakeBackend.calls.filter((call) => call.cmd === "open_pending").length;
    const openFile = await eventHandler("open-file");
    await act(async () => openFile({ payload: null }));
    await waitFor(() =>
      expect(fakeBackend.calls.filter((call) => call.cmd === "open_pending").length).toBe(before + 1),
    );
  });
});

/** The handler App registered for native menu clicks. */
async function menuHandler(): Promise<(id: string) => void> {
  let found: ((event: { payload: string }) => void) | undefined;
  await waitFor(() => {
    const call = vi.mocked(listen).mock.calls.find(([name]) => name === "menu-action");
    expect(call).toBeDefined();
    found = call?.[1] as unknown as (event: { payload: string }) => void;
  });
  return (id: string) => found?.({ payload: id });
}

/** The handler App registered for a backend event. */
async function eventHandler(name: string): Promise<(event: { payload: unknown }) => void> {
  let found: ((event: { payload: unknown }) => void) | undefined;
  await waitFor(() => {
    const call = vi.mocked(listen).mock.calls.find(([event]) => event === name);
    expect(call).toBeDefined();
    found = call?.[1] as unknown as (event: { payload: unknown }) => void;
  });
  return (event) => found?.(event);
}
