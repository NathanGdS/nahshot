/** Settings window: prints folder, autostart, Print Screen, capture hotkey, theme, language and updates. */

import "./theme.css";
import "./pages.css";
import * as api from "./api";
import type { Language, Settings, Theme } from "./api";
import { accelerator, heldModifiers, isModifierKey } from "./hotkey";
import { LANG_NAMES, LANGS, localizeDocument, t, tn } from "./i18n";
import { icon } from "./icons";
import { applyTheme, setTheme } from "./theme";
import { choose, confirmDialog, h, toast } from "./ui";

applyTheme();
localizeDocument();

let current: Settings | null = null;

/** A settings row: title and description on the left, the control on the right. */
function row(title: string, description: string | null, control: HTMLElement, extra?: HTMLElement): HTMLElement {
  return h(
    "section",
    { class: "row" },
    h("div", { class: "row-text" }, h("h2", { class: "row-title" }, title), description ? h("p", { class: "row-desc" }, description) : null, extra),
    h("div", { class: "row-control" }, control),
  );
}

/** A switch-styled checkbox that calls `save` on change and reverts if it fails. */
function toggle(label: string, save: (on: boolean) => Promise<unknown>): HTMLInputElement {
  const input = h("input", { class: "switch", type: "checkbox", role: "switch", "aria-label": label });
  input.addEventListener("change", async () => {
    input.disabled = true;
    try {
      await save(input.checked);
    } catch (e) {
      input.checked = !input.checked;
      toast(api.friendlyError(e), "error");
    } finally {
      input.disabled = false;
    }
  });
  return input;
}

// ---- prints folder -------------------------------------------------------------

const folderPath = h("p", { class: "path" });
const folderLabel = h("span", {}, t("settings.folder.change"));
const folderButton = h("button", { class: "btn", type: "button", onclick: () => changeFolder() }, icon("folder"), folderLabel);

const samePath = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

async function changeFolder(): Promise<void> {
  if (!current) return;
  let picked: string | null;
  try {
    picked = await api.pickPrintsDir();
  } catch (e) {
    return toast(api.friendlyError(e), "error");
  }
  if (!picked || samePath(picked, current.printsDir)) return;
  const answer = await choose({
    title: t("settings.folder.moveTitle"),
    body: t("settings.folder.moveBody", { path: picked }),
    choices: [
      { value: "cancel", label: t("common.cancel") },
      { value: "no", label: t("common.no") },
      { value: "yes", label: t("common.yes"), kind: "primary" },
    ],
  });
  if (answer !== "yes" && answer !== "no") return;

  folderButton.disabled = true;
  folderLabel.textContent = answer === "yes" ? t("settings.folder.moving") : t("settings.folder.changing");
  try {
    const r = await api.changePrintsDir(picked, answer === "yes");
    const moved = tn("settings.folder.moved", r.moved);
    if (r.failed > 0) {
      toast(t("settings.folder.changedPartial", { moved, failed: tn("settings.folder.notMoved", r.failed) }), "error");
    } else if (answer === "yes") {
      toast(t("settings.folder.changedMoved", { moved }));
    } else {
      toast(t("settings.folder.changedKept"));
    }
  } catch (e) {
    toast(t("settings.folder.failed", { error: api.friendlyError(e) }), "error");
  } finally {
    folderButton.disabled = false;
    folderLabel.textContent = t("settings.folder.change");
  }
}

// ---- switches ------------------------------------------------------------------

const autostart = toggle(t("settings.autostart.title"), api.setAutostart);
const printScreen = toggle(t("settings.printScreen.title"), api.setDefaultPrintScreen);

// ---- capture hotkey --------------------------------------------------------------

const hotkey = h("button", { class: "hotkey", type: "button", "aria-label": t("settings.hotkey.title") });
const hotkeyError = h("p", { class: "field-error", role: "alert" });
let recording = false;

function showHotkey(text: string, placeholder = false): void {
  hotkey.replaceChildren(
    ...(placeholder
      ? [h("span", { class: "hotkey-hint" }, text)]
      : text.split("+").flatMap((part, i) => [...(i > 0 ? [h("span", { class: "hotkey-plus" }, "+")] : []), h("kbd", {}, part)])),
  );
}

function startRecording(): void {
  recording = true;
  hotkey.classList.add("recording");
  hotkeyError.textContent = "";
  showHotkey(t("settings.hotkey.recording"), true);
}

function stopRecording(): void {
  recording = false;
  hotkey.classList.remove("recording");
  if (current) showHotkey(current.altHotkey);
}

