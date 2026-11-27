import { describe, expect, it, vi } from "vitest";
import { fireEvent, renderHook } from "@testing-library/react";
import { shortcutKey, shortcutLabel, useShortcuts } from "./useShortcuts";

describe("shortcut labels and keys", () => {
  it("labels the Mac Command modifier and the Windows/Linux Control modifier", () => {
    expect(shortcutLabel("Enter", "macos")).toBe("⌘+Enter");
    expect(shortcutLabel("P", "windows")).toBe("Ctrl+P");
    expect(shortcutLabel(",", "linux")).toBe("Ctrl+,");
  });

  it("preserves Command and Control keyboard shortcuts", () => {
    expect(
      shortcutKey({
        key: "Enter",
        metaKey: true,
        ctrlKey: false,
        shiftKey: false,
      }),
    ).toBe("mod+enter");
    expect(shortcutKey({ key: "P", metaKey: false, ctrlKey: true, shiftKey: false })).toBe("mod+p");
  });
});

it("never starts or pauses capture from an editable field", () => {
  const capture = vi.fn();
  const pause = vi.fn();
  const { unmount } = renderHook(() => useShortcuts({ "mod+enter": capture, "mod+p": pause }));
  const input = document.createElement("textarea");
  document.body.append(input);
  fireEvent.keyDown(input, { key: "Enter", metaKey: true });
  fireEvent.keyDown(input, { key: "p", ctrlKey: true });
  expect(capture).not.toHaveBeenCalled();
  expect(pause).not.toHaveBeenCalled();
  fireEvent.keyDown(document.body, { key: "Enter", metaKey: true });
  expect(capture).toHaveBeenCalledTimes(1);
  input.remove();
  unmount();
});

it("keeps an open dialog's draft from being unmounted by app shortcuts", () => {
  const toggle = vi.fn();
  const { unmount } = renderHook(() => useShortcuts({ "mod+j": toggle }));
  const dialog = document.createElement("div");
  dialog.setAttribute("role", "dialog");
  const input = document.createElement("textarea");
  dialog.append(input);
  document.body.append(dialog);
  fireEvent.keyDown(input, { key: "j", metaKey: true });
  expect(toggle).not.toHaveBeenCalled();
  dialog.remove();
  unmount();
});
