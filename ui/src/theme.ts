/** Applies the theme the app passes as `?theme=system|light|dark`. */
export function applyTheme(): void {
  const theme = new URLSearchParams(location.search).get("theme");
  if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
}
