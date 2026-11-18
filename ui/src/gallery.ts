/**
 * Gallery window: toolbar, day-grouped thumbnail grid with selection (clicks, check
 * circles, day checks, drag rectangle), the selection bar and context menu, sidebar and viewer.
 */

import "./theme.css";
import * as api from "./api";
import type { Filter, Print, Sidebar } from "./api";
import { groupByDay, type DayLabels, type Order } from "./group";
import { locale, localizeDocument, t, tn } from "./i18n";
import { icon, type IconName } from "./icons";
import { DRAG_TYPE, SidebarView } from "./sidebar";
import { applyTheme } from "./theme";
import { clickTile, dayState, favoriteTarget, marqueeSelect, rectFrom, sameSet, toggleDay, type Rect } from "./selection";
import { attempt, confirmDialog, h, inlineInput, showMenu, showMenuBelow, toast, type MenuItem } from "./ui";
import { Viewer } from "./viewer";

applyTheme();
localizeDocument();

const state = {
  filter: { kind: "all" } as Filter,
  search: "",
  order: "newest" as Order,
  /** Prints in display order (the grid's reading order). */
  prints: [] as Print[],
  sidebar: { albums: [], tags: [], total: 0, favorites: 0 } as Sidebar,
  selected: new Set<number>(),
  /** Last toggled or plainly clicked print: the fixed end of a Shift range. */
  anchor: null as number | null,
  /** Print ids of each day section, in display order. */
  days: [] as number[][],
};

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T;
const grid = $<HTMLElement>("#grid");
const content = $<HTMLElement>("#content");
const title = $<HTMLElement>("#title");
const search = $<HTMLInputElement>("#search");
const sortButton = $<HTMLButtonElement>("#sort");
const toolbar = $<HTMLElement>(".toolbar");
const selbar = $<HTMLElement>("#selbar");

const sidebar = new SidebarView($("#sidebar"), {
  filter: () => state.filter,
  select: (filter) => {
    state.filter = filter;
    state.selected.clear();
    paintSelection();
    content.scrollTop = 0;
    reload();
  },
});
const viewer = new Viewer({ prints: () => state.prints, albums: () => state.sidebar.albums });

// ---- loading -------------------------------------------------------------

let loadSeq = 0;

/** Reloads prints and sidebar for the current filter, keeping scroll and selection. */
async function reload(): Promise<void> {
  const seq = ++loadSeq;
  const query = {
    filter: state.filter,
    search: state.search.trim() || null,
    order: state.order === "newest" ? "newestFirst" : "oldestFirst",
  } as const;
  const [prints, side] = await Promise.all([api.listPrints(query), api.getSidebar()]).catch((e) => {
    toast(t("gallery.loadFailed", { error: api.friendlyError(e) }), "error");
    return [null, null] as const;
  });
  if (seq !== loadSeq || !prints || !side) return;

  // A filter whose album or tag vanished falls back to "All".
  const f = state.filter;
  if ((f.kind === "album" && !side.albums.includes(f.value)) || (f.kind === "tag" && !side.tags.includes(f.value))) {
    state.filter = { kind: "all" };
    return reload();
  }

  state.sidebar = side;
  const groups = groupByDay(prints, new Date(), state.order, dayLabels);
  state.prints = groups.flatMap((g) => g.items);
  const ids = new Set(state.prints.map((p) => p.id));
  for (const id of state.selected) if (!ids.has(id)) state.selected.delete(id);

  sidebar.update(side);
  renderGrid(groups);
  paintSelection();
  viewer.refresh();
  fillThumbs(seq);
}

/** Thumbnails built per round trip, so the first rows show up quickly. */
const THUMB_BATCH = 24;

/** Builds the missing thumbnails in display order, swapping each into its tile. Stops when a newer load starts. */
async function fillThumbs(seq: number): Promise<void> {
  const missing = state.prints.filter((p) => !p.thumb);
  for (let i = 0; i < missing.length && seq === loadSeq; i += THUMB_BATCH) {
    const batch = missing.slice(i, i + THUMB_BATCH);
    const built = await api.ensureThumbs(batch.map((p) => p.id)).catch(() => []);
    if (seq !== loadSeq) return;
    for (const { id, thumb } of built) {
      const print = batch.find((p) => p.id === id);
      if (print) print.thumb = thumb;
      const img = grid.querySelector<HTMLImageElement>(`.tile[data-id="${id}"] img`);
      if (img) img.src = api.fileSrc(thumb);
    }
  }
}

