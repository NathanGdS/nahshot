/** The gallery's left sidebar: All, Favorites, albums (create/rename/delete, drop target) and tags. */

import * as api from "./api";
import type { Filter, Sidebar } from "./api";
import { t } from "./i18n";
import { icon, type IconName } from "./icons";
import { attempt, confirmDialog, h, inlineInput, showMenu } from "./ui";

/** Drag-and-drop type carrying a JSON array of print ids. */
export const DRAG_TYPE = "application/x-nahshot-ids";

export interface SidebarHost {
  /** The filter currently shown. */
  filter: () => Filter;
  /** Switches the grid to `filter`. */
  select: (filter: Filter) => void;
}

const sameFilter = (a: Filter, b: Filter) =>
  a.kind === b.kind && ("value" in a ? a.value : null) === ("value" in b ? b.value : null);

/** Renders into `root` and re-renders on `update`, deferring while an inline edit is open. */
export class SidebarView {
  private data: Sidebar = { albums: [], tags: [], total: 0, favorites: 0 };
  /** `"new"` while creating an album, the album name while renaming it. */
  private editing: string | null = null;

  constructor(
    private root: HTMLElement,
    private host: SidebarHost,
  ) {}

  update(data: Sidebar): void {
    this.data = data;
    if (this.editing === null) this.render();
  }

  render(): void {
    const { albums, tags, total, favorites } = this.data;
    const current = this.host.filter();
    const item = (filter: Filter, iconName: IconName, label: string, count?: number) => {
      const active = sameFilter(filter, current);
      return h(
        "button",
        {
          class: `nav-item${active ? " active" : ""}`,
          "aria-current": active ? "page" : null,
          title: label,
          onclick: () => this.host.select(filter),
        },
        icon(iconName, iconName === "star" && active),
        h("span", { class: "nav-label" }, label),
        count === undefined ? null : h("span", { class: "nav-count" }, String(count)),
      );
    };

    const albumItems = albums.map((name) =>
      this.editing === name
        ? this.editField(name)
        : this.dropTarget(item({ kind: "album", value: name }, "folder", name), name),
    );
    const newAlbum =
      this.editing === "new"
        ? this.editField(null)
        : h(
            "button",
            { class: "nav-item nav-add", onclick: () => this.edit("new") },
            icon("plus"),
            h("span", { class: "nav-label" }, t("album.new")),
          );

    this.root.replaceChildren(
      h("div", { class: "brand" }, h("span", { class: "brand-mark" }), "nahshot"),
      h(
        "nav",
        { class: "nav" },
        item({ kind: "all" }, "photos", t("filter.all"), total),
        item({ kind: "favorites" }, "star", t("filter.favorites"), favorites),
        h("h2", { class: "nav-heading" }, t("sidebar.albums")),
        ...albumItems,
        newAlbum,
        h("h2", { class: "nav-heading" }, t("sidebar.tags")),
        tags.length
          ? h("div", { class: "nav-tags" }, ...tags.map((tag) => item({ kind: "tag", value: tag }, "hash", tag)))
          : h("p", { class: "nav-empty" }, t("sidebar.noTags")),
      ),
      h(
        "button",
        { class: "nav-item nav-footer", onclick: () => attempt(api.openSettings()) },
        icon("gear"),
        h("span", { class: "nav-label" }, t("sidebar.settings")),
      ),
    );
  }

  private edit(target: string | null): void {
    this.editing = target;
    this.render();
  }

  /** Inline field creating an album (`album === null`) or renaming `album`. */
  private editField(album: string | null): HTMLElement {
    const field = inlineInput({
      value: album ?? "",
      placeholder: t("album.namePlaceholder"),
      submit: async (value) => {
        if (album === null) {
          await api.createAlbum(value);
        } else if (value.trim() !== album) {
          const renamed = await api.renameAlbum(album, value);
          const f = this.host.filter();
          if (f.kind === "album" && f.value === album) this.host.select({ kind: "album", value: renamed });
        }
      },
      done: () => this.edit(null),
    });
    return h("div", { class: "nav-edit" }, icon(album === null ? "plus" : "folder"), field);
  }

  /** Makes an album item accept dropped prints and offer Rename / Delete on right click. */
  private dropTarget(el: HTMLElement, album: string): HTMLElement {
    const accepts = (e: DragEvent) => e.dataTransfer?.types.includes(DRAG_TYPE) ?? false;
    el.addEventListener("dragover", (e) => {
      if (!accepts(e)) return;
      e.preventDefault();
      e.dataTransfer!.dropEffect = "move";
      el.classList.add("drop-target");
    });
    el.addEventListener("dragleave", () => el.classList.remove("drop-target"));
    el.addEventListener("drop", (e) => {
      el.classList.remove("drop-target");
      if (!accepts(e)) return;
      e.preventDefault();
      const ids = JSON.parse(e.dataTransfer!.getData(DRAG_TYPE)) as number[];
      attempt(api.moveToAlbum(ids, album));
    });
    el.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      showMenu(e.clientX, e.clientY, [
        { label: t("action.rename"), icon: "pencil", action: () => this.edit(album) },
        { label: t("action.delete"), icon: "trash", danger: true, action: () => this.deleteAlbum(album) },
      ]);
    });
    return el;
  }

  private async deleteAlbum(album: string): Promise<void> {
    const ok = await confirmDialog({
      title: t("album.deleteTitle", { name: album }),
      body: t("album.deleteBody"),
      confirm: t("album.deleteConfirm"),
      danger: true,
    });
    if (!ok) return;
    const f = this.host.filter();
    if (await attempt(api.deleteAlbum(album))) {
      if (f.kind === "album" && f.value === album) this.host.select({ kind: "all" });
    }
  }
}
