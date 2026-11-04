/** Small DOM helpers and shared widgets: menus, confirm dialog, toasts, inline inputs. */

import { friendlyError } from "./api";
import { icon, type IconName } from "./icons";

type Child = Node | string | null | undefined | false;

/** Creates an element. `on*` function props become listeners, `class` sets the class list. */
export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Record<string, unknown> = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = String(value);
    else if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2), value as EventListener);
    } else if (typeof value === "string" || value === true) {
      el.setAttribute(key, value === true ? "" : value);
    } else {
      (el as unknown as Record<string, unknown>)[key] = value;
    }
  }
  el.append(...children.filter((c): c is Node | string => c != null && c !== false));
  return el;
}

/** One entry of a popup menu; `"-"` is a separator. */
export type MenuItem =
  | "-"
  | { label: string; icon?: IconName; checked?: boolean; danger?: boolean; action: () => void };

let closeMenu: (() => void) | null = null;

/** Shows a popup menu at (x, y), kept inside the window. */
export function showMenu(x: number, y: number, items: MenuItem[]): void {
  closeMenu?.();
  const menu = h("div", { class: "menu", role: "menu" });
  for (const item of items) {
    if (item === "-") {
      menu.append(h("div", { class: "menu-sep", role: "separator" }));
      continue;
    }
    const lead = item.checked ? icon("check") : item.icon ? icon(item.icon) : h("span", { class: "icon" });
    menu.append(
      h(
        "button",
        {
          class: `menu-item${item.danger ? " danger" : ""}`,
          role: "menuitem",
          onclick: () => {
            closeMenu?.();
            item.action();
          },
        },
        lead,
        h("span", {}, item.label),
      ),
    );
  }
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, innerWidth - r.width - 8)}px`;
  menu.style.top = `${Math.min(y, innerHeight - r.height - 8)}px`;
  (menu.querySelector("button") as HTMLButtonElement | null)?.focus();

  const onDown = (e: Event) => {
    if (!menu.contains(e.target as Node)) closeMenu?.();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closeMenu?.();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const buttons = [...menu.querySelectorAll<HTMLButtonElement>(".menu-item")];
      const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = (i + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next]?.focus();
    }
  };
  closeMenu = () => {
    menu.remove();
    removeEventListener("pointerdown", onDown, true);
    removeEventListener("keydown", onKey, true);
    removeEventListener("blur", closeMenu!);
    closeMenu = null;
  };
  addEventListener("pointerdown", onDown, true);
  addEventListener("keydown", onKey, true);
  addEventListener("blur", closeMenu);
}

/** Opens a menu right under `anchor`. */
export function showMenuBelow(anchor: HTMLElement, items: MenuItem[]): void {
  const r = anchor.getBoundingClientRect();
  showMenu(r.left, r.bottom + 4, items);
}

/** One button of a {@link choose} dialog. */
export interface Choice {
  value: string;
  label: string;
  kind?: "primary" | "danger";
}

/**
 * Asks a question with several answers in a modal dialog. Resolves to the chosen
 * value, or `null` when dismissed with Esc. The last choice gets the focus.
 */
export function choose(opts: { title: string; body?: string; choices: Choice[] }): Promise<string | null> {
  return new Promise((resolve) => {
    const buttons = opts.choices.map((c) => h("button", { class: `btn${c.kind ? ` btn-${c.kind}` : ""}`, value: c.value }, c.label));
    const dialog = h(
      "dialog",
      { class: "dialog" },
      h(
        "form",
        { method: "dialog" },
        h("h2", { class: "dialog-title" }, opts.title),
        opts.body ? h("p", { class: "dialog-body" }, opts.body) : null,
        h("div", { class: "dialog-actions" }, ...buttons),
      ),
    );
    dialog.addEventListener("close", () => {
      resolve(dialog.returnValue || null);
      dialog.remove();
    });
    document.body.append(dialog);
    dialog.showModal();
    buttons.at(-1)?.focus();
  });
}

/** Asks a yes/no question in a modal dialog; resolves to the answer. */
export async function confirmDialog(opts: { title: string; body?: string; confirm: string; danger?: boolean }): Promise<boolean> {
  const answer = await choose({
    title: opts.title,
    body: opts.body,
    choices: [
      { value: "cancel", label: "Cancelar" },
      { value: "ok", label: opts.confirm, kind: opts.danger ? "danger" : "primary" },
    ],
  });
  return answer === "ok";
}

/** Shows a short message at the bottom of the window. */
export function toast(message: string, kind: "info" | "error" = "info"): void {
  const t = h("div", { class: `toast toast-${kind}`, role: kind === "error" ? "alert" : "status" }, message);
  document.body.append(t);
  requestAnimationFrame(() => t.classList.add("show"));
  setTimeout(() => {
    t.classList.remove("show");
    t.addEventListener("transitionend", () => t.remove(), { once: true });
  }, kind === "error" ? 4500 : 2200);
}

/** Awaits a command, reporting a failure as an error toast. Resolves to whether it worked. */
export async function attempt(work: Promise<unknown>): Promise<boolean> {
  try {
    await work;
    return true;
  } catch (e) {
    toast(friendlyError(e), "error");
    return false;
  }
}

/**
 * A text field that submits on Enter and cancels on Esc (or on blur while unchanged).
 * `submit` rejecting shows its error under the field and keeps it open.
 */
export function inlineInput(opts: {
  value?: string;
  placeholder?: string;
  submit: (value: string) => Promise<unknown>;
  done: () => void;
}): HTMLElement {
  const input = h("input", { class: "inline-input", type: "text", spellcheck: "false", placeholder: opts.placeholder ?? "" });
  input.value = opts.value ?? "";
  const error = h("p", { class: "field-error", role: "alert" });
  const wrap = h("div", { class: "inline-field" }, input, error);
  let busy = false;
  let closed = false;
  const done = () => {
    if (closed) return;
    closed = true;
    opts.done();
  };
  input.addEventListener("keydown", async (e) => {
    e.stopPropagation();
    if (e.key === "Escape") done();
    if (e.key !== "Enter" || busy) return;
    busy = true;
    try {
      await opts.submit(input.value);
      done();
    } catch (err) {
      error.textContent = friendlyError(err);
      input.classList.add("invalid");
      input.select();
    } finally {
      busy = false;
    }
  });
  input.addEventListener("input", () => {
    error.textContent = "";
    input.classList.remove("invalid");
  });
  input.addEventListener("blur", () => {
    if (!busy && !error.textContent && input.value === (opts.value ?? "")) done();
  });
  requestAnimationFrame(() => {
    input.focus();
    input.select();
  });
  return wrap;
}