let changeTimer: number | undefined;
/** Coalesces bursts of `library-changed` (a move fires the command and the watcher). */
function scheduleReload(): void {
  clearTimeout(changeTimer);
  changeTimer = setTimeout(reload, 120);
}

// ---- grid ------------------------------------------------------------------

const dayLabels: DayLabels = { today: t("date.today"), yesterday: t("date.yesterday"), locale: locale() };

const filterTitle = (f: Filter) =>
  f.kind === "all" ? t("filter.all") : f.kind === "favorites" ? t("filter.favorites") : f.kind === "album" ? f.value : `#${f.value}`;

function renderGrid(groups: { label: string; items: Print[] }[]): void {
  const scroll = content.scrollTop;
  title.textContent = filterTitle(state.filter);
  const count = state.prints.length;
  $<HTMLElement>("#subtitle").textContent = tn("gallery.count", count);

  state.days = groups.map((g) => g.items.map((p) => p.id));
  if (!groups.length) {
    grid.replaceChildren(emptyState());
    return;
  }
  grid.replaceChildren(
    ...groups.map((g, i) =>
      h(
        "section",
        { class: "day" },
        h(
          "h2",
          { class: "day-label" },
          h(
            "button",
            { class: "day-check", type: "button", "data-day": String(i), title: t("gallery.selectDay", { day: g.label }), "aria-label": t("gallery.selectDay", { day: g.label }) },
            icon("check"),
          ),
          g.label,
          h("span", { class: "day-count" }, String(g.items.length)),
        ),
        h("div", { class: "tiles" }, ...g.items.map(tile)),
      ),
    ),
  );
  content.scrollTop = scroll;
}

function emptyState(): HTMLElement {
  const filtered = state.search.trim() !== "" || state.filter.kind !== "all";
  return h(
    "div",
    { class: "empty" },
    icon(filtered ? "search" : "photos"),
    h("h2", {}, filtered ? t("gallery.empty.filteredTitle") : t("gallery.empty.title")),
    h(
      "p",
      {},
      filtered ? t("gallery.empty.filteredBody") : t("gallery.empty.body"),
    ),
  );
}

function tile(p: Print): HTMLElement {
  const badges = h(
    "div",
    { class: "tile-badges" },
    p.favorite ? h("span", { class: "badge badge-star", title: t("gallery.favorite") }, icon("star", true)) : null,
    p.album ? h("span", { class: "badge" }, icon("folder"), p.album) : null,
    ...p.tags.slice(0, 2).map((t) => h("span", { class: "badge" }, `#${t}`)),
  );
  const el = h(
    "figure",
    {
      class: `tile${state.selected.has(p.id) ? " selected" : ""}`,
      draggable: "true",
      tabindex: "-1",
      title: p.fileName,
      "data-id": String(p.id),
    },
    // Without a cached thumbnail the tile shows its placeholder until `fillThumbs` gets one.
    h("img", { src: p.thumb ? api.fileSrc(p.thumb) : null, alt: p.fileName, loading: "lazy", decoding: "async", draggable: "false" }),
    h("span", { class: "tile-check", title: t("gallery.select") }, icon("check")),
    badges,
    h("figcaption", { class: "tile-name" }, p.fileName),
  );
  return el;
}

/** Repaints everything that shows the selection: tiles, day checks, selection mode and the bar. */
function paintSelection(): void {
  for (const el of grid.querySelectorAll<HTMLElement>(".tile")) {
    el.classList.toggle("selected", state.selected.has(Number(el.dataset.id)));
  }
  for (const el of grid.querySelectorAll<HTMLElement>(".day-check")) {
    const ids = state.days[Number(el.dataset.day)] ?? [];
    const s = dayState(ids, state.selected);
    el.classList.toggle("all", s === "all");
    el.classList.toggle("some", s === "some");
    el.setAttribute("aria-pressed", s === "all" ? "true" : "false");
  }
  const selecting = state.selected.size > 0;
  grid.classList.toggle("selecting", selecting);
  toolbar.classList.toggle("selecting", selecting);
  renderSelbar();
}

