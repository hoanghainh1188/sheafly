import { defineMessages } from "../i18n";

export const taskUsageMessages = defineMessages(
  {
    head: "Công việc / nguồn lực",
    /** A day column from a calendar key ("2026-01-05"): "05/01". */
    day: (key: string) => `${key.slice(8)}/${key.slice(5, 7)}`,
    empty: "Chưa có công việc.",
  },
  {
    head: "Task / resource",
    /** Month first, like the ISO dates English shows elsewhere: "01-05". */
    day: (key: string) => `${key.slice(5, 7)}-${key.slice(8)}`,
    empty: "No tasks yet.",
  },
  {
    head: "タスク / リソース",
    day: (key: string) => `${parseInt(key.slice(5, 7), 10)}/${parseInt(key.slice(8), 10)}`,
    empty: "タスクがありません。",
  },
);
