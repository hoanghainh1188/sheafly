import { defineMessages } from "./i18n";

const WEEKDAYS_VI = ["CN", "T2", "T3", "T4", "T5", "T6", "T7"];
const WEEKDAYS_JA = ["日", "月", "火", "水", "木", "金", "土"];
const WEEKDAYS_EN = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS_EN = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

/** Timescale labels. Months are 1–12; weekdays follow `getUTCDay()` (0 is Sunday). */
export const timescaleMessages = defineMessages(
  {
    monthBand: (month: number, year: number) => `Tháng ${month} ${year}`,
    weekBand: (week: number) => `Tuần ${week}`,
    monthShort: (month: number, year: number) => `T${month}/${year}`,
    weekday: (day: number) => WEEKDAYS_VI[day] ?? "",
  },
  {
    monthBand: (month: number, year: number) => `${MONTHS_EN[month - 1]} ${year}`,
    weekBand: (week: number) => `Week ${week}`,
    monthShort: (month: number, year: number) => `${MONTHS_EN[month - 1].slice(0, 3)} ${year}`,
    weekday: (day: number) => WEEKDAYS_EN[day] ?? "",
  },
  {
    monthBand: (month: number, year: number) => `${year}年${month}月`,
    weekBand: (week: number) => `第 ${week} 週`,
    monthShort: (month: number, year: number) => `${year}/${month}`,
    weekday: (day: number) => WEEKDAYS_JA[day] ?? "",
  },
);
