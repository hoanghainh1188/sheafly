import { afterEach, describe, expect, it, vi } from "vitest";

import { setCurrentLang } from "./i18n/lang";

import {
  dateKeyToInstant,
  datetimeLocalToInstant,
  formatDateKey,
  formatDateKeyDisplay,
  formatDateOnly,
  formatDuration,
  formatHours,
  formatInstant,
  formatMoney,
  formatMoneyInput,
  parseMoney,
  normalizePredecessorText,
  instantToDateKey,
  instantToDatetimeLocal,
  minuteToTime,
  parseDateOnly,
  parseDuration,
  parseInstant,
  parseTimeOfDay,
  shiftDays,
  timeToMinute,
  todayInstant,
  utcMidnight,
} from "./format";
import { at } from "./test/fixtures";

describe("formatDuration", () => {
  it.each([
    [0, "0 ngày"],
    [480, "1 ngày"],
    [2400, "5 ngày"],
    [720, "1,5 ngày"],
    [768, "1,6 ngày"],
    [1536, "3,2 ngày"],
    [3600, "7,5 ngày"],
    [60, "1 giờ"],
    [540, "9 giờ"],
    [12060, "201 giờ"],
    [90, "1,5 giờ"],
    [45, "0,75 giờ"],
    [25, "25 phút"],
    [-480, "-1 ngày"],
    [-720, "-1,5 ngày"],
    [-120, "-0,25 ngày"],
    [-25, "-25 phút"],
    [59.6, "1 giờ"],
    [0.4, "0 ngày"],
  ])("%s minutes -> %s", (minutes, text) => {
    expect(formatDuration(minutes)).toBe(text);
  });

  it("returns an empty string for non-finite input", () => {
    expect(formatDuration(Number.NaN)).toBe("");
    expect(formatDuration(Number.POSITIVE_INFINITY)).toBe("");
  });
});

describe("parseDuration", () => {
  it.each([
    ["2", 960],
    ["1.5", 720],
    ["1,5", 720],
    ["2 ngày", 960],
    ["2ngay", 960],
    ["2d", 960],
    ["3 giờ", 180],
    ["3gio", 180],
    ["3h", 180],
    ["0.25h", 15],
    ["45 phút", 45],
    ["45phut", 45],
    ["45m", 45],
    ["  4 H ", 240],
    ["-1", -480],
    ["0", 0],
  ])("%j -> %s", (text, minutes) => {
    expect(parseDuration(text)).toBe(minutes);
  });

  it.each(["", "   ", "abc", "1 tuần", "1..5", "h", "2 d d"])("rejects %j", (text) => {
    expect(parseDuration(text)).toBeNull();
  });

  it("round-trips every whole minute from -3 days to 30 days", () => {
    for (let minutes = -3 * 480; minutes <= 30 * 480; minutes += 1) {
      expect(parseDuration(formatDuration(minutes))).toBe(minutes);
    }
  });

  it("round-trips with formatDuration", () => {
    for (const minutes of [0, 15, 25, 45, 60, 90, 540, 720, 768, 1440, 4800, 12060]) {
      expect(parseDuration(formatDuration(minutes))).toBe(minutes);
    }
  });
});

describe("formatInstant / parseInstant", () => {
  it("formats with UTC fields as dd/mm/yyyy hh:mm", () => {
    expect(formatInstant(at(2026, 1, 5, 8, 0))).toBe("05/01/2026 08:00");
    expect(formatInstant(at(2026, 12, 31, 23, 59))).toBe("31/12/2026 23:59");
  });

  it("parses full and short forms; date-only defaults to 08:00", () => {
    expect(parseInstant("05/01/2026 08:30")).toBe(at(2026, 1, 5, 8, 30));
    expect(parseInstant("5/1/2026 7:05")).toBe(at(2026, 1, 5, 7, 5));
    expect(parseInstant(" 5/1/2026 ")).toBe(at(2026, 1, 5, 8, 0));
  });

  it.each(["31/02/2026", "29/02/2026 08:00", "32/01/2026", "00/01/2026", "01/13/2026", "01/01/2026 24:00", "01/01/2026 08:60", "2026-02-30", "26-01-01", "1/1/26", ""])(
    "rejects %j",
    (text) => {
      expect(parseInstant(text)).toBeNull();
    },
  );

  it("accepts a leap day", () => {
    expect(parseInstant("29/02/2028")).toBe(at(2028, 2, 29, 8, 0));
  });

  it("round-trips", () => {
    const ms = at(2026, 7, 14, 16, 45);
    expect(parseInstant(formatInstant(ms))).toBe(ms);
  });
});

