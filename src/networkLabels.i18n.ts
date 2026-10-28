import { defineMessages } from "./i18n";

/** Network box text, shared by the on-screen diagram and its PDF export. */
export const networkBoxMessages = defineMessages(
  {
    recurring: (count: number) => `Lặp · ${count} lần`,
    summary: "Tóm tắt",
    milestone: "Mốc",
    start: "Bắt đầu",
    finish: "Kết thúc",
    complete: "Hoàn thành",
    taskFallback: (id: number) => `Công việc ${id}`,
    criticalSuffix: " · Găng",
  },
  {
    recurring: (count: number) => `Recurring · ${count} ${count === 1 ? "occurrence" : "occurrences"}`,
    summary: "Summary",
    milestone: "Milestone",
    start: "Start",
    finish: "Finish",
    complete: "Complete",
    taskFallback: (id: number) => `Task ${id}`,
    criticalSuffix: " · Critical",
  },
  {
    recurring: (count: number) => `定期 · ${count} 回`,
    summary: "サマリー",
    milestone: "マイルストーン",
    start: "開始日",
    finish: "終了日",
    complete: "達成率",
    taskFallback: (id: number) => `タスク ${id}`,
    criticalSuffix: " · クリティカル",
  },
);