/** Replaces the selection and repaints. */
function setSelection(selected: Set<number>, anchor = state.anchor): void {
  state.selected = selected;
  state.anchor = anchor;
  paintSelection();
}

const tileId = (e: Event) => {
  const el = (e.target as HTMLElement).closest<HTMLElement>(".tile");
  return el ? Number(el.dataset.id) : null;
};

/** Set right after a drag rectangle, so the click that ends it does not clear the selection. */
let swallowClick = false;

grid.addEventListener("click", (e) => {
  if (swallowClick) {
    swallowClick = false;
    return;
  }
  const target = e.target as HTMLElement;
  const dayCheck = target.closest<HTMLElement>(".day-check");
  if (dayCheck) {
    const ids = state.days[Number(dayCheck.dataset.day)] ?? [];
    return setSelection(toggleDay(state.selected, ids));
  }
  const id = tileId(e);
  if (id === null) {
    if (!e.ctrlKey && !e.shiftKey) state.selected.clear();
    return paintSelection();
  }
  const order = state.prints.map((p) => p.id);
  const next = clickTile(state, order, id, { ctrl: e.ctrlKey, shift: e.shiftKey, onCheck: !!target.closest(".tile-check") });
  setSelection(next.selected, next.anchor);
});

grid.addEventListener("dblclick", (e) => {
  const id = tileId(e);
  if (id !== null && !(e.target as HTMLElement).closest(".tile-check")) viewer.open(id);
});

grid.addEventListener("contextmenu", (e) => {
  const id = tileId(e);
  if (id === null) return;
  e.preventDefault();
  if (!state.selected.has(id)) setSelection(new Set([id]), id);
  showMenu(e.clientX, e.clientY, selectionMenu({ x: e.clientX, y: e.clientY }));
});

grid.addEventListener("dragstart", (e) => {
  const id = tileId(e);
  if (id === null || !e.dataTransfer) return;
  if (!state.selected.has(id)) {
    state.selected = new Set([id]);
    state.anchor = id;
    paintSelection();
  }
  const ids = [...state.selected];
  e.dataTransfer.setData(DRAG_TYPE, JSON.stringify(ids));
  e.dataTransfer.effectAllowed = "move";
  const ghost = h("div", { class: "drag-ghost" }, icon("photos"), tn("gallery.count", ids.length));
  document.body.append(ghost);
  e.dataTransfer.setDragImage(ghost, 16, 16);
  requestAnimationFrame(() => ghost.remove());
});

// ---- selection actions -------------------------------------------------------

const selectedPrints = () => state.prints.filter((p) => state.selected.has(p.id));
const countLabel = (n: number) => tn("gallery.selected", n);

/** Where a follow-up popup opens: under a button, or at the context menu's spot. */
type Spot = HTMLElement | { x: number; y: number };

function popupAt(spot: Spot, items: MenuItem[]): void {
  if (spot instanceof HTMLElement) showMenuBelow(spot, items);
  else showMenu(spot.x, spot.y, items);
}

async function toggleFavorite(): Promise<void> {
  const prints = selectedPrints();
  if (prints.length) await attempt(api.setFavorite(prints.map((p) => p.id), favoriteTarget(prints)));
}

function albumMenu(spot: Spot): void {
  const prints = selectedPrints();
  const ids = prints.map((p) => p.id);
  const albums = state.sidebar.albums;
  const allIn = (album: string | null) => prints.length > 0 && prints.every((p) => p.album === album);
  popupAt(spot, [
    ...albums.map((a) => ({ label: a, icon: "folder" as const, checked: allIn(a), action: () => attempt(api.moveToAlbum(ids, a)) })),
    ...(albums.length ? ["-" as const] : []),
    { label: t("album.none"), checked: allIn(null), action: () => attempt(api.removeFromAlbum(ids)) },
  ]);
}

let closePopover: (() => void) | null = null;

