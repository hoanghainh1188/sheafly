import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../../i18n";
import { fakeBackend } from "../../test/fakeBackend";
import { makeProject, makeResource, MONDAY_8AM } from "../../test/fixtures";
import { LIGHT_CANVAS } from "../../theme";
import { LoadMapView } from "./LoadMapView";

const project = makeProject({
  start: MONDAY_8AM,
  resources: [makeResource({ uid: 10, name: "An", group: "Backend" }), makeResource({ uid: 11, name: "Bình", group: "Frontend" })],
  skills: [{ id: 1, name: "Rust" }],
});

function recordingContext() {
  const fills: string[] = [];
  let fillStyle = "";
  const target = {
    get fillStyle() {
      return fillStyle;
    },
    set fillStyle(value: string) {
      fillStyle = value;
      fills.push(value);
    },
  };
  const ctx = new Proxy(target, {
    get: (object, key) => (key in object ? object[key as keyof typeof object] : () => {}),
    set: (object, key, value) => {
      (object as Record<string | symbol, unknown>)[key] = value;
      return true;
    },
  });
  return { ctx, fills };
}

function renderView(onPickResource = vi.fn(), lang: "vi" | "en" = "vi", proj = project) {
  fakeBackend.loadMap = {
    buckets: ["2026-01-05", "2026-01-12"],
    rows: [
      { resource_uid: 10, label: "An", cells: [{ work: 2400, capacity: 2400 }, { work: 3000, capacity: 2400 }] },
      { resource_uid: 11, label: "Bình", cells: [{ work: 600, capacity: 2400 }, { work: 0, capacity: 0 }] },
    ],
  };
  const recorded = recordingContext();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(recorded.ctx as never);
  render(
    <I18nProvider lang={lang}>
      <LoadMapView project={proj} onPickResource={onPickResource} />
    </I18nProvider>,
  );
  return { onPickResource, recorded };
}

function lastLoadMap() {
  const calls = fakeBackend.calls.filter((call) => call.cmd === "load_map");
  return calls[calls.length - 1]?.args as {
    from: string;
    to: string;
    bucket: string;
    group?: { type: string; id?: number; min_level?: number };
    baseline?: number;
  };
}

