// Theme handling: resolves "system" against prefers-color-scheme and writes
// `data-theme` on <html>, which index.css keys its tokens off.

import type { Theme } from "./ipc";

const media = () =>
  typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia("(prefers-color-scheme: dark)")
    : null;

export function resolveTheme(theme: Theme): "light" | "dark" {
  if (theme !== "system") return theme;
  return media()?.matches ? "dark" : "light";
}

let unlistenSystem: (() => void) | null = null;

/** Apply `theme` now and keep following the OS while it is "system". */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = resolveTheme(theme);
  unlistenSystem?.();
  unlistenSystem = null;
  if (theme === "system") {
    const m = media();
    if (!m) return;
    const handler = () => {
      document.documentElement.dataset.theme = m.matches ? "dark" : "light";
    };
    m.addEventListener?.("change", handler);
    unlistenSystem = () => m.removeEventListener?.("change", handler);
  }
}
