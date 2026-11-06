/** Settings window: prints folder, autostart, Print Screen, capture hotkey, theme and updates. */

import "./theme.css";
import "./pages.css";
import * as api from "./api";
import type { Settings, Theme } from "./api";
import { accelerator, heldModifiers, isModifierKey } from "./hotkey";
import { icon } from "./icons";
import { applyTheme, setTheme } from "./theme";
import { choose, confirmDialog, h, toast } from "./ui";

applyTheme();

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
const folderLabel = h("span", {}, "Alterar…");
const folderButton = h("button", { class: "btn", type: "button", onclick: () => changeFolder() }, icon("folder"), folderLabel);

const samePath = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

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
    title: "Mover os prints existentes para a nova pasta?",
    body: `Nova pasta: ${picked}. Se não mover, os prints antigos continuam na galeria a partir da pasta atual.`,
    choices: [
      { value: "cancel", label: "Cancelar" },
      { value: "no", label: "Não" },
      { value: "yes", label: "Sim", kind: "primary" },
    ],
  });
  if (answer !== "yes" && answer !== "no") return;

  folderButton.disabled = true;
  folderLabel.textContent = answer === "yes" ? "Movendo…" : "Alterando…";
  try {
    const r = await api.changePrintsDir(picked, answer === "yes");
    if (r.failed > 0) {
      toast(
        `Pasta alterada. ${plural(r.moved, "print movido", "prints movidos")}; ` +
          `${plural(r.failed, "não pôde ser movido e continua", "não puderam ser movidos e continuam")} na pasta antiga.`,
        "error",
      );
    } else if (answer === "yes") {
      toast(`Pasta alterada. ${plural(r.moved, "print movido", "prints movidos")}.`);
    } else {
      toast("Pasta alterada. Os prints antigos continuam na galeria.");
    }
  } catch (e) {
    toast(`Não foi possível trocar a pasta: ${api.friendlyError(e)}`, "error");
  } finally {
    folderButton.disabled = false;
    folderLabel.textContent = "Alterar…";
  }
}

// ---- switches ------------------------------------------------------------------

const autostart = toggle("Iniciar com o Windows", api.setAutostart);
const printScreen = toggle("Usar o nahshot como Print Screen padrão", api.setDefaultPrintScreen);

// ---- capture hotkey --------------------------------------------------------------

const hotkey = h("button", { class: "hotkey", type: "button", "aria-label": "Atalho de captura" });
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
  showHotkey("Pressione a combinação…", true);
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
    hotkeyError.textContent = "Tecla não suportada. Use letras, números, F1–F24 ou Print Screen.";
    return;
  }
  if (mods.length === 0) {
    hotkeyError.textContent = "Use pelo menos um modificador: Ctrl, Alt, Shift ou Win.";
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

const themes: [Theme, string][] = [
  ["system", "Sistema"],
  ["light", "Claro"],
  ["dark", "Escuro"],
];
const themeButtons = themes.map(([value, label]) =>
  h(
    "button",
    {
      class: "segment",
      type: "button",
      role: "radio",
      onclick: async () => {
        if (current?.theme === value) return;
        setTheme(value);
        try {
          await api.setTheme(value);
        } catch (e) {
          toast(api.friendlyError(e), "error");
          if (current) render(current);
        }
      },
    },
    label,
  ),
);
const themePicker = h("div", { class: "segmented", role: "radiogroup", "aria-label": "Tema" }, ...themeButtons);

// ---- updates ---------------------------------------------------------------------

const updateStatus = h("p", { class: "path", role: "status" });
const updateLabel = h("span", {}, "Procurar atualizações");
const updateButton = h("button", { class: "btn", type: "button", onclick: () => checkForUpdate() }, updateLabel);
let installing = false;

function idle(): void {
  installing = false;
  updateButton.disabled = false;
  updateLabel.textContent = "Procurar atualizações";
}

async function checkForUpdate(): Promise<void> {
  updateButton.disabled = true;
  updateLabel.textContent = "Procurando…";
  try {
    const info = await api.checkUpdate();
    updateStatus.textContent = `Versão ${info.current}`;
    if (!info.latest) {
      updateStatus.textContent = `Versão ${info.current}. Você está na versão mais recente.`;
      return idle();
    }
    if (info.devBuild) {
      updateStatus.textContent = `Versão ${info.latest} disponível, mas este é um build de desenvolvimento e não se atualiza.`;
      return idle();
    }
    const ok = await confirmDialog({
      title: `Atualizar para a versão ${info.latest}?`,
      body: `Você tem a ${info.current}. O nahshot fecha, atualiza e abre de novo sozinho.`,
      confirm: "Atualizar agora",
    });
    if (!ok) return idle();
    installing = true;
    updateLabel.textContent = "Baixando…";
    await api.installUpdate();
    // A successful install closes the app, so getting here means it did not happen.
    idle();
  } catch (e) {
    toast(api.friendlyError(e), "error");
    idle();
  }
}

void api.onUpdateProgress((pct) => {
  if (installing) updateLabel.textContent = pct === null ? "Baixando…" : `Baixando… ${pct}%`;
});

// ---- layout ----------------------------------------------------------------------

document.querySelector("#sections")!.append(
  row("Pasta dos prints", null, folderButton, folderPath),
  row("Iniciar com o Windows", "Abre o nahshot na bandeja quando você entra no Windows.", autostart),
  row(
    "Usar o nahshot como Print Screen padrão",
    "Desligado, o Print Screen fica com o Windows e o nahshot abre pelo atalho abaixo ou pela bandeja.",
    printScreen,
  ),
  row("Atalho de captura", "Clique no campo e pressione a combinação.", hotkey, hotkeyError),
  row("Tema", null, themePicker),
  row("Atualizações", "O nahshot também procura sozinho quando abre.", updateButton, updateStatus),
);

/** Shows `s` in every control. */
function render(s: Settings): void {
  current = s;
  folderPath.textContent = s.printsDir;
  folderPath.title = s.printsDir;
  autostart.checked = s.autostart;
  printScreen.checked = s.defaultPrintScreen;
  if (!recording) showHotkey(s.altHotkey);
  themeButtons.forEach((b, i) => b.setAttribute("aria-checked", String(themes[i][0] === s.theme)));
}

void api.onSettingsChanged(render);
api.getSettings().then(render, (e) => toast(`Não foi possível ler as configurações: ${api.friendlyError(e)}`, "error"));
