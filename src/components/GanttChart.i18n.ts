import { defineMessages } from "../i18n";

export const ganttChartMessages = defineMessages(
  {
    chart:
      "Biểu đồ Gantt. Mũi tên lên, xuống chọn công việc; Alt và mũi tên trái, phải dời một ngày làm việc; thêm Shift để dời ngày kết thúc; Enter để sửa.",
    levelingGap: "Tách do cân bằng (chen ngang)",
  },
  {
    chart:
      "Gantt chart. Up and Down arrows select a task; Alt with Left or Right arrow moves it one working day; add Shift to move the finish date; Enter to edit.",
    levelingGap: "Split by leveling (interrupted)",
  },
  {
    chart:
      "ガントチャート。上下矢印キーでタスクを選択、Alt+左右矢印キーで1営業日移動、Shift追加で終了日を変更、Enterで編集。",
    levelingGap: "平準化による分割 (割り込み)",
  },
);