hotkey.addEventListener("click", () => (recording ? stopRecording() : startRecording()));
hotkey.addEventListener("blur", () => recording && stopRecording());
hotkey.addEventListener("keydown", async (e) => {
  if (!recording) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      startRecording();
    }
    return;
  }
  const mods = heldModifiers(e);
  if (e.key === "Tab" && mods.length === 0) return stopRecording();
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape" && mods.length === 0) return stopRecording();
  if (isModifierKey(e.key)) return showHotkey(`${mods.join("+")}+…`, true);
  const combo = accelerator(e);
  if (!combo) {
    hotkeyError.textContent = t("settings.hotkey.unsupported");
    return;
  }
  if (mods.length === 0) {
    hotkeyError.textContent = t("settings.hotkey.needModifier");
    return;
  }
  try {
    const saved = await api.setAltHotkey(combo);
    if (current) current.altHotkey = saved;
    hotkeyError.textContent = "";
    stopRecording();
  } catch (err) {
    hotkeyError.textContent = api.friendlyError(err);
  }
});

// ---- theme -----------------------------------------------------------------------

/** A row of radio-like buttons; `pick` runs when a different option is chosen. */
function segmented<T extends string>(label: string, options: [T, string][], pick: (value: T) => void): { el: HTMLElement; buttons: HTMLButtonElement[] } {
  const buttons = options.map(([value, text]) => h("button", { class: "segment", type: "button", role: "radio", onclick: () => pick(value) }, text));
  return { el: h("div", { class: "segmented", role: "radiogroup", "aria-label": label }, ...buttons), buttons };
}

const themes: [Theme, string][] = [
  ["system", t("settings.theme.system")],
  ["light", t("settings.theme.light")],
  ["dark", t("settings.theme.dark")],
];
const themePicker = segmented(t("settings.theme.title"), themes, async (value) => {
  if (current?.theme === value) return;
  setTheme(value);
  try {
    await api.setTheme(value);
  } catch (e) {
    toast(api.friendlyError(e), "error");
    if (current) render(current);
  }
});

// ---- language --------------------------------------------------------------------

const languages: [Language, string][] = [["system", t("settings.language.system")], ...LANGS.map((l): [Language, string] => [l, LANG_NAMES[l]])];
const languagePicker = segmented(t("settings.language.title"), languages, async (value) => {
  if (current?.language === value) return;
  // On success the app reloads this window in the new language.
  try {
    await api.setLanguage(value);
  } catch (e) {
    toast(api.friendlyError(e), "error");
  }
});

// ---- updates ---------------------------------------------------------------------

const updateStatus = h("p", { class: "path", role: "status" });
const updateLabel = h("span", {}, t("settings.updates.check"));
const updateButton = h("button", { class: "btn", type: "button", onclick: () => checkForUpdate() }, updateLabel);
let installing = false;

function idle(): void {
  installing = false;
  updateButton.disabled = false;
  updateLabel.textContent = t("settings.updates.check");
}

async function checkForUpdate(): Promise<void> {
  updateButton.disabled = true;
  updateLabel.textContent = t("settings.updates.checking");
  try {
    const info = await api.checkUpdate();
    updateStatus.textContent = t("settings.updates.version", { version: info.current });
    if (!info.latest) {
      updateStatus.textContent = t("settings.updates.upToDate", { version: info.current });
      return idle();
    }
    if (info.devBuild) {
      updateStatus.textContent = t("settings.updates.devBuild", { version: info.latest });
      return idle();
    }
    const ok = await confirmDialog({
      title: t("settings.updates.confirmTitle", { version: info.latest }),
      body: t("settings.updates.confirmBody", { current: info.current }),
      confirm: t("update.now"),
    });
    if (!ok) return idle();
    installing = true;
    updateLabel.textContent = t("settings.updates.downloading");
    await api.installUpdate();
    // A successful install closes the app, so getting here means it did not happen.
    idle();
  } catch (e) {
    toast(api.friendlyError(e), "error");
    idle();
  }
}

void api.onUpdateProgress((pct) => {
  if (installing) updateLabel.textContent = pct === null ? t("settings.updates.downloading") : t("settings.updates.downloadingPct", { pct });
});

// ---- layout ----------------------------------------------------------------------

document.querySelector("#sections")!.append(
  row(t("settings.folder.title"), null, folderButton, folderPath),
  row(t("settings.autostart.title"), t("settings.autostart.desc"), autostart),
  row(t("settings.printScreen.title"), t("settings.printScreen.desc"), printScreen),
  row(t("settings.hotkey.title"), t("settings.hotkey.desc"), hotkey, hotkeyError),
  row(t("settings.theme.title"), null, themePicker.el),
  row(t("settings.language.title"), null, languagePicker.el),
  row(t("settings.updates.title"), t("settings.updates.desc"), updateButton, updateStatus),
);

/** Shows `s` in every control. */
function render(s: Settings): void {
  current = s;
  folderPath.textContent = s.printsDir;
  folderPath.title = s.printsDir;
  autostart.checked = s.autostart;
  printScreen.checked = s.defaultPrintScreen;
  if (!recording) showHotkey(s.altHotkey);
  themePicker.buttons.forEach((b, i) => b.setAttribute("aria-checked", String(themes[i][0] === s.theme)));
  languagePicker.buttons.forEach((b, i) => b.setAttribute("aria-checked", String(languages[i][0] === s.language)));
}

void api.onSettingsChanged(render);
api.getSettings().then(render, (e) => toast(t("settings.loadFailed", { error: api.friendlyError(e) }), "error"));
