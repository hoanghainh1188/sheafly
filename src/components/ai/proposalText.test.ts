import { afterEach, describe, expect, it } from "vitest";

import { setCurrentLang } from "../../i18n";
import { makeAssignment, makeProject, makeResource, makeTask } from "../../test/fixtures";
import { describeCommand, describeGoal } from "./proposalText";

const project = makeProject({
  tasks: [makeTask({ uid: 1, name: "Thiết kế" }), makeTask({ uid: 2, name: "Lập trình" })],
  resources: [makeResource({ uid: 7, name: "An" })],
  assignments: [makeAssignment({ uid: 5, task_uid: 2, resource_uid: 7 })],
  dependencies: [{ uid: 9, predecessor_uid: 1, successor_uid: 2, kind: "fs", lag: 0 }],
});

afterEach(() => setCurrentLang("vi"));

describe("proposal text", () => {
  it("names tasks and people, and assignments and links by their ends", () => {
    expect(describeCommand({ type: "assign_resource", task_uid: 1, resource_uid: 7, units: 0.5 }, project)).toBe("Giao An vào Thiết kế (0,5)");
    expect(describeCommand({ type: "remove_assignment", uid: 5 }, project)).toBe("Bỏ An khỏi Lập trình");
    expect(describeCommand({ type: "remove_dependency", uid: 9 }, project)).toBe("Bỏ liên kết Thiết kế → Lập trình");
    expect(describeCommand({ type: "add_dependency", predecessor_uid: 1, successor_uid: 2, kind: "ss", lag: 0 }, project)).toBe("Lập trình chờ Thiết kế (SS)");
    expect(describeCommand({ type: "apply_leveling", delays: [{ uid: 2, minutes: 480 }, { uid: 1, minutes: 0 }], splits: [] }, project)).toBe(
      "Cân bằng lại nguồn lực (dời 1 việc)",
    );
    expect(describeCommand({ type: "set_priority", uid: 42, priority: 700 }, project)).toBe("Đặt độ ưu tiên của T42 là 700");
    expect(describeGoal({ kind: "deadline", task: 2 }, project)).toBe("Kịp deadline: Lập trình");
  });

  it("speaks English", () => {
    setCurrentLang("en");
    expect(describeCommand({ type: "remove_assignment", uid: 5 }, project)).toBe("Take An off Lập trình");
    expect(describeGoal({ kind: "finish" }, project)).toBe("Finish the project earlier");
  });
});
