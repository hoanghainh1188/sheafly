import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { fakeBackend } from "../../test/fakeBackend";
import { makeHealthCriterion, makeHealthReport, makeProject } from "../../test/fixtures";
import { useScheduleHealth } from "./useScheduleHealth";

describe("useScheduleHealth", () => {
  it("asks nothing until wanted", () => {
    fakeBackend.reset();
    renderHook(() => useScheduleHealth(makeProject(), false));
    expect(fakeBackend.calls.some((call) => call.cmd === "schedule_health")).toBe(false);
  });

  it("asks again for each project and knows when the report is older", async () => {
    fakeBackend.reset();
    const first = makeProject({ name: "A" });
    const { result, rerender } = renderHook(({ project }) => useScheduleHealth(project, true), { initialProps: { project: first } });
    await waitFor(() => expect(result.current.current).toBe(true));
    const release = fakeBackend.holdNext("schedule_health");
    fakeBackend.health = makeHealthReport([makeHealthCriterion(2, { status: "fail" })]);
    rerender({ project: makeProject({ name: "B" }) });
    expect(result.current.current).toBe(false);
    release();
    await waitFor(() => expect(result.current.report?.criteria[1].status).toBe("fail"));
    expect(result.current.current).toBe(true);
  });

  it("reports an error and tries again when wanted again", async () => {
    fakeBackend.reset();
    fakeBackend.failNext("schedule_health", "Cần Sheafly Pro");
    const project = makeProject();
    const { result, rerender } = renderHook(({ wanted }) => useScheduleHealth(project, wanted), { initialProps: { wanted: true } });
    await waitFor(() => expect(result.current.error).toBe("Cần Sheafly Pro"));
    rerender({ wanted: false });
    rerender({ wanted: true });
    await waitFor(() => expect(result.current.report).not.toBeNull());
    expect(result.current.error).toBeNull();
  });
});
