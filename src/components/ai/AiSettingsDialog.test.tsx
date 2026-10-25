import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { AiSettingsDialog } from "./AiSettingsDialog";

const setCalls = () => fakeBackend.calls.filter((call) => call.cmd === "ai_set_settings");

function open(aiPro = true, onOpenLicense = vi.fn()) {
  render(<AiSettingsDialog aiPro={aiPro} onClose={() => {}} onOpenLicense={onOpenLicense} />);
  return onOpenLicense;
}

describe("AiSettingsDialog", () => {
  it("turns AI on only after the data notice is accepted", async () => {
    fakeBackend.reset();
    open();
    const enable = await screen.findByRole("checkbox", { name: /Bật gợi ý AI/ });
    expect(enable).not.toBeChecked();

    await userEvent.click(enable);
    const notice = screen.getByRole("region", { name: "Dữ liệu được gửi đi" });
    expect(notice).toHaveTextContent("Không gửi ghi chú");
    expect(setCalls()).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Không bật" }));
    expect(screen.queryByRole("region", { name: "Dữ liệu được gửi đi" })).not.toBeInTheDocument();
    expect(enable).not.toBeChecked();
    expect(setCalls()).toEqual([]);

    await userEvent.click(enable);
    await userEvent.click(screen.getByRole("button", { name: "Đồng ý và bật" }));
    await waitFor(() => expect(enable).toBeChecked());
    expect(fakeBackend.ai).toEqual({ enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true });

    // Once accepted, turning it off and on again asks nothing more.
    await userEvent.click(enable);
    await waitFor(() => expect(enable).not.toBeChecked());
    await userEvent.click(enable);
    await waitFor(() => expect(enable).toBeChecked());
    expect(screen.queryByRole("region", { name: "Dữ liệu được gửi đi" })).not.toBeInTheDocument();
  });

  it("saves the key once, clears the field and never shows it back", async () => {
    fakeBackend.reset();
    open();
    expect(await screen.findByText("Chưa lưu khóa API.")).toBeInTheDocument();
    const field = screen.getByLabelText("Khóa API Anthropic");
    expect(field).toHaveAttribute("type", "password");
    await userEvent.type(field, "sk-ant-test");
    await userEvent.click(screen.getByRole("button", { name: "Lưu khóa" }));
    expect(await screen.findByText(/Đã lưu khóa API trong kho khóa/)).toBeInTheDocument();
    expect(field).toHaveValue("");
    expect(fakeBackend.calls.find((call) => call.cmd === "ai_save_key")?.args).toEqual({ key: "sk-ant-test" });

    await userEvent.click(screen.getByRole("button", { name: "Quên khóa" }));
    expect(await screen.findByText("Chưa lưu khóa API.")).toBeInTheDocument();
    expect(fakeBackend.aiKey).toBe(false);
  });

  it("chooses a model by name or by ID, and masks names", async () => {
    fakeBackend.reset();
    open();
    const model = await screen.findByLabelText("Model");
    await userEvent.selectOptions(model, "claude-sonnet-5-5");
    await waitFor(() => expect(fakeBackend.ai.model).toBe("claude-sonnet-5-5"));

    await userEvent.selectOptions(model, "other");
    const id = screen.getByLabelText("ID model");
    await userEvent.type(id, "claude-haiku-5-5{Enter}");
    await waitFor(() => expect(fakeBackend.ai.model).toBe("claude-haiku-5-5"));

    await userEvent.click(screen.getByRole("checkbox", { name: /Ẩn tên việc/ }));
    await waitFor(() => expect(fakeBackend.ai.mask).toBe(true));
    expect(fakeBackend.ai.model).toBe("claude-haiku-5-5");
  });

  it("checks the connection when licensed, and opens the license dialog otherwise", async () => {
    fakeBackend.reset();
    fakeBackend.ai = { enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true };
    fakeBackend.aiKey = true;
    open();
    const test = await screen.findByRole("button", { name: /Kiểm tra kết nối/ });
    await waitFor(() => expect(test).toBeEnabled());
    await userEvent.click(test);
    expect(await screen.findByRole("status")).toHaveTextContent("Kết nối được với claude-opus-5-5 (15 token).");
  });

  it("sends a Free user to the license dialog without calling the provider", async () => {
    fakeBackend.reset();
    fakeBackend.ai = { enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true };
    fakeBackend.aiKey = true;
    const onOpenLicense = open(false);
    const test = await screen.findByRole("button", { name: /Kiểm tra kết nối/ });
    await waitFor(() => expect(test).toBeEnabled());
    await userEvent.click(test);
    expect(onOpenLicense).toHaveBeenCalledOnce();
    expect(fakeBackend.calls.some((call) => call.cmd === "ai_test")).toBe(false);
  });

  it("cannot check the connection while AI is off or no key is saved", async () => {
    fakeBackend.reset();
    open();
    expect(await screen.findByRole("button", { name: /Kiểm tra kết nối/ })).toBeDisabled();
  });

  it("shows an error the backend answers with", async () => {
    fakeBackend.reset();
    fakeBackend.ai = { enabled: true, model: "claude-opus-5-5", mask: false, notice_accepted: true };
    fakeBackend.aiKey = true;
    fakeBackend.failNext("ai_test", "Anthropic không nhận khóa API này (sai khóa, đã thu hồi hoặc không đủ quyền)");
    open();
    const test = await screen.findByRole("button", { name: /Kiểm tra kết nối/ });
    await waitFor(() => expect(test).toBeEnabled());
    await userEvent.click(test);
    expect(await screen.findByRole("alert")).toHaveTextContent("Anthropic không nhận khóa API này");
  });

  it("shows what is in use when a change is refused", async () => {
    fakeBackend.reset();
    fakeBackend.failNext("ai_set_settings", "Không lưu được cài đặt");
    open();
    const mask = await screen.findByRole("checkbox", { name: /Ẩn tên việc/ });
    await userEvent.click(mask);
    expect(await screen.findByRole("alert")).toHaveTextContent("Không lưu được cài đặt");
    await waitFor(() => expect(mask).not.toBeChecked());
    expect(fakeBackend.ai.mask).toBe(false);
  });

  it("keeps a click made right after typing a model ID", async () => {
    fakeBackend.reset();
    open();
    await userEvent.selectOptions(await screen.findByLabelText("Model"), "other");
    await userEvent.type(screen.getByLabelText("ID model"), "claude-haiku-5-5");
    // Clicking the mask box blurs the ID field, which saves the model first.
    await userEvent.click(screen.getByRole("checkbox", { name: /Ẩn tên việc/ }));
    await waitFor(() => expect(fakeBackend.ai).toMatchObject({ model: "claude-haiku-5-5", mask: true }));
  });

  it("speaks English", async () => {
    fakeBackend.reset();
    render(
      <I18nProvider lang="en">
        <AiSettingsDialog aiPro onClose={() => {}} onOpenLicense={() => {}} />
      </I18nProvider>,
    );
    expect(await screen.findByRole("checkbox", { name: /Turn on AI recommendations/ })).toBeInTheDocument();
    expect(screen.getByLabelText("Anthropic API key")).toBeInTheDocument();
  });
});
