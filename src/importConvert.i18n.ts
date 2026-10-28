import { defineMessages } from "./i18n";

/** Why a cell of an imported table cannot be read. */
export const importConvertMessages = defineMessages(
  {
    needsName: "Chọn cột Tên",
    level: "Cấp là số nguyên từ 1",
    work: "Effort không đọc được (ví dụ 16, 2 ngày, 3h)",
    duration: "Thời lượng không đọc được (ví dụ 2, 1,5 ngày, 4h)",
    priority: "Ưu tiên là số nguyên từ 0 đến 1.000",
    date: "Ngày không đọc được (ví dụ 09/01/2026 hoặc 2026-01-09 13:00)",
    units: "Đơn vị không đọc được (ví dụ 1, 0,5, 50%, 4h/ngày)",
    noLine: (line: string) => `Không có dòng ${line} trong bảng`,
    field: (name: string) => `Giá trị không hợp với trường ${name}`,
    kind: "Loại là Người hoặc Máy",
    rate: "Đơn giá là số không âm",
  },
  {
    needsName: "Choose the Name column",
    level: "The level is a whole number from 1",
    work: "Cannot read the effort (for example 16, 2 days, 3h)",
    duration: "Cannot read the duration (for example 2, 1.5 days, 4h)",
    priority: "The priority is a whole number from 0 to 1,000",
    date: "Cannot read the date (for example 2026-01-09 or 2026-01-09 13:00)",
    units: "Cannot read the units (for example 1, 0.5, 50%, 4 hrs/day)",
    noLine: (line: string) => `The table has no line ${line}`,
    field: (name: string) => `The value does not fit the field ${name}`,
    kind: "The kind is Person or Machine",
    rate: "The rate is a number of at least 0",
  },
  {
    needsName: "タスク名列を選択してください",
    level: "レベルは 1 以上の整数です",
    work: "工数を読み取れません (例: 16、2 日、3h)",
    duration: "期間を読み取れません (例: 2、1.5 日、4h)",
    priority: "優先度は 0〜1,000 の整数です",
    date: "日付を読み取れません (例: 2026-01-09 または 2026-01-09 13:00)",
    units: "単位数を読み取れません (例: 1、0.5、50%、4h/日)",
    noLine: (line: string) => `テーブルに ${line} 行目がありません`,
    field: (name: string) => `値がフィールド “${name}” に適合しません`,
    kind: "種類は作業 (Person) または設備 (Machine) です",
    rate: "単価は 0 以上の数値です",
  },
);
