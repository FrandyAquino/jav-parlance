import { useEffect } from "react";
import type { Theme } from "@/bindings/Theme";

/**
 * Applies the theme and declares whether the OS backdrop effect is available.
 * Linux compositors give the webview no blur, so surfaces become opaque there.
 */
export function useTheme(theme: Theme | undefined, platform: string | undefined): void {
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.effects = platform === "linux" ? "none" : "native";
  }, [platform]);

  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      root.dataset.theme = theme === "light" || theme === "dark" ? theme : media.matches ? "dark" : "light";
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}
