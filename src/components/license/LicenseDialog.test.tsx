import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeLicense, makeProLicense } from "../../test/fixtures";
import { LicenseDialog } from "./LicenseDialog";

describe("LicenseDialog", () => {
  it("shows Free with no key and activates a key that works", async () => {
    fakeBackend.reset();
    const onChange = vi.fn();
    render(<LicenseDialog onClose={() => {}} onChange={onChange} />);
    expect(await screen.findByRole("heading", { name: "Sheafly Free" })).toBeInTheDocument();
    const activate = screen.getByRole("button", { name: "Kích hoạt" });
    expect(activate).toBeDisabled();
    await userEvent.type(screen.getByLabelText("License key"), "SHEAFLY-ABCDE");
    await userEvent.click(activate);
    expect(await screen.findByRole("heading", { name: "Sheafly Pro" })).toBeInTheDocument();
    expect(screen.getByText("Cấp cho Lan (lan@example.com)")).toBeInTheDocument();
    expect(screen.getByText("Gồm các bản phát hành đến 24/10/2027")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Đã kích hoạt.");
    expect(screen.getByLabelText("License key")).toHaveValue("");
    expect(fakeBackend.calls.find((call) => call.cmd === "license_install")?.args).toEqual({ key: "SHEAFLY-ABCDE" });
    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ state: "active" }));
  });

  it("says why a key is refused and keeps the license there was", async () => {
    fakeBackend.reset();
    fakeBackend.license = makeProLicense();
    fakeBackend.licenseRefusal = makeLicense({ state: "inactive", problem: "expired", problem_day: "2026-11-07", plan: "trial" });
    render(<LicenseDialog onClose={() => {}} />);
    expect(await screen.findByRole("heading", { name: "Sheafly Pro" })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("License key"), "SHEAFLY-OLD");
    await userEvent.click(screen.getByRole("button", { name: "Thay license" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Bản dùng thử đã hết hạn ngày 07/11/2026.");
    expect(screen.getByRole("heading", { name: "Sheafly Pro" })).toBeInTheDocument();
    expect(screen.getByLabelText("License key")).toHaveValue("SHEAFLY-OLD");
  });

  it("explains a saved key that no longer covers this build", async () => {
    fakeBackend.reset();
    fakeBackend.license = makeProLicense({ state: "inactive", problem: "updates_ended", problem_day: "2026-01-31", features: [] });
    render(
      <I18nProvider lang="en">
        <LicenseDialog onClose={() => {}} />
      </I18nProvider>,
    );
    expect(
      await screen.findByText(/^The saved license does not work with this version: The license covers releases up to 2026-01-31; this one is newer\./),
    ).toHaveAttribute("role", "alert");
  });

  it("names no plan for a saved key that does not read", async () => {
    fakeBackend.reset();
    fakeBackend.license = makeLicense({ state: "inactive", problem: "bad_signature" });
    render(<LicenseDialog onClose={() => {}} />);
    expect(await screen.findByRole("heading", { name: "License đã lưu" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Sheafly Pro" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("alert").some((alert) => alert.textContent?.includes("không khớp chữ ký"))).toBe(true);
  });

  it("asks once more before removing the license", async () => {
    fakeBackend.reset();
    fakeBackend.license = makeProLicense();
    render(<LicenseDialog onClose={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "Gỡ license" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Gỡ license trên máy này?");
    await userEvent.click(screen.getByRole("button", { name: "Giữ lại" }));
    expect(fakeBackend.calls.some((call) => call.cmd === "license_remove")).toBe(false);
    await userEvent.click(screen.getByRole("button", { name: "Gỡ license" }));
    await userEvent.click(screen.getByRole("button", { name: "Có, gỡ license" }));
    expect(await screen.findByRole("heading", { name: "Sheafly Free" })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Gỡ license" })).not.toBeInTheDocument());
  });
});
