import { defineMessages } from "../i18n";

const VI_WEEKDAYS = ["T2", "T3", "T4", "T5", "T6", "T7", "CN"];
const JA_WEEKDAYS = ["月", "火", "水", "木", "金", "土", "日"];
const EN_WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const EN_MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export const dateTimeFieldMessages = defineMessages(
  {
    /** Column head of the month grid; `index` 0 is Monday. */
    weekday: (index: number) => VI_WEEKDAYS[index] ?? "",
    /** Month heading; `month` is 1–12. */
    monthTitle: (month: number, year: number) => `Tháng ${month}, ${year}`,
    pickTime: "Chọn giờ",
    openDatePicker: "Mở lịch chọn ngày",
    pickDate: "Chọn ngày",
    clearDate: "Xóa ngày",
    clear: "Xóa",
    previousMonth: "Tháng trước",
    nextMonth: "Tháng sau",
    today: "Hôm nay",
    cancel: "Hủy",
    choose: "Chọn",
    hour: "Giờ",
    minute: "Phút",
    typeExactTime: "Gõ giờ chính xác vào ô nếu cần",
    datePlaceholder: "dd/mm/yyyy",
  },
  {
    weekday: (index: number) => EN_WEEKDAYS[index] ?? "",
    monthTitle: (month: number, year: number) => `${EN_MONTHS[month - 1] ?? month} ${year}`,
    pickTime: "Choose time",
    openDatePicker: "Open date picker",
    pickDate: "Choose date",
    clearDate: "Clear date",
    clear: "Clear",
    previousMonth: "Previous month",
    nextMonth: "Next month",
    today: "Today",
    cancel: "Cancel",
    choose: "OK",
    hour: "Hour",
    minute: "Minute",
    typeExactTime: "Type an exact time in the box if needed",
    datePlaceholder: "yyyy-mm-dd",
  },
  {
    weekday: (index: number) => JA_WEEKDAYS[index] ?? "",
    monthTitle: (month: number, year: number) => `${year}年${month}月`,
    pickTime: "時刻を選択",
    openDatePicker: "日付ピッカーを開く",
    pickDate: "日付を選択",
    clearDate: "日付をクリア",
    clear: "クリア",
    previousMonth: "前月",
    nextMonth: "翌月",
    today: "今日",
    cancel: "キャンセル",
    choose: "決定",
    hour: "時",
    minute: "分",
    typeExactTime: "必要に応じて入力欄に直接時刻を入力してください",
    datePlaceholder: "yyyy-mm-dd",
  },
);

/** Monday-first weekday indexes for the grid header. */
export const WEEKDAY_INDEXES = [0, 1, 2, 3, 4, 5, 6] as const;
