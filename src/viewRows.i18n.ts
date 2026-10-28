import { defineMessages } from "./i18n";

/** Group headers that the view builds itself (resource and summary names come from the project). */
export const viewRowsMessages = defineMessages(
  {
    resourceFallback: (uid: number) => `NL ${uid}`,
    unassigned: "Chưa gán",
    noSummary: "Không có summary",
    taskFallback: (uid: number) => `Task ${uid}`,
    noValue: (field: string) => `Chưa có ${field}`,
  },
  {
    resourceFallback: (uid: number) => `Resource ${uid}`,
    unassigned: "Unassigned",
    noSummary: "No summary task",
    taskFallback: (uid: number) => `Task ${uid}`,
    noValue: (field: string) => `No ${field}`,
  },
  {
    resourceFallback: (uid: number) => `リソース ${uid}`,
    unassigned: "未割り当て",
    noSummary: "サマリータスクなし",
    taskFallback: (uid: number) => `タスク ${uid}`,
    noValue: (field: string) => `${field} なし`,
  },
);
