import { describe, expect, it } from "vitest";

import { makeProject, makeResource, makeTask } from "../../test/fixtures";
import { adviceNames, adviceParts, refUid } from "./adviceText";

const names = adviceNames(
  makeProject({
    tasks: [makeTask({ uid: 2, name: "Lập trình" }), makeTask({ uid: 42, name: "Bàn giao" })],
    resources: [makeResource({ uid: 7, name: "An" })],
  }),
);

describe("adviceParts", () => {
  it("shows tasks and people by name where the AI wrote their IDs", () => {
    expect(adviceParts("T42 trễ vì R7 làm T2 trước.", names)).toEqual([
      { kind: "task", uid: 42, name: "Bàn giao" },
      { kind: "text", text: " trễ vì " },
      { kind: "resource", uid: 7, name: "An" },
      { kind: "text", text: " làm " },
      { kind: "task", uid: 2, name: "Lập trình" },
      { kind: "text", text: " trước." },
    ]);
  });

  it("leaves IDs the project lacks and words that only start like one as written", () => {
    expect(adviceParts("T99 and T2x or AT2, R70.", names)).toEqual([{ kind: "text", text: "T99 and T2x or AT2, R70." }]);
    expect(adviceParts("", names)).toEqual([]);
  });

  it("reads an ID of the expected kind only", () => {
    expect([refUid("T42", "T"), refUid("R7", "R"), refUid("R7", "T"), refUid("T4 2", "T"), refUid("t4", "T")]).toEqual([
      42,
      7,
      null,
      null,
      null,
    ]);
  });
});
