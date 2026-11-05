/** Window theme: starts from `?theme=system|light|dark`, then follows the settings. */

import { onSettingsChanged, type Theme } from "./api";

/** Forces `theme` on this window; `system` follows Windows. */
export function setTheme(theme: Theme | string | null): void {
  if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
}

/** Applies the theme the app passed in the URL and keeps it in sync with the settings. */
export function applyTheme(): void {
  setTheme(new URLSearchParams(location.search).get("theme"));
  void onSettingsChanged((s) => setTheme(s.theme));
}
