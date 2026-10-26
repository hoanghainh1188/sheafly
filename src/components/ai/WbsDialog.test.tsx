import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeResource, makeTask } from "../../test/fixtures";
import type { Command, ImportTask, Project } from "../../types";
import { WbsDialog } from "./WbsDialog";

const project = makeProject({
  tasks: [makeTask({ uid: 1, name: "Thiết kế" })],
  resources: [makeResource({ uid: 7, name: "An" })],
});

const rows: ImportTask[] = [
  { name: "Ví điện tử", level: 0 },
  { name: "Phân tích", level: 1, work: 720, assignees: [{ name: "An", units: 1 }] },
  { name: "Làm API", level: 1, work: 1440, after: [1] },
];

function open(at: Project = project) {
  const send = vi.fn(async (_command: Command) => fakeBackend.snapshot);
  const onClose = vi.fn();
  const view = render(<WbsDialog project={at} afterTask={at.tasks[0]} error={null} send={send} onClose={onClose} />);
  return { send, onClose, ...view };
}

const calls = (cmd: string) => fakeBackend.calls.filter((call) => call.cmd === cmd);

async function build(description = "Thêm ví điện tử") {
  await userEvent.type(screen.getByRole("textbox", { name: "Mô tả phần việc cần lập kế hoạch" }), description);
  await userEvent.click(screen.getByRole("button", { name: "Tạo WBS" }));
}

describe("WbsDialog", () => {
  it("builds rows, checks them with the engine and imports them in one import_rows", async () => {
    fakeBackend.reset();
    fakeBackend.aiWbs = { ...fakeBackend.aiWbs!, tasks: rows, note: "Giả định An phân tích." };
    const { send, onClose } = open();
    expect(screen.getByRole("button", { name: "Nhập 0 dòng" })).toBeDisabled();
    await build();
    expect(calls("ai_wbs")[0].args).toEqual({ description: "Thêm ví điện tử" });
    const table = await screen.findByRole("table", { name: "Các dòng sẽ nhập" });
    const lines = within(table).getAllByRole("row").slice(1).map((row) => row.textContent);
    expect(lines).toEqual(["1Ví điện tửNhóm", "2Phân tích1,5 ngàyAn", "3Làm API3 ngày2"]);
    expect(screen.getByText("Ghi chú của AI: Giả định An phân tích.")).toBeInTheDocument();
    expect(screen.getByText(/sau "Thiết kế"/)).toBeInTheDocument();
    const importButton = await screen.findByRole("button", { name: "Nhập 3 dòng" });
    await waitFor(() => expect(importButton).toBeEnabled());
    expect(calls("check_import")[0].args).toEqual({ afterUid: 1, resources: [], tasks: rows });
    await userEvent.click(importButton);
    expect(send).toHaveBeenCalledWith({ type: "import_rows", after_uid: 1, resources: [], tasks: rows });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("shows the engine's problems by row and keeps Import off", async () => {
    fakeBackend.reset();
    fakeBackend.aiWbs = { ...fakeBackend.aiWbs!, tasks: rows };
    fakeBackend.importProblems = [{ table: "tasks", row: 2, column: "after", message: "Liên kết tạo vòng lặp" }];
    const { send } = open();
    await build();
    expect(await screen.findByText("Dòng 3: Liên kết tạo vòng lặp")).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Làm API/ })).toHaveClass("bad");
    expect(screen.getByRole("button", { name: "Nhập 3 dòng" })).toBeDisabled();
    expect(send).not.toHaveBeenCalled();
  });

  it("checks again when the project changes", async () => {
    fakeBackend.reset();
    fakeBackend.aiWbs = { ...fakeBackend.aiWbs!, tasks: rows };
    const { rerender, send, onClose } = open();
    await build();
    await screen.findByRole("table");
    await waitFor(() => expect(calls("check_import")).toHaveLength(1));
    const edited = { ...project, name: "Đã sửa" };
    rerender(<WbsDialog project={edited} afterTask={edited.tasks[0]} error={null} send={send} onClose={onClose} />);
    await waitFor(() => expect(calls("check_import")).toHaveLength(2));
  });

  it("shows why a request failed, stops one running, and stops it on close", async () => {
    fakeBackend.reset();
    const { unmount } = open();
    fakeBackend.failNext("ai_wbs", "Hãy nhập mô tả phần việc cần lập kế hoạch");
    await build("x");
    expect(await screen.findByRole("alert")).toHaveTextContent("Không dựng được WBS: Hãy nhập mô tả");
    const release = fakeBackend.holdNext("ai_wbs");
    await userEvent.click(screen.getByRole("button", { name: "Tạo WBS" }));
    expect(screen.getByRole("status")).toHaveTextContent("AI đang dựng WBS…");
    await userEvent.click(screen.getByRole("button", { name: "Dừng" }));
    expect(calls("ai_cancel")).toHaveLength(1);
    await act(async () => release());
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    fakeBackend.holdNext("ai_wbs");
    await userEvent.click(screen.getByRole("button", { name: "Tạo WBS" }));
    unmount();
    expect(calls("ai_cancel")).toHaveLength(2);
  });
});
