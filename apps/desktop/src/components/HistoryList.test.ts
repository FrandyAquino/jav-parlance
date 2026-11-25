import { describe, expect, it } from "vitest";
import { toPlainText } from "./HistoryList";

describe("toPlainText", () => {
  it("formats timestamps and indents translations under each line", () => {
    const text = toPlainText([
      { startMs: 61_000, language: "en", text: "Hello", translation: "Hola" },
      { startMs: 65_000, language: "en", text: "Bye", translation: null },
    ]);
    expect(text).toBe("[01:01] Hello\n        Hola\n[01:05] Bye");
  });
});
