/** Full-window viewer for one print, with its actions. Esc closes, ←/→ navigate. */

import * as api from "./api";
import type { Print } from "./api";
import { icon, type IconName } from "./icons";
import { attempt, confirmDialog, h, inlineInput, showMenuBelow, toast } from "./ui";

export interface ViewerHost {
  /** Prints in display order. */
  prints: () => Print[];
  albums: () => string[];
}

const when = new Intl.DateTimeFormat("pt-BR", { dateStyle: "long", timeStyle: "short" });
const stem = (fileName: string) => fileName.replace(/\.png$/i, "");

export class Viewer {
  private id: number | null = null;
  private renaming = false;
  private readonly root: HTMLElement;
  private readonly img = h("img", { class: "viewer-img", alt: "", draggable: "false" });
  private readonly panel = h("aside", { class: "viewer-panel" });
  private readonly prev: HTMLButtonElement;
  private readonly next: HTMLButtonElement;
  private tagInput: HTMLInputElement | null = null;

  constructor(private host: ViewerHost) {
    const nav = (dir: -1 | 1, name: IconName, label: string) =>
      h("button", { class: `viewer-nav viewer-${dir < 0 ? "prev" : "next"}`, title: label, "aria-label": label, onclick: () => this.step(dir) }, icon(name));
    this.prev = nav(-1, "left", "Anterior (←)");
    this.next = nav(1, "right", "Próximo (→)");
    const stage = h(
      "div",
      { class: "viewer-stage", onclick: (e: MouseEvent) => e.target === stage && this.close() },
      this.img,
      this.prev,
      this.next,
      h("button", { class: "viewer-close", title: "Fechar (Esc)", "aria-label": "Fechar", onclick: () => this.close() }, icon("close")),
    );
    this.root = h("div", { class: "viewer", role: "dialog", "aria-modal": "true", hidden: true }, stage, this.panel);
    document.body.append(this.root);
  }

  get isOpen(): boolean {
    return this.id !== null;
  }

  open(id: number): void {
    this.id = id;
    this.renaming = false;
    this.root.hidden = false;
    this.render();
  }

  close(): void {
    this.id = null;
    this.root.hidden = true;
    this.img.removeAttribute("src");
  }

  /** Re-reads the current print after the list reloaded; closes if it is gone. */
  refresh(): void {
    if (this.id === null || this.renaming) return;
    if (!this.current()) this.close();
    else this.render();
  }

  /** Handles a key while open; returns whether it was consumed. */
  onKey(e: KeyboardEvent): boolean {
    const typing = e.target instanceof HTMLInputElement;
    if (e.key === "Escape") this.close();
    else if (typing) return false;
    else if (e.key === "ArrowLeft") this.step(-1);
    else if (e.key === "ArrowRight") this.step(1);
    else if (e.key === "Delete") this.delete();
    else return false;
    return true;
  }

  private current(): Print | undefined {
    return this.host.prints().find((p) => p.id === this.id);
  }

  private step(dir: -1 | 1): void {
    const list = this.host.prints();
    const i = list.findIndex((p) => p.id === this.id);
    const target = list[i + dir];
    if (target) this.open(target.id);
  }

  private render(): void {
    const p = this.current();
    if (!p) return;
    const list = this.host.prints();
    const index = list.indexOf(p);
    const src = api.fileSrc(p.path);
    if (this.img.getAttribute("src") !== src) this.img.src = src;
    this.img.alt = p.fileName;
    this.prev.disabled = index <= 0;
    this.next.disabled = index >= list.length - 1;

    const refocusTags = document.activeElement === this.tagInput;
    const action = (name: IconName, label: string, run: () => void, extra = "") =>
      h("button", { class: `action ${extra}`, title: label, onclick: run }, icon(name, extra.includes("on")), h("span", {}, label));

    const title = this.renaming
      ? inlineInput({
          value: stem(p.fileName),
          placeholder: "Nome do arquivo",
          submit: (v) => api.renamePrint(p.id, v),
          done: () => {
            this.renaming = false;
            this.render();
          },
        })
      : h("h2", { class: "viewer-title", title: p.fileName }, p.fileName);

    this.tagInput = h("input", { class: "tag-input", type: "text", placeholder: "Adicionar etiqueta…", spellcheck: "false" });
    this.tagInput.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      const input = e.target as HTMLInputElement;
      const tag = input.value.trim();
      if (!tag) return;
      input.value = "";
      attempt(api.addTag(p.id, tag));
    });
    const chips = p.tags.map((t) =>
      h(
        "span",
        { class: "chip" },
        `#${t}`,
        h("button", { class: "chip-x", title: `Remover ${t}`, "aria-label": `Remover ${t}`, onclick: () => attempt(api.removeTag(p.id, t)) }, icon("close")),
      ),
    );

    const albumButton = h(
      "button",
      { class: "select", onclick: () => this.albumMenu(albumButton, p) },
      icon("folder"),
      h("span", {}, p.album ?? "Sem álbum"),
      icon("down"),
    );

    this.panel.replaceChildren(
      h("header", { class: "viewer-head" }, title, h("p", { class: "viewer-meta" }, when.format(new Date(p.capturedAt)), h("span", { class: "viewer-pos" }, `${index + 1} de ${list.length}`))),
      h(
        "div",
        { class: "actions" },
        action("copy", "Copiar", () => this.copy(p)),
        action("reveal", "Abrir na pasta", () => attempt(api.revealPrint(p.id))),
        action("pencil", "Renomear", () => {
          this.renaming = true;
          this.render();
        }),
        action("star", p.favorite ? "Favorito" : "Favoritar", () => attempt(api.setFavorite(p.id, !p.favorite)), p.favorite ? "on" : ""),
      ),
      h("section", { class: "field" }, h("h3", { class: "field-label" }, "Álbum"), albumButton),
      h("section", { class: "field" }, h("h3", { class: "field-label" }, "Etiquetas"), h("div", { class: "chips" }, ...chips, this.tagInput)),
      h("div", { class: "viewer-spacer" }),
      h("button", { class: "btn btn-danger-soft", onclick: () => this.delete() }, icon("trash"), "Apagar"),
    );
    if (refocusTags) this.tagInput.focus();
  }

  private albumMenu(anchor: HTMLElement, p: Print): void {
    const albums = this.host.albums();
    showMenuBelow(anchor, [
      ...albums.map((a) => ({ label: a, icon: "folder" as const, checked: p.album === a, action: () => attempt(api.moveToAlbum([p.id], a)) })),
      ...(albums.length ? ["-" as const] : []),
      { label: "Sem álbum", checked: p.album === null, action: () => attempt(api.removeFromAlbum([p.id])) },
    ]);
  }

  private async copy(p: Print): Promise<void> {
    if (await attempt(api.copyPrint(p.id))) toast("Copiado para a área de transferência");
  }

  private async delete(): Promise<void> {
    const p = this.current();
    if (!p) return;
    const ok = await confirmDialog({ title: "Mover 1 print(s) para a Lixeira?", confirm: "Mover para a Lixeira", danger: true });
    if (!ok) return;
    const list = this.host.prints();
    const neighbor = list[list.indexOf(p) + 1] ?? list[list.indexOf(p) - 1];
    if (await attempt(api.deletePrints([p.id]))) {
      if (neighbor) this.open(neighbor.id);
      else this.close();
    }
  }
}