describe("date-only helpers", () => {
  it("formatDateOnly drops the time", () => {
    expect(formatDateOnly(at(2026, 3, 9, 17, 0))).toBe("09/03/2026");
  });

  it("parseDateOnly maps to UTC midnight and rejects a time part", () => {
    expect(parseDateOnly("09/03/2026")).toBe(at(2026, 3, 9));
    expect(parseDateOnly(" 9/3/2026 ")).toBe(at(2026, 3, 9));
    expect(parseDateOnly("09/03/2026 08:00")).toBeNull();
    expect(parseDateOnly("31/04/2026")).toBeNull();
  });

  it("converts between date keys and instants", () => {
    expect(dateKeyToInstant("2026-03-09")).toBe(at(2026, 3, 9));
    expect(dateKeyToInstant("09/03/2026")).toBeNull();
    expect(instantToDateKey(at(2026, 3, 9, 15, 30))).toBe("2026-03-09");
  });

  it("formats and parses display date keys", () => {
    expect(formatDateKeyDisplay("2026-03-09")).toBe("09/03/2026");
    expect(formatDateKeyDisplay("bad")).toBe("bad");
    expect(formatDateKey("9/3/2026")).toBe("2026-03-09");
    expect(formatDateKey("2026-03-09")).toBe("2026-03-09");
    expect(formatDateKey("2026-3-9")).toBe("2026-03-09");
  });

  it("shiftDays keeps the time and crosses month and year boundaries", () => {
    expect(shiftDays(at(2026, 1, 30, 8, 15), 3)).toBe(at(2026, 2, 2, 8, 15));
    expect(shiftDays(at(2026, 1, 1, 8, 0), -1)).toBe(at(2025, 12, 31, 8, 0));
  });

  it("utcMidnight truncates to the day", () => {
    expect(utcMidnight(at(2026, 5, 6, 23, 59))).toBe(at(2026, 5, 6));
  });
});

describe("todayInstant", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the local wall-clock day as UTC midnight", () => {
    vi.useFakeTimers();
    // Local noon keeps the date the same in every timezone.
    vi.setSystemTime(new Date(2026, 9, 10, 12, 0));
    expect(todayInstant()).toBe(at(2026, 10, 10));
  });
});

describe("time-of-day helpers", () => {
  it.each([
    ["08:30", 510],
    ["8:30", 510],
    ["8h30", 510],
    ["0830", 510],
    ["830", 510],
    ["00:00", 0],
    ["24:00", 1440],
  ])("parseTimeOfDay(%j) = %s", (text, minute) => {
    expect(parseTimeOfDay(text)).toBe(minute);
  });

  it.each(["24:30", "25:00", "08:60", "8", "8:3", "abc", ""])("parseTimeOfDay rejects %j", (text) => {
    expect(parseTimeOfDay(text)).toBeNull();
  });

  it("minuteToTime clamps and rounds", () => {
    expect(minuteToTime(510)).toBe("08:30");
    expect(minuteToTime(1440)).toBe("24:00");
    expect(minuteToTime(2000)).toBe("24:00");
    expect(minuteToTime(-5)).toBe("00:00");
    expect(minuteToTime(59.6)).toBe("01:00");
  });

  it("timeToMinute reads HH:MM and rejects 24:00", () => {
    expect(timeToMinute("17:45")).toBe(1065);
    expect(timeToMinute("24:00")).toBeNull();
    expect(timeToMinute("7:45")).toBeNull();
  });

  it("datetime-local conversions use UTC fields", () => {
    const ms = at(2026, 2, 3, 4, 5);
    expect(instantToDatetimeLocal(ms)).toBe("2026-02-03T04:05");
    expect(datetimeLocalToInstant("2026-02-03T04:05")).toBe(ms);
    expect(datetimeLocalToInstant("2026-02-03T04:05:59")).toBe(ms);
    expect(datetimeLocalToInstant("2026-02-03")).toBeNull();
  });
});

describe("number formatting", () => {
  it("formatMoney uses Vietnamese grouping and no decimals", () => {
    expect(formatMoney(1_234_567.6)).toBe("1.234.568");
    expect(formatMoney(0)).toBe("0");
    expect(formatMoney(-2500)).toBe("-2.500");
  });

  it("formatHours hides near-zero values and keeps one decimal with a comma", () => {
    expect(formatHours(0)).toBe("");
    expect(formatHours(0.04)).toBe("");
    expect(formatHours(8)).toBe("8");
    expect(formatHours(7.25)).toBe("7,3");
    expect(formatHours(-1.5)).toBe("-1,5");
  });
});