/** A small floating box with a tag field (and the tags in use as shortcuts) that tags every selected print. */
function tagPopover(spot: Spot): void {
  closePopover?.();
  const ids = [...state.selected];
  if (!ids.length) return;
  const apply = async (tag: string) => {
    const name = tag.trim();
    if (!name) throw new Error("invalid name: name is empty");
    await api.addTag(ids, name);
    toast(tn("tag.added", ids.length, { tag: name }));
  };
  const field = inlineInput({ placeholder: t("tag.new"), submit: apply, done: () => closePopover?.() });
  const shortcuts = state.sidebar.tags.slice(0, 12).map((tag) =>
    // Pressing a chip must not blur the field first: an unchanged field closes the box on blur.
    h("button", { class: "chip chip-btn", type: "button", onmousedown: (e: MouseEvent) => e.preventDefault(), onclick: () => attempt(apply(tag)).then((ok) => ok && closePopover?.()) }, `#${tag}`),
  );
  const box = h(
    "div",
    { class: "popover", role: "dialog", "aria-label": t("action.addTag") },
    h("p", { class: "popover-title" }, tn("tag.for", ids.length)),
    field,
    shortcuts.length ? h("div", { class: "chips popover-chips" }, ...shortcuts) : null,
  );
  document.body.append(box);
  let x: number, y: number;
  if (spot instanceof HTMLElement) {
    const r = spot.getBoundingClientRect();
    [x, y] = [r.left, r.bottom + 4];
  } else {
    [x, y] = [spot.x, spot.y];
  }
  const b = box.getBoundingClientRect();
  box.style.left = `${Math.max(8, Math.min(x, innerWidth - b.width - 8))}px`;
  box.style.top = `${Math.max(8, Math.min(y, innerHeight - b.height - 8))}px`;

  const onDown = (e: Event) => {
    if (!box.contains(e.target as Node)) closePopover?.();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      closePopover?.();
    }
  };
  closePopover = () => {
    box.remove();
    removeEventListener("pointerdown", onDown, true);
    removeEventListener("keydown", onKey, true);
    closePopover = null;
  };
  addEventListener("pointerdown", onDown, true);
  addEventListener("keydown", onKey, true);
}

async function copySelected(): Promise<void> {
  const [id] = state.selected;
  if (state.selected.size === 1 && (await attempt(api.copyPrint(id)))) toast(t("copy.done"));
}

function revealSelected(): void {
  const [id] = state.selected;
  if (state.selected.size === 1) attempt(api.revealPrint(id));
}

function selectAll(): void {
  setSelection(new Set(state.prints.map((p) => p.id)));
}

/** The context menu for the current selection; follow-up popups open at `spot`. */
function selectionMenu(spot: { x: number; y: number }): MenuItem[] {
  const prints = selectedPrints();
  const single = prints.length === 1 ? prints[0] : null;
  const fav = favoriteTarget(prints);
  const items: MenuItem[] = [];
  if (single) {
    items.push(
      { label: t("action.open"), icon: "photos", action: () => viewer.open(single.id) },
      { label: t("action.copy"), icon: "copy", action: copySelected },
      { label: t("action.reveal"), icon: "reveal", action: revealSelected },
      "-",
    );
  }
  items.push(
    { label: fav ? t("action.favorite") : t("action.unfavorite"), icon: "star", action: toggleFavorite },
    { label: t("action.moveToAlbumMenu"), icon: "folder", action: () => albumMenu(spot) },
    { label: t("action.addTagMenu"), icon: "hash", action: () => tagPopover(spot) },
    "-",
    { label: single ? t("action.delete") : t("action.deleteCount", { count: prints.length }), icon: "trash", danger: true, action: deleteSelected },
  );
  return items;
}