describe("LoadMapView", () => {
  it("asks for twelve weeks by default and draws the cells", async () => {
    const { recorded } = renderView();
    await waitFor(() => expect(lastLoadMap()?.bucket).toBe("week"));
    expect(lastLoadMap()?.from).toBe("2026-01-05");
    expect(lastLoadMap()?.to).toBe("2026-03-29");
    await waitFor(() => expect(recorded.fills.length).toBeGreaterThan(0));
    expect(recorded.fills).toContain(LIGHT_CANVAS.loadFull);
    expect(recorded.fills).toContain(LIGHT_CANVAS.loadOver);
    expect(recorded.fills).toContain(LIGHT_CANVAS.loadLight);
  });

  it("marks the Pro modes and leaves the person, day and week load free", () => {
    renderView();
    for (const name of ["Vai trò Pro", "Kỹ năng Pro", "Tháng Pro", "Biểu đồ cột Pro"]) {
      expect(screen.getByRole("radio", { name: new RegExp(`^${name}$`, "i") })).toBeInTheDocument();
    }
    for (const name of ["Người", "Ngày", "Tuần"]) {
      expect(screen.getByRole("radio", { name: new RegExp(`^${name}$`, "i") })).toBeInTheDocument();
    }
    expect(screen.getByRole("combobox", { name: /Pro/ })).toBeInTheDocument();
  });

  it("can switch period to days, weeks, or months", async () => {
    renderView();
    // Switch to days
    fireEvent.click(screen.getByRole("radio", { name: /ngày|days/i }));
    await waitFor(() => expect(lastLoadMap()?.bucket).toBe("day"));

    // Switch to months
    fireEvent.click(screen.getByRole("radio", { name: /tháng|months/i }));
    await waitFor(() => expect(lastLoadMap()?.bucket).toBe("month"));
  });

  it("can switch group mode to role or skill", async () => {
    renderView();
    // Switch to role
    fireEvent.click(screen.getByRole("radio", { name: /vai trò|role/i }));
    await waitFor(() => expect(lastLoadMap()?.group?.type).toBe("group"));

    // Switch to skill
    fireEvent.click(screen.getByRole("radio", { name: /kỹ năng|skill/i }));
    await waitFor(() => {
      expect(lastLoadMap()?.group?.type).toBe("skill");
      expect(lastLoadMap()?.group?.id).toBe(1);
    });
  });

  it("can toggle between heatmap grid and histogram mode", async () => {
    renderView();
    await waitFor(() => expect(screen.getByRole("img")).toBeInTheDocument());

    // Switch to histogram mode
    fireEvent.click(screen.getByRole("radio", { name: /biểu đồ cột|histogram/i }));
    await waitFor(() => {
      expect(document.querySelector(".load-histogram-svg")).toBeInTheDocument();
    });
  });

  it("sends baseline number when selected", async () => {
    renderView();
    const baselineSelect = screen.getByLabelText(/baseline/i);
    fireEvent.change(baselineSelect, { target: { value: "0" } });
    await waitFor(() => expect(lastLoadMap()?.baseline).toBe(0));
  });

  it("picks a resource when clicking their name", async () => {
    const { onPickResource } = renderView();
    await waitFor(() => expect(screen.getByRole("button", { name: /An/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: /An/ }));
    expect(onPickResource).toHaveBeenCalledWith(10);
  });
  it("asks for whole calendar months, six from the last day of a month", async () => {
    renderView(vi.fn(), "vi", { ...project, start: Date.UTC(2026, 7, 31, 8) });
    fireEvent.click(screen.getByRole("radio", { name: /tháng/i }));
    const count = screen.getByRole("spinbutton");
    fireEvent.change(count, { target: { value: "6" } });
    await waitFor(() => expect(lastLoadMap()).toMatchObject({ bucket: "month", from: "2026-08-31", to: "2027-01-31" }));
  });

  it("names the baselines and the rows' kind through the catalog", async () => {
    renderView(vi.fn(), "en");
    expect(screen.getByRole("option", { name: "Baseline 3" })).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector(".load-corner")?.textContent).toBe("Person"));
    fireEvent.click(screen.getByRole("radio", { name: /^role/i }));
    await waitFor(() => expect(document.querySelector(".load-corner")?.textContent).toBe("Role"));
  });

  it("shows hours with the language's unit in the histogram, zero as 0", async () => {
    renderView(vi.fn(), "en");
    fireEvent.click(screen.getByRole("radio", { name: /histogram/i }));
    fireEvent.change(await screen.findByRole("combobox", { name: /Person/ }), { target: { value: "1" } });
    const columns = await waitFor(() => {
      const found = document.querySelectorAll(".load-histogram-svg > g[style]");
      expect(found.length).toBe(2);
      return found;
    });
    fireEvent.mouseEnter(columns[1]);
    const tip = document.querySelector(".load-histogram-tooltip")!;
    expect(tip.textContent).toContain("Assigned: 0 hrs");
    expect(tip.textContent).toContain("Capacity: 0 hrs");
    expect(tip.textContent).toContain("Load:");
    expect(tip.textContent).not.toMatch(/Tải|\dh\b/);
    expect(screen.getAllByText("0h").length).toBeGreaterThan(0);
  });

  it("colours the histogram through styles, never SVG colour attributes", async () => {
    renderView();
    fireEvent.click(screen.getByRole("radio", { name: /biểu đồ cột/i }));
    await waitFor(() => expect(document.querySelector(".load-histogram-svg")).toBeInTheDocument());
    const svg = document.querySelector(".load-histogram-svg")!;
    expect(svg.querySelectorAll("[fill], [stroke]")).toHaveLength(0);
    expect(svg.querySelector("line")?.getAttribute("style")).toContain("var(--chart-grid)");
  });
});
