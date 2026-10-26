import { afterEach, describe, expect, it } from "vitest";

import { setCurrentLang } from "../../i18n";
import { makeProject, makeTask } from "../../test/fixtures";
import type { Payment } from "../../types";
import { anchorTasks, defaultAnchor, followsTask, formatPercent, parseLagDays, parsePercent, percentTotal } from "./paymentText";

function payment(id: number, amount: Payment["amount"]): Payment {
  return { id, name: `P${id}`, amount, anchor: { type: "date", at: 0 }, lag_days: 0, retained: false, received: [] };
}

describe("paymentText", () => {
  afterEach(() => setCurrentLang("vi"));

  it("reads percents with either decimal mark and a sign, up to the limit", () => {
    expect(parsePercent("30")).toBe(30);
    expect(parsePercent("12,5 %")).toBe(12.5);
    expect(parsePercent("12.5%")).toBe(12.5);
    expect(parsePercent("100")).toBe(100);
    expect(parsePercent("101")).toBeNull();
    expect(parsePercent("6", 5)).toBeNull();
    expect(parsePercent("-5")).toBeNull();
    expect(parsePercent("ba mươi")).toBeNull();
  });

  it("shows percents with the language's decimal mark", () => {
    expect(formatPercent(12.5)).toBe("12,5%");
    setCurrentLang("en");
    expect(formatPercent(12.5)).toBe("12.5%");
    expect(formatPercent(30)).toBe("30%");
  });

  it("reads Net as whole days from 0 to 365", () => {
    expect(parseLagDays(" 30 ")).toBe(30);
    expect(parseLagDays("0")).toBe(0);
    expect(parseLagDays("365")).toBe(365);
    expect(parseLagDays("366")).toBeNull();
    expect(parseLagDays("1,5")).toBeNull();
  });

  it("puts milestones and groups first, each in outline order", () => {
    const project = makeProject({
      tasks: [
        makeTask({ uid: 1, summary: true }),
        makeTask({ uid: 2 }),
        makeTask({ uid: 3, milestone: true }),
        makeTask({ uid: 4 }),
      ],
    });
    expect(anchorTasks(project).map((task) => task.uid)).toEqual([1, 3, 2, 4]);
  });

  it("anchors a new payment at the last milestone, else a group, else the project start", () => {
    const tasks = [makeTask({ uid: 1, summary: true }), makeTask({ uid: 2, milestone: true }), makeTask({ uid: 3, milestone: true })];
    expect(defaultAnchor(makeProject({ tasks }))).toEqual({ type: "finish", task: 3 });
    expect(defaultAnchor(makeProject({ tasks: tasks.slice(0, 1) }))).toEqual({ type: "finish", task: 1 });
    const empty = makeProject({ tasks: [] });
    expect(defaultAnchor(empty)).toEqual({ type: "date", at: empty.start });
  });

  it("adds up the percent payments only", () => {
    expect(percentTotal([payment(1, { fixed: 100 })])).toBeNull();
    expect(percentTotal([payment(1, { percent: 30 }), payment(2, { fixed: 5 }), payment(3, { percent: 60.1 })])).toBe(90.1);
  });

  it("knows which anchors follow a task", () => {
    expect(followsTask({ type: "finish", task: 4 }, 4)).toBe(true);
    expect(followsTask({ type: "start", task: 4 }, 5)).toBe(false);
    expect(followsTask({ type: "date", at: 0 }, 4)).toBe(false);
  });
});