function renderSelbar(): void {
  const prints = selectedPrints();
  if (!prints.length) {
    selbar.replaceChildren();
    selbar.hidden = true;
    return;
  }
  selbar.hidden = false;
  const single = prints.length === 1;
  const fav = favoriteTarget(prints);
  const all = prints.length === state.prints.length;
  const action = (
    name: IconName,
    label: string,
    run: (b: HTMLButtonElement) => void,
    opts: { filled?: boolean; danger?: boolean; hint?: string } = {},
  ) => {
    const b: HTMLButtonElement = h(
      "button",
      { class: `btn btn-ghost selbar-action${opts.danger ? " danger" : ""}`, type: "button", title: opts.hint ?? label, onclick: () => run(b) },
      icon(name, opts.filled),
      h("span", { class: "selbar-label" }, label),
    );
    return b;
  };
  const children = [
    h(
      "button",
      { class: "btn btn-ghost selbar-close", type: "button", title: t("selection.clearHint"), "aria-label": t("selection.clear"), onclick: () => setSelection(new Set()) },
      icon("close"),
    ),
    h("span", { class: "selbar-count", role: "status" }, countLabel(prints.length)),
    all ? null : h("button", { class: "btn btn-ghost selbar-all", type: "button", title: t("selection.allHint"), onclick: selectAll }, t("selection.all")),
    h("span", { class: "selbar-spacer" }),
    action("star", fav ? t("action.favorite") : t("action.unfavorite"), toggleFavorite, { filled: !fav }),
    action("folder", t("action.moveToAlbumShort"), (b) => albumMenu(b), { hint: t("action.moveToAlbum") }),
    action("hash", t("action.tagShort"), (b) => tagPopover(b), { hint: t("action.addTag") }),
    single ? action("copy", t("action.copy"), copySelected) : null,
    single ? action("reveal", t("action.revealShort"), revealSelected, { hint: t("action.reveal") }) : null,
    action("trash", t("action.delete"), deleteSelected, { danger: true, hint: t("action.trashHint") }),
  ];
  selbar.replaceChildren(...children.filter((c) => c !== null));
}

// ---- drag rectangle ------------------------------------------------------------

/** Pixels the pointer must travel before a press on empty space becomes a drag rectangle. */
const DRAG_START = 4;
/** Distance from the top/bottom edge where the list starts scrolling, and its top speed (px/frame). */
const EDGE = 48;
const MAX_SPEED = 18;

let marquee: {
  pointer: number;
  /** Start point in content coordinates (scroll included). */
  x0: number;
  y0: number;
  clientX: number;
  clientY: number;
  base: Set<number>;
  tiles: { id: number; rect: Rect }[];
  box: HTMLElement | null;
  frame: number;
} | null = null;

/** Converts a viewport point to content coordinates, which stay put while the list scrolls. */
function toContent(clientX: number, clientY: number): { x: number; y: number } {
  const r = content.getBoundingClientRect();
  return { x: clientX - r.left + content.scrollLeft, y: clientY - r.top + content.scrollTop };
}

function tileRects(): { id: number; rect: Rect }[] {
  const r = content.getBoundingClientRect();
  return [...grid.querySelectorAll<HTMLElement>(".tile")].map((el) => {
    const t = el.getBoundingClientRect();
    const left = t.left - r.left + content.scrollLeft;
    const top = t.top - r.top + content.scrollTop;
    return { id: Number(el.dataset.id), rect: { left, top, right: left + t.width, bottom: top + t.height } };
  });
}

function updateMarquee(): void {
  const m = marquee;
  if (!m) return;
  const p = toContent(m.clientX, m.clientY);
  // The box stays inside the scrollable area.
  const x = Math.max(0, Math.min(p.x, content.clientWidth));
  const y = Math.max(0, Math.min(p.y, content.scrollHeight));
  const box = rectFrom(m.x0, m.y0, x, y);
  if (!m.box) {
    m.box = h("div", { class: "marquee", "aria-hidden": "true" });
    content.append(m.box);
  }
  Object.assign(m.box.style, {
    left: `${box.left}px`,
    top: `${box.top}px`,
    width: `${box.right - box.left}px`,
    height: `${box.bottom - box.top}px`,
  });
  const next = marqueeSelect(m.base, m.tiles, box);
  if (!sameSet(next, state.selected)) setSelection(next);
}

/** Scrolls while the pointer sits near the top or bottom edge, faster the closer it is. */
function autoScroll(): void {
  const m = marquee;
  if (!m?.box) return;
  const r = content.getBoundingClientRect();
  const y = m.clientY;
  let speed = 0;
  if (y < r.top + EDGE) speed = -((r.top + EDGE - y) / EDGE) * MAX_SPEED;
  else if (y > r.bottom - EDGE) speed = ((y - (r.bottom - EDGE)) / EDGE) * MAX_SPEED;
  speed = Math.round(Math.max(-MAX_SPEED, Math.min(MAX_SPEED, speed)));
  if (speed) {
    const before = content.scrollTop;
    content.scrollTop += speed;
    if (content.scrollTop !== before) updateMarquee();
  }
  m.frame = requestAnimationFrame(autoScroll);
}

