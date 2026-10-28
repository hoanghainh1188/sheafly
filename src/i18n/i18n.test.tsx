import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { formatDuration } from "../format";
import { defineMessages, detectLang, getLang, I18nProvider, pick, setCurrentLang, useMessages } from ".";

const sample = defineMessages(
  { hello: "Xin chào", count: (n: number) => `${n} việc` },
  { hello: "Hello", count: (n: number) => `${n} ${n === 1 ? "task" : "tasks"}` },
);

// Type checks only: English must have exactly the Vietnamese keys and signatures.
// @ts-expect-error missing key
defineMessages({ a: "A", b: "B" }, { a: "A" });
// @ts-expect-error extra key
defineMessages({ a: "A" }, { a: "A", b: "B" });
// @ts-expect-error different parameters
defineMessages({ f: (n: number) => `${n}` }, { f: (s: string) => s });

function Greeting() {
  const m = useMessages(sample);
  return (
    <p>
      {m.hello} · {m.count(1)} · {formatDuration(480)}
    </p>
  );
}

describe("i18n", () => {
  afterEach(() => setCurrentLang("vi"));

  it("detects Vietnamese or Japanese when the system prefers it", () => {
    expect(detectLang(["vi-VN", "en-US"])).toBe("vi");
    expect(detectLang(["vi"])).toBe("vi");
    expect(detectLang(["en-US", "vi-VN"])).toBe("en");
    expect(detectLang(["ja-JP"])).toBe("ja");
    expect(detectLang(["fr-FR"])).toBe("en");
    expect(detectLang([])).toBe("en");
  });

  it("picks the current language outside React", () => {
    expect(pick(sample).hello).toBe("Xin chào");
    setCurrentLang("en");
    expect(pick(sample).count(2)).toBe("2 tasks");
  });

  it("renders messages and formats in the provider's language", () => {
    const { rerender } = render(
      <I18nProvider lang="vi">
        <Greeting />
      </I18nProvider>,
    );
    expect(screen.getByText("Xin chào · 1 việc · 1 ngày")).toBeInTheDocument();
    rerender(
      <I18nProvider lang="en">
        <Greeting />
      </I18nProvider>,
    );
    expect(screen.getByText("Hello · 1 task · 1 day")).toBeInTheDocument();
    expect(getLang()).toBe("en");
  });
});
