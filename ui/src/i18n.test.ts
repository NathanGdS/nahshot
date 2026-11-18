import { describe, expect, test } from "vitest";
import { LANGS, TEXTS, asLang, fill, translate, type Lang } from "./i18n";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe("locale files", () => {
  const reference = Object.keys(TEXTS.en).sort();

  test.each(LANGS.filter((l) => l !== "en"))("%s has exactly the English keys", (l) => {
    expect(Object.keys(TEXTS[l]).sort()).toEqual(reference);
  });

  test.each(LANGS)("%s keeps every placeholder of the English text", (l) => {
    for (const key of reference) {
      expect(placeholders(TEXTS[l][key]), `${l}: ${key}`).toEqual(placeholders(TEXTS.en[key]));
    }
  });

  test.each(LANGS)("%s has no empty texts", (l) => {
    for (const [key, text] of Object.entries(TEXTS[l])) expect(text.trim(), `${l}: ${key}`).not.toBe("");
  });

  test("plural keys come in pairs", () => {
    for (const key of reference) {
      if (key.endsWith(".one")) expect(reference, key).toContain(key.replace(/\.one$/, ".other"));
      if (key.endsWith(".other")) expect(reference, key).toContain(key.replace(/\.other$/, ".one"));
    }
  });
});

describe("keys used in the code", () => {
  // Every source that shows text: the web UI and the Rust side (app, overlay, toast).
  const sources: Record<string, string> = {
    ...import.meta.glob(["./*.ts", "!./*.test.ts"], { query: "?raw", import: "default", eager: true }),
    ...import.meta.glob("../*.html", { query: "?raw", import: "default", eager: true }),
    ...import.meta.glob("../../src-tauri/src/**/*.rs", { query: "?raw", import: "default", eager: true }),
    ...import.meta.glob("../../crates/nahshot-capture/src/**/*.rs", { query: "?raw", import: "default", eager: true }),
  };
  const keys = new Set(Object.keys(TEXTS.en));
  const namespaces = new Set([...keys].map((k) => k.split(".")[0]));
  /** A key, or the base of a `.one`/`.other` pair. */
  const known = (k: string) => keys.has(k) || (keys.has(`${k}.one`) && keys.has(`${k}.other`));

  // In the UI, any quoted "namespace.rest" literal: t("gallery.search"), data-i18n="welcome.title",
  // or a key kept in a table. In Rust, only the first argument of t(...) / tf(...), since Rust
  // code is full of dotted literals such as "settings.json".
  const uiKey = /["'`]([a-z]+(?:\.[A-Za-z]+)+)["'`]/g;
  const rustKey = /\btf?\(\s*"([a-z]+(?:\.[A-Za-z]+)+)"/g;
  const used = new Map<string, string>();
  for (const [file, text] of Object.entries(sources)) {
    for (const m of text.matchAll(file.endsWith(".rs") ? rustKey : uiKey)) {
      if (namespaces.has(m[1].split(".")[0])) used.set(m[1], file);
    }
  }

  test("the scan finds sources on both sides", () => {
    const files = Object.keys(sources);
    expect(files.some((f) => f.endsWith("gallery.ts"))).toBe(true);
    expect(files.some((f) => f.endsWith("tray.rs"))).toBe(true);
    expect(files.some((f) => f.endsWith("toolbar.rs"))).toBe(true);
  });

  test("every key the code uses exists", () => {
    const missing = [...used].filter(([k]) => !known(k)).map(([k, f]) => `${k} (${f})`);
    expect(missing).toEqual([]);
  });

  test("every key in the locale files is used", () => {
    const unused = [...keys].filter((k) => !used.has(k) && !used.has(k.replace(/\.(one|other)$/, "")));
    expect(unused).toEqual([]);
  });
});

describe("lookup", () => {
  test("translates and fills placeholders", () => {
    expect(translate("pt", "viewer.position", { index: 2, total: 9 })).toBe("2 de 9");
    expect(translate("es", "tray.quit")).toBe("Salir");
  });

  test("an unknown key shows itself", () => {
    expect(translate("pt", "no.such.key")).toBe("no.such.key");
  });

  test("an unknown placeholder is kept", () => {
    expect(fill("{a} {b}", { a: 1 })).toBe("1 {b}");
  });

  test("only supported codes are languages", () => {
    expect(asLang("pt")).toBe<Lang>("pt");
    expect(asLang("fr")).toBeNull();
    expect(asLang(null)).toBeNull();
  });
});