content.addEventListener("pointerdown", (e) => {
  if (e.button !== 0 || viewer.isOpen) return;
  const target = e.target as HTMLElement;
  if (target.closest(".tile, button, input, a")) return;
  // A press on the scrollbar is not a press on the grid.
  if (e.clientX >= content.getBoundingClientRect().left + content.clientWidth) return;
  const p = toContent(e.clientX, e.clientY);
  marquee = {
    pointer: e.pointerId,
    x0: p.x,
    y0: p.y,
    clientX: e.clientX,
    clientY: e.clientY,
    base: e.ctrlKey || e.shiftKey ? new Set(state.selected) : new Set(),
    tiles: [],
    box: null,
    frame: 0,
  };
});

content.addEventListener("pointermove", (e) => {
  const m = marquee;
  if (!m || e.pointerId !== m.pointer) return;
  m.clientX = e.clientX;
  m.clientY = e.clientY;
  if (!m.box) {
    const p = toContent(e.clientX, e.clientY);
    if (Math.hypot(p.x - m.x0, p.y - m.y0) < DRAG_START) return;
    // Keeps the drag going when the pointer leaves the list (or the window).
    try {
      content.setPointerCapture(e.pointerId);
    } catch {
      // Not an active pointer (e.g. a synthetic event): the drag still works inside the list.
    }
    document.body.classList.add("marquee-active");
    m.tiles = tileRects();
    updateMarquee();
    m.frame = requestAnimationFrame(autoScroll);
    return;
  }
  updateMarquee();
});

function endMarquee(e: PointerEvent): void {
  const m = marquee;
  if (!m || e.pointerId !== m.pointer) return;
  marquee = null;
  if (!m.box) return;
  cancelAnimationFrame(m.frame);
  m.box.remove();
  document.body.classList.remove("marquee-active");
  if (content.hasPointerCapture(e.pointerId)) content.releasePointerCapture(e.pointerId);
  // The click that ends a drag must not clear what it just selected.
  swallowClick = true;
  setTimeout(() => (swallowClick = false), 0);
}
content.addEventListener("pointerup", endMarquee);
content.addEventListener("pointercancel", endMarquee);

// ---- toolbar -----------------------------------------------------------------

let searchTimer: number | undefined;
search.addEventListener("input", () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.search = search.value;
    reload();
  }, 200);
});

function paintSort(): void {
  const newest = state.order === "newest";
  sortButton.textContent = newest ? t("sort.newest") : t("sort.oldest");
  sortButton.title = newest ? t("sort.newestHint") : t("sort.oldestHint");
}
sortButton.addEventListener("click", () => {
  state.order = state.order === "newest" ? "oldest" : "newest";
  paintSort();
  reload();
});

// ---- keyboard --------------------------------------------------------------------

async function deleteSelected(): Promise<void> {
  const ids = [...state.selected];
  if (!ids.length) return;
  const ok = await confirmDialog({ title: tn("trash.confirm", ids.length), confirm: t("trash.confirmButton"), danger: true });
  if (ok && (await attempt(api.deletePrints(ids)))) state.selected.clear();
}

addEventListener("keydown", (e) => {
  if (document.querySelector("dialog[open], .menu, .popover")) return;
  if (viewer.isOpen) {
    if (viewer.onKey(e)) e.preventDefault();
    return;
  }
  if (e.target instanceof HTMLInputElement) {
    if (e.key === "Escape" && e.target === search && search.value) {
      search.value = "";
      search.dispatchEvent(new Event("input"));
    }
    return;
  }
  if (e.key === "Delete") deleteSelected();
  else if (e.key === "Enter") {
    const id = state.anchor !== null && state.selected.has(state.anchor) ? state.anchor : [...state.selected][0];
    if (id !== undefined) viewer.open(id);
  } else if (e.key === "Escape") {
    setSelection(new Set());
  } else if (e.key === "a" && e.ctrlKey) {
    e.preventDefault();
    selectAll();
  } else if (e.key === "f" && e.ctrlKey) {
    e.preventDefault();
    search.focus();
    search.select();
  }
});

// The webview's own menu (Reload, Inspect…) makes no sense in an app window.
addEventListener("contextmenu", (e) => {
  if (!(e.target instanceof HTMLInputElement)) e.preventDefault();
});

// ---- boot --------------------------------------------------------------------

$<HTMLElement>("#search-icon").replaceWith(icon("search"));
paintSort();
api.onLibraryChanged(scheduleReload);
reload();
