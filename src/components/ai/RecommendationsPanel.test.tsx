import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeResource, makeTask } from "../../test/fixtures";
import type { Project, RiskReport } from "../../types";
import { RecommendationsPanel } from "./RecommendationsPanel";

const project = makeProject({
  tasks: [makeTask({ uid: 1, name: "Thiết kế" }), makeTask({ uid: 2, name: "Lập trình" }), makeTask({ uid: 4, name: "Bàn giao" })],
  resources: [makeResource({ uid: 7, name: "An" })],
});

const report: RiskReport = {
  cards: [
    {
      severity: "high",
      title: "T4 trễ deadline",
      what: "T4 hết hạn 08/01 nhưng xong 09/01.",
      why: "T1 và T2 nối tiếp trên R7, không có độ chùng.",
      chain: ["T1", "T2", "T4"],
      focus: "Rút ngắn T2.",
      tasks: [],
      people: ["R7"],
    },
  ],
  input_tokens: 12345,
  output_tokens: 678,
  requests: 3,
  truncated: true,
  omitted_tasks: 120,
  omitted_resources: 4,
};

function enable() {
  fakeBackend.ai = { enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true };
}

function open(overrides: Partial<{ aiPro: boolean; project: Project }> = {}) {
  const handlers = { onOpenLicense: vi.fn(), onOpenSettings: vi.fn(), onPickTask: vi.fn(), onApplied: vi.fn(), onOpenWbs: vi.fn(), onClose: vi.fn() };
  const view = render(
    <RecommendationsPanel project={overrides.project ?? project} aiPro={overrides.aiPro ?? true} settingsKey={0} {...handlers} />,
  );
  return { ...handlers, ...view };
}

const calls = (cmd: string) => fakeBackend.calls.filter((call) => call.cmd === cmd);

