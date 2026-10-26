import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeAssignment, makeProject, makeResource, makeTask } from "../../test/fixtures";
import type { FixReport, Project, Proposal, ProposalTrial } from "../../types";
import { RecommendationsPanel } from "./RecommendationsPanel";

const project = makeProject({
  tasks: [makeTask({ uid: 1, name: "Thiết kế" }), makeTask({ uid: 3, name: "Viết tài liệu" })],
  resources: [makeResource({ uid: 1, name: "An" })],
  assignments: [makeAssignment({ uid: 5, task_uid: 3, resource_uid: 1 })],
});

const remove: Proposal = {
  id: "toolu_11",
  goal: { kind: "overallocation", resource: 1 },
  commands: [{ type: "remove_assignment", uid: 5 }],
  level: false,
  reason: "T3 trùng T1 trên R1.",
};
const longer: Proposal = {
  id: "toolu_61",
  goal: { kind: "finish" },
  commands: [{ type: "set_duration", uid: 1, minutes: 4800 }],
  level: false,
  reason: "Cho T1 thêm thời gian.",
};
const missing: Proposal = {
  id: "toolu_41",
  goal: { kind: "finish" },
  commands: [{ type: "set_duration", uid: 9, minutes: 480 }],
  level: false,
  reason: "Rút ngắn T9.",
};

function fixes(proposals: Proposal[], note = ""): FixReport {
  return { proposals, note, input_tokens: 4000, output_tokens: 300, requests: 2, truncated: false, omitted_tasks: 0, omitted_resources: 0 };
}

const worse: ProposalTrial = {
  commands: longer.commands,
  assignments: [],
  moves: [],
  late: [],
  overallocated: [],
  before: { finish: 0, late: 0, over_minutes: 0, critical: 1 },
  after: { finish: 0, late: 1, over_minutes: 0, critical: 1 },
  target_before: 0,
  target_after: 0,
  verdict: "worse",
  worse: ["late"],
};

function setUp(report: FixReport) {
  fakeBackend.reset();
  fakeBackend.ai = { enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true };
  fakeBackend.aiFixes = report;
}

function panel(at: Project = project, onApplied = vi.fn()) {
  return <RecommendationsPanel project={at} aiPro settingsKey={0} onOpenLicense={vi.fn()} onOpenSettings={vi.fn()} onPickTask={vi.fn()} onApplied={onApplied} onOpenWbs={vi.fn()} onClose={vi.fn()} />;
}

const calls = (cmd: string) => fakeBackend.calls.filter((call) => call.cmd === cmd);

async function ask() {
  await userEvent.click(await screen.findByRole("button", { name: "Sửa quá tải và trễ hạn" }));
}

describe("AI proposals", () => {
  it("shows each proposal with names, its trial and Apply, and applies exactly what was tried", async () => {
    setUp(fixes([remove]));
    const onApplied = vi.fn();
    render(panel(project, onApplied));
    await ask();
    const card = await screen.findByRole("article");
    expect(card).toHaveTextContent("Bớt quá tải: An");
    expect(card).toHaveTextContent("Bỏ An khỏi Viết tài liệu");
    expect(card).toHaveTextContent("Viết tài liệu trùng Thiết kế trên An.");
    expect(card).not.toHaveTextContent("T3");
    expect(await within(card).findByText("Cải thiện")).toBeInTheDocument();
    const table = within(card).getByRole("table", { name: "Kết quả chạy thử" });
    expect(within(table).getByRole("row", { name: /Giờ quá tải/ })).toHaveTextContent("80");
    expect(calls("apply_command")).toEqual([]);

    await userEvent.click(within(card).getByRole("button", { name: "Áp dụng" }));
    expect(calls("ai_apply")[0].args).toEqual({ id: "toolu_11", commands: remove.commands });
    expect(onApplied).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole("article")).not.toBeInTheDocument());
  });

  it("drops proposals that are no better or that the engine refuses, without Apply", async () => {
    setUp(fixes([remove, longer, missing]));
    fakeBackend.aiTrials = { toolu_61: worse, toolu_41: "Không tìm thấy công việc" };
    render(panel());
    await ask();
    const dropped = (await screen.findByText("Đã loại (2)")).closest("details") as HTMLElement;
    await userEvent.click(within(dropped).getByText("Đã loại (2)"));
    const cards = within(dropped).getAllByRole("article");
    expect(cards[0]).toHaveTextContent("Không tốt hơn");
    expect(cards[1]).toHaveTextContent("Engine không áp dụng được đề xuất này: Không tìm thấy công việc");
    for (const card of cards) expect(within(card).queryByRole("button", { name: "Áp dụng" })).not.toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Các đề xuất" }).children).toHaveLength(1);
  });

  it("tries again when the project changes and waits for that trial before Apply", async () => {
    setUp(fixes([remove]));
    const { rerender } = render(panel());
    await ask();
    const apply = await screen.findByRole("button", { name: "Áp dụng" });
    expect(calls("ai_trial")).toHaveLength(1);
    const release = fakeBackend.holdNext("ai_trial");
    rerender(panel({ ...project, name: "Đã sửa" }));
    await waitFor(() => expect(calls("ai_trial")).toHaveLength(2));
    expect(apply).toBeDisabled();
    await act(async () => release());
    await waitFor(() => expect(screen.getByRole("button", { name: "Áp dụng" })).toBeEnabled());
  });

  it("says why there is nothing to propose, and Dismiss removes a card", async () => {
    setUp(fixes([], "Không có gì trễ."));
    render(panel());
    await ask();
    expect(await screen.findByText("AI không có đề xuất nào: Không có gì trễ.")).toBeInTheDocument();

    setUp(fixes([remove]));
    render(panel());
    await userEvent.click((await screen.findAllByRole("button", { name: "Sửa quá tải và trễ hạn" }))[1]);
    await userEvent.click(await screen.findByRole("button", { name: "Bỏ qua" }));
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    expect(calls("ai_apply")).toEqual([]);
  });

  it("shows an apply the backend refused on the card", async () => {
    setUp(fixes([remove]));
    render(panel());
    await ask();
    fakeBackend.failNext("ai_apply", "Dự án đã thay đổi từ lúc chạy thử đề xuất. Hãy đợi kết quả chạy thử mới.");
    await userEvent.click(await screen.findByRole("button", { name: "Áp dụng" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Không áp dụng được: Dự án đã thay đổi");
  });

  it("speaks English", async () => {
    setUp(fixes([remove]));
    render(<I18nProvider lang="en">{panel()}</I18nProvider>);
    await userEvent.click(await screen.findByRole("button", { name: "Fix overallocation and deadlines" }));
    const card = await screen.findByRole("article");
    expect(card).toHaveTextContent("Relieve: An");
    expect(card).toHaveTextContent("Take An off Viết tài liệu");
    expect(await within(card).findByText("Improves")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Apply" })).toBeEnabled();
  });
});