describe("English formatting", () => {
  afterEach(() => setCurrentLang("vi"));

  it("shows ISO dates, decimal points, English units and US grouping", () => {
    setCurrentLang("en");
    expect(formatInstant(at(2026, 1, 5, 8, 0))).toBe("2026-01-05 08:00");
    expect(formatDateOnly(at(2026, 1, 5, 8, 0))).toBe("2026-01-05");
    expect(formatDateKeyDisplay("2026-03-09")).toBe("2026-03-09");
    expect(formatDuration(0)).toBe("0 days");
    expect(formatDuration(480)).toBe("1 day");
    expect(formatDuration(3600)).toBe("7.5 days");
    expect(formatDuration(60)).toBe("1 hr");
    expect(formatDuration(90)).toBe("1.5 hrs");
    expect(formatDuration(7)).toBe("7 mins");
    expect(formatMoney(3480000)).toBe("3,480,000");
    expect(formatHours(2.5)).toBe("2.5");
  });

  it("reads ISO dates only in English, so 1/5/2026 is never taken as May 1", () => {
    setCurrentLang("en");
    expect(parseInstant("2026-01-05 08:00")).toBe(at(2026, 1, 5, 8, 0));
    expect(parseInstant("05/01/2026 08:00")).toBeNull();
    expect(parseInstant("1/5/2026")).toBeNull();
    expect(formatDateKey("1/5/2026")).toBeNull();
    setCurrentLang("vi");
    expect(parseInstant("2026-01-05 08:00")).toBe(at(2026, 1, 5, 8, 0));
    expect(parseInstant("05/01/2026 08:00")).toBe(at(2026, 1, 5, 8, 0));
  });

  it("reads durations in both languages whatever the interface language", () => {
    setCurrentLang("en");
    expect(parseDuration("7.5 days")).toBe(3600);
    expect(parseDuration("2 hrs")).toBe(120);
    expect(parseDuration("30 mins")).toBe(30);
    expect(parseDuration("1,5 ngày")).toBe(720);
    expect(parseDuration("1 week")).toBeNull();
  });

  it("round-trips every duration it shows", () => {
    setCurrentLang("en");
    for (const minutes of [0, 1, 7, 60, 90, 480, 720, 3600, 2410]) {
      expect(parseDuration(formatDuration(minutes))).toBe(minutes);
    }
  });
});

describe("normalizePredecessorText", () => {
  it("turns English lag units into the short forms the engine reads", () => {
    expect(normalizePredecessorText("2SS+2 hrs")).toBe("2SS+2h");
    expect(normalizePredecessorText("3FS-30 mins; 4")).toBe("3FS-30m; 4");
    expect(normalizePredecessorText("5FF+1.5 days")).toBe("5FF+1.5d");
    expect(normalizePredecessorText("2SS+1d")).toBe("2SS+1d");
    expect(normalizePredecessorText("2SS+2giờ")).toBe("2SS+2giờ");
  });
});

describe("money in fields", () => {
  afterEach(() => setCurrentLang("vi"));

  it("shows an amount with the language's grouping and up to two decimals", () => {
    expect(formatMoneyInput(1_248_000)).toBe("1.248.000");
    expect(formatMoneyInput(65_000.5)).toBe("65.000,5");
    setCurrentLang("en");
    expect(formatMoneyInput(1_248_000)).toBe("1,248,000");
    expect(formatMoneyInput(65_000.5)).toBe("65,000.5");
  });

  it("reads an amount grouped as shown, or plain with either decimal mark", () => {
    expect(parseMoney("1.248.000")).toBe(1_248_000);
    expect(parseMoney("1.248.000,5")).toBe(1_248_000.5);
    expect(parseMoney(" 1 248 000 ")).toBe(1_248_000);
    expect(parseMoney("1248000")).toBe(1_248_000);
    expect(parseMoney("12,5")).toBe(12.5);
    expect(parseMoney("12.5")).toBe(12.5);
    setCurrentLang("en");
    expect(parseMoney("1,248,000.5")).toBe(1_248_000.5);
    expect(parseMoney("12.5")).toBe(12.5);
  });

  it("refuses what is not an amount", () => {
    for (const text of ["", "abc", "1.2.3", "-5", "1,,2"]) expect(parseMoney(text)).toBeNull();
  });
});

describe("Japanese format", () => {
  afterEach(() => setCurrentLang("vi"));

  it("formats and parses durations in Japanese", () => {
    setCurrentLang("ja");
    expect(formatDuration(480)).toBe("1 日");
    expect(formatDuration(720)).toBe("1.5 日");
    expect(formatDuration(60)).toBe("1 時間");
    expect(formatDuration(30)).toBe("0.5 時間");
    expect(formatDuration(25)).toBe("25 分");

    expect(parseDuration("1日")).toBe(480);
    expect(parseDuration("1.5 日")).toBe(720);
    expect(parseDuration("2時間")).toBe(120);
    expect(parseDuration("25分")).toBe(25);

    for (const minutes of [0, 1, 7, 25, 60, 90, 480, 720, 3600]) {
      expect(parseDuration(formatDuration(minutes))).toBe(minutes);
    }
  });

  it("formats dates and money in Japanese", () => {
    setCurrentLang("ja");
    expect(formatDateOnly(Date.UTC(2026, 0, 5))).toBe("2026-01-05");
    expect(formatMoney(1_248_000)).toBe("1,248,000");
    expect(formatHours(2.5)).toBe("2.5");
  });
});