describe("RecommendationsPanel", () => {
  it("introduces the feature and offers the license without Pro, reading nothing", async () => {
    fakeBackend.reset();
    const { onOpenLicense } = open({ aiPro: false });
    expect(screen.getByRole("complementary", { name: "Gợi ý AI" })).toHaveTextContent("Gợi ý AI Pro");
    expect(screen.getByText(/thuộc Sheafly Pro/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Kích hoạt Sheafly Pro…" }));
    expect(onOpenLicense).toHaveBeenCalledOnce();
    expect(calls("ai_settings")).toEqual([]);
    expect(screen.queryByRole("button", { name: "Giải thích rủi ro" })).not.toBeInTheDocument();
  });

  it("points to the AI settings while AI is off", async () => {
    fakeBackend.reset();
    const { onOpenSettings } = open();
    await userEvent.click(await screen.findByRole("button", { name: "Mở thiết lập Gợi ý AI…" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
    expect(screen.getByText(/Trợ giúp → Gợi ý AI/)).toBeInTheDocument();
    expect(calls("ai_risks")).toEqual([]);
  });

  it("explains the risks with real names, a chain to pick from, the tokens and a shortened summary", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiRisks = report;
    const { onPickTask } = open();
    await userEvent.click(await screen.findByRole("button", { name: "Giải thích rủi ro" }));
    const card = await screen.findByRole("article");
    expect(card).toHaveTextContent("NặngBàn giao trễ deadline");
    expect(card).toHaveTextContent("Thiết kế và Lập trình nối tiếp trên An");
    expect(card).not.toHaveTextContent("T4");
    expect(card).not.toHaveTextContent("R7");
    const chain = card.querySelectorAll(".ai-chain button");
    expect([...chain].map((button) => button.textContent)).toEqual(["Thiết kế", "Lập trình", "Bàn giao"]);
    await userEvent.click(chain[1] as HTMLElement);
    expect(onPickTask).toHaveBeenCalledWith(2);
    expect(screen.getByText("Đã dùng 12.345 token vào, 678 token ra (3 yêu cầu).")).toBeInTheDocument();
    expect(screen.getByText(/bỏ 120 việc và 4 người/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hỏi lại" })).toBeEnabled();
  });

  it("says when the project changed since it asked", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiRisks = report;
    const { rerender, ...handlers } = open();
    await userEvent.click(await screen.findByRole("button", { name: "Giải thích rủi ro" }));
    await screen.findByRole("article");
    expect(screen.queryByText(/Dự án đã đổi/)).not.toBeInTheDocument();
    const edited = { ...project, name: "Đã sửa" };
    rerender(<RecommendationsPanel project={edited} aiPro settingsKey={0} {...handlers} />);
    expect(screen.getByText(/Dự án đã đổi từ lúc hỏi/)).toBeInTheDocument();
  });

  it("stops a running request and drops its late answer", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiRisks = report;
    open();
    const explain = await screen.findByRole("button", { name: "Giải thích rủi ro" });
    const release = fakeBackend.holdNext("ai_risks");
    await userEvent.click(explain);
    expect(screen.getByRole("status")).toHaveTextContent("AI đang đọc lịch…");
    await userEvent.click(screen.getByRole("button", { name: "Dừng" }));
    expect(calls("ai_cancel")).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent("Đã dừng.");
    await act(async () => release());
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
  });

  it("stops the request when the panel closes", async () => {
    fakeBackend.reset();
    enable();
    const { unmount } = open();
    const explain = await screen.findByRole("button", { name: "Giải thích rủi ro" });
    fakeBackend.holdNext("ai_risks");
    await userEvent.click(explain);
    unmount();
    expect(calls("ai_cancel")).toHaveLength(1);
  });

  it("shows why a request failed, and a stopped one as stopped", async () => {
    fakeBackend.reset();
    enable();
    open();
    fakeBackend.failNext("ai_risks", "Không kết nối được tới Anthropic");
    await userEvent.click(await screen.findByRole("button", { name: "Giải thích rủi ro" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Không lấy được gợi ý: Không kết nối được tới Anthropic");
    fakeBackend.aiRisks = null;
    await userEvent.click(screen.getByRole("button", { name: "Giải thích rủi ro" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Đã dừng."));
  });

  it("speaks English", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiRisks = { ...report, requests: 1, truncated: false };
    render(
      <I18nProvider lang="en">
        <RecommendationsPanel project={project} aiPro settingsKey={0} onOpenLicense={vi.fn()} onOpenSettings={vi.fn()} onPickTask={vi.fn()} onApplied={vi.fn()} onOpenWbs={vi.fn()} onClose={vi.fn()} />
      </I18nProvider>,
    );
    await userEvent.click(await screen.findByRole("button", { name: "Explain risks" }));
    expect(await screen.findByRole("article")).toHaveTextContent("High");
    expect(screen.getByText("Used 12,345 input and 678 output tokens (1 request).")).toBeInTheDocument();
    expect(screen.queryByText(/shortened/)).not.toBeInTheDocument();
  });

  it("checks the estimates: cards, then proposals the engine tries", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiEstimates = {
      cards: [{ ...report.cards[0], title: "T2 lệch effort", chain: [], people: [] }],
      proposals: [{ id: "toolu_1", goal: { kind: "estimate", task: 2 }, commands: [{ type: "set_work", uid: 2, minutes: 960 }], level: false, reason: "Như T1." }],
      note: "",
      input_tokens: 10,
      output_tokens: 5,
      requests: 2,
      truncated: false,
      omitted_tasks: 0,
      omitted_resources: 0,
    };
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Rà soát ước lượng" }));
    expect(await screen.findByRole("heading", { name: "Lập trình lệch effort" })).toBeInTheDocument();
    expect(await screen.findByText("Sửa ước lượng: Lập trình")).toBeInTheDocument();
    await waitFor(() => expect(calls("ai_trial")).toHaveLength(1));
    expect(calls("ai_estimates")).toHaveLength(1);
  });

  it("says when the estimates look fine", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiEstimates = { ...fakeBackend.aiEstimates!, note: "Ước lượng ổn." };
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Rà soát ước lượng" }));
    expect(await screen.findByText("AI không thấy vấn đề: Ước lượng ổn.")).toBeInTheDocument();
  });

  it("writes a status report with real names, to copy or save as Markdown", async () => {
    fakeBackend.reset();
    enable();
    fakeBackend.aiStatus = {
      ...fakeBackend.aiStatus!,
      report: { title: "Tuần 2", markdown: "# Tuần 2\n\n## Rủi ro\n\n- **T4** trễ vì R7 bận.\n" },
    };
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Báo cáo tình trạng" }));
    const article = await screen.findByRole("article", { name: "Báo cáo tình trạng" });
    expect(article).toHaveTextContent("Bàn giao trễ vì An bận.");
    expect(article).not.toHaveTextContent("**");
    await userEvent.click(screen.getByRole("button", { name: "Sao chép" }));
    expect(writeText).toHaveBeenCalledWith("# Tuần 2\n\n## Rủi ro\n\n- **Bàn giao** trễ vì An bận.\n");
    expect(await screen.findByText("Đã sao chép báo cáo (có tên thật).")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Lưu Markdown…" }));
    expect(calls("save_text_report")[0].args).toEqual({ text: "# Tuần 2\n\n## Rủi ro\n\n- **Bàn giao** trễ vì An bận.\n", fileName: "Tuan-2.md" });
  });

  it("opens the WBS dialog from its button", async () => {
    fakeBackend.reset();
    enable();
    const { onOpenWbs } = open();
    await userEvent.click(await screen.findByRole("button", { name: "Dựng WBS từ mô tả…" }));
    expect(onOpenWbs).toHaveBeenCalledOnce();
  });
});
