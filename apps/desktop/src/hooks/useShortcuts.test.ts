import { describe, expect, it } from "vitest";
import { shortcutKey, shortcutLabel } from "./useShortcuts";

describe("shortcut labels and keys", () => {
  it("labels the Mac Command modifier and the Windows/Linux Control modifier", () => {
    expect(shortcutLabel("Enter", "macos")).toBe("⌘+Enter");
    expect(shortcutLabel("P", "windows")).toBe("Ctrl+P");
    expect(shortcutLabel(",", "linux")).toBe("Ctrl+,");
  });

  it("preserves Command and Control keyboard shortcuts", () => {
    expect(shortcutKey({ key: "Enter", metaKey: true, ctrlKey: false, shiftKey: false })).toBe("mod+enter");
    expect(shortcutKey({ key: "P", metaKey: false, ctrlKey: true, shiftKey: false })).toBe("mod+p");
  });
});
