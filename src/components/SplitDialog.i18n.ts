import { defineMessages } from "../i18n";

export const splitDialogMessages = defineMessages(
  {
    title: "Tách công việc",
    cancel: "Hủy",
    split: "Tách",
    /** The task's line under the title; dates and duration come already formatted. */
    summary: (name: string, start: string, finish: string, duration: string) =>
      `${name} · ${start} – ${finish} · thời lượng ${duration}`,
    splitAt: "Tách tại",
    splitAtLabel: "Thời điểm tách",
    gap: "Khoảng ngắt (giờ làm việc)",
    gapLabel: "Khoảng ngắt",
    explanation:
      "Thời lượng giữ nguyên. Phần việc sau điểm tách dời muộn hơn đúng bằng khoảng ngắt, nên ngày kết thúc cũng lùi theo.",
    badGap: "Khoảng ngắt phải lớn hơn 0, ví dụ 1 ngày, 4 giờ, 30 phút",
    badPoint: "Điểm tách phải nằm giữa ngày bắt đầu và ngày kết thúc của công việc",
  },
  {
    title: "Split task",
    cancel: "Cancel",
    split: "Split",
    summary: (name: string, start: string, finish: string, duration: string) =>
      `${name} · ${start} – ${finish} · duration ${duration}`,
    splitAt: "Split at",
    splitAtLabel: "Split point",
    gap: "Gap (working time)",
    gapLabel: "Gap",
    explanation:
      "The duration stays the same. Work after the split point moves later by the length of the gap, so the finish date moves too.",
    badGap: "The gap must be greater than 0, for example 1 day, 4 hrs, 30 mins",
    badPoint: "The split point must fall between the task's start and finish",
  },
  {
    title: "タスクの分割",
    cancel: "キャンセル",
    split: "分割",
    summary: (name: string, start: string, finish: string, duration: string) =>
      `${name} · ${start} – ${finish} · 期間 ${duration}`,
    splitAt: "分割位置",
    splitAtLabel: "分割時点",
    gap: "休止期間 (稼働時間)",
    gapLabel: "休止期間",
    explanation:
      "期間は変更されません。分割時点以降の作業が休止期間分後ろにずれ、終了日も連動して遅くなります。",
    badGap: "休止期間は 0 より大きい値を指定してください (例: 1日、4時間、30分)",
    badPoint: "分割位置はタスクの開始日と終了日の間に設定してください",
  },
);
