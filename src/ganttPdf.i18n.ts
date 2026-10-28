import { defineMessages } from "./i18n";

/** Text printed in the PDF exports (Gantt and network diagram). */
export const pdfMessages = defineMessages(
  {
    untitled: "Dự án",
    name: "Tên",
    start: "Bắt đầu",
    finish: "Kết thúc",
    page: (page: number, count: number) => `Trang ${page}/${count}`,
    networkTitle: (project: string) => `${project} — Sơ đồ mạng`,
  },
  {
    untitled: "Project",
    name: "Name",
    start: "Start",
    finish: "Finish",
    page: (page: number, count: number) => `Page ${page}/${count}`,
    networkTitle: (project: string) => `${project} — Network diagram`,
  },
  {
    untitled: "プロジェクト",
    name: "名前",
    start: "開始日",
    finish: "終了日",
    page: (page: number, count: number) => `${page} / ${count} ページ`,
    networkTitle: (project: string) => `${project} — ネットワーク図`,
  },
);
