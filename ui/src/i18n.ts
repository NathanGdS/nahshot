/**
 * The UI texts, from the same `locales/<code>.json` files the Rust side embeds.
 *
 * The app passes the resolved language in the URL (`?lang=en|pt|es`); changing it
 * reloads the window, so the language is fixed for the page's lifetime.
 */

import en from "../../locales/en.json";
import es from "../../locales/es.json";
import pt from "../../locales/pt.json";

export type Lang = "en" | "pt" | "es";

export const LANGS: Lang[] = ["en", "pt", "es"];

/** Each language's texts. English is the reference and the fallback. */
export const TEXTS: Record<Lang, Record<string, string>> = { en, pt, es };

/** Native name of each language, shown as is in every language. */
export const LANG_NAMES: Record<Lang, string> = { en: "English", pt: "Português", es: "Español" };

/** `Intl` locale of each language (dates). */
const LOCALES: Record<Lang, string> = { en: "en-US", pt: "pt-BR", es: "es" };

/** `code` when it is a supported language, else `null`. */
export function asLang(code: string | null | undefined): Lang | null {
  return LANGS.includes(code as Lang) ? (code as Lang) : null;
}

/** The page's language: `?lang=`, else the browser's, else English. */
export const lang: Lang =
  asLang(new URLSearchParams(globalThis.location?.search ?? "").get("lang")) ??
  asLang(globalThis.navigator?.language?.slice(0, 2)) ??
  "en";

/** The `Intl` locale for dates in the page's language. */
export const locale = (l: Lang = lang) => LOCALES[l];

export type Vars = Record<string, string | number>;

/** Replaces each `{name}` in `text` with its value. */
export function fill(text: string, vars: Vars = {}): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in vars ? String(vars[name]) : whole));
}

/** The text for `key` in `l`, falling back to English and then to the key itself. */
export function translate(l: Lang, key: string, vars?: Vars): string {
  return fill(TEXTS[l][key] ?? TEXTS.en[key] ?? key, vars);
}

/** The text for `key` in the page's language. */
export const t = (key: string, vars?: Vars) => translate(lang, key, vars);

/**
 * The plural form of `key` for `count`: `key.one` when it is 1, `key.other` otherwise
 * (right for en, es and pt as the app writes it), with `{count}` filled in.
 */
export function tn(key: string, count: number, vars: Vars = {}): string {
  return t(`${key}.${count === 1 ? "one" : "other"}`, { count, ...vars });
}

/**
 * Fills the static HTML: `data-i18n` sets the text, `data-i18n-<attr>` sets that
 * attribute (`placeholder`, `aria-label`, `title`). Also sets `<html lang>`.
 */
export function localizeDocument(root: ParentNode = document): void {
  document.documentElement.lang = locale();
  for (const el of root.querySelectorAll<HTMLElement>("[data-i18n]")) el.textContent = t(el.dataset.i18n!);
  for (const attr of ["placeholder", "aria-label", "title"]) {
    for (const el of root.querySelectorAll<HTMLElement>(`[data-i18n-${attr}]`)) {
      el.setAttribute(attr, t(el.getAttribute(`data-i18n-${attr}`)!));
    }
  }
}
