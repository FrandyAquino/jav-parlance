import { useEffect } from "react";

export type ShortcutMap = Record<string, () => void>;

export function shortcutLabel(key: string, platform: string | undefined): string {
  return `${platform === "macos" ? "⌘" : "Ctrl"}+${key}`;
}

/** Normalizes a keyboard event to e.g. `mod+enter`, `mod+,`. */
export function shortcutKey(event: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey">): string {
  const parts = [];
  if (event.ctrlKey || event.metaKey) parts.push("mod");
  if (event.shiftKey) parts.push("shift");
  parts.push(event.key.toLowerCase());
  return parts.join("+");
}

/**
 * In-window shortcuts. Matching handlers also suppress the webview's own
 * bindings (Ctrl+P would otherwise open the print dialog).
 */
export function useShortcuts(shortcuts: ShortcutMap): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const handler = shortcuts[shortcutKey(event)];
      if (!handler) return;
      event.preventDefault();
      handler();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [shortcuts]);
}
