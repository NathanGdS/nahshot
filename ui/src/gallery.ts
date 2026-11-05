/** Gallery window: toolbar, day-grouped thumbnail grid with selection, sidebar and viewer. */

import "./theme.css";
import * as api from "./api";
import type { Filter, Print, Sidebar } from "./api";
import { groupByDay, type Order } from "./group";
import { icon } from "./icons";
import { DRAG_TYPE, SidebarView } from "./sidebar";
import { applyTheme } from "./theme";
import { attempt, confirmDialog, h, toast } from "./ui";
import { Viewer } from "./viewer";

applyTheme();

const state = {
  filter: { kind: "all" } as Filter,
  search: "",
  order: "newest" as Order,
  /** Prints in display order (the grid's reading order). */
  prints: [] as Print[],
  sidebar: { albums: [], tags: [], total: 0, favorites: 0 } as Sidebar,
  selected: new Set<number>(),
  /** Last plainly clicked print: the fixed end of a Shift range. */
  anchor: null as number | null,
};

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T;
const grid = $<HTMLElement>("#grid");
const content = $<HTMLElement>("#content");
const title = $<HTMLElement>("#title");
const search = $<HTMLInputElement>("#search");
const sortButton = $<HTMLButtonElement>("#sort");

const sidebar = new SidebarView($("#sidebar"), {
  filter: () => state.filter,
  select: (filter) => {
    state.filter = filter;
    state.selected.clear();
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
    toast(`Não foi possível carregar a galeria: ${api.friendlyError(e)}`, "error");
    return [null, null] as const;
  });
  if (seq !== loadSeq || !prints || !side) return;

  // A filter whose album or tag vanished falls back to "Todos".
  const f = state.filter;
  if ((f.kind === "album" && !side.albums.includes(f.value)) || (f.kind === "tag" && !side.tags.includes(f.value))) {
    state.filter = { kind: "all" };
    return reload();
  }

  state.sidebar = side;
  const groups = groupByDay(prints, new Date(), state.order);
  state.prints = groups.flatMap((g) => g.items);
  const ids = new Set(state.prints.map((p) => p.id));
  for (const id of state.selected) if (!ids.has(id)) state.selected.delete(id);

  sidebar.update(side);
  renderGrid(groups);
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

const filterTitle = (f: Filter) =>
  f.kind === "all" ? "Todos" : f.kind === "favorites" ? "Favoritos" : f.kind === "album" ? f.value : `#${f.value}`;

function renderGrid(groups: { label: string; items: Print[] }[]): void {
  const scroll = content.scrollTop;
  title.textContent = filterTitle(state.filter);
  const count = state.prints.length;
  $<HTMLElement>("#subtitle").textContent = count === 1 ? "1 print" : `${count} prints`;

  if (!groups.length) {
    grid.replaceChildren(emptyState());
    return;
  }
  grid.replaceChildren(
    ...groups.map((g) =>
      h(
        "section",
        { class: "day" },
        h("h2", { class: "day-label" }, g.label, h("span", { class: "day-count" }, String(g.items.length))),
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
    h("h2", {}, filtered ? "Nada encontrado" : "Nenhum print ainda"),
    h(
      "p",
      {},
      filtered
        ? "Tente outra busca ou escolha outro filtro na barra lateral."
        : "Use o atalho de captura ou o menu da bandeja para tirar o primeiro print.",
    ),
  );
}

function tile(p: Print): HTMLElement {
  const badges = h(
    "div",
    { class: "tile-badges" },
    p.favorite ? h("span", { class: "badge badge-star", title: "Favorito" }, icon("star", true)) : null,
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
    h("span", { class: "tile-check", "aria-hidden": "true" }, icon("check")),
    badges,
    h("figcaption", { class: "tile-name" }, p.fileName),
  );
  return el;
}

function paintSelection(): void {
  for (const el of grid.querySelectorAll<HTMLElement>(".tile")) {
    el.classList.toggle("selected", state.selected.has(Number(el.dataset.id)));
  }
}

const tileId = (e: Event) => {
  const el = (e.target as HTMLElement).closest<HTMLElement>(".tile");
  return el ? Number(el.dataset.id) : null;
};

grid.addEventListener("click", (e) => {
  const id = tileId(e);
  if (id === null) {
    if (!e.ctrlKey && !e.shiftKey) state.selected.clear();
    return paintSelection();
  }
  if (e.shiftKey && state.anchor !== null) {
    const order = state.prints.map((p) => p.id);
    const [a, b] = [order.indexOf(state.anchor), order.indexOf(id)].sort((x, y) => x - y);
    if (!e.ctrlKey) state.selected.clear();
    for (const sid of order.slice(Math.max(a, 0), b + 1)) state.selected.add(sid);
  } else if (e.ctrlKey) {
    if (!state.selected.delete(id)) state.selected.add(id);
    state.anchor = id;
  } else {
    state.selected = new Set([id]);
    state.anchor = id;
  }
  paintSelection();
});

grid.addEventListener("dblclick", (e) => {
  const id = tileId(e);
  if (id !== null) viewer.open(id);
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
  const ghost = h("div", { class: "drag-ghost" }, icon("photos"), ids.length === 1 ? "1 print" : `${ids.length} prints`);
  document.body.append(ghost);
  e.dataTransfer.setDragImage(ghost, 16, 16);
  requestAnimationFrame(() => ghost.remove());
});

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
  sortButton.textContent = newest ? "Mais novo ↓" : "Mais velho ↑";
  sortButton.title = newest ? "Mostrando do mais novo pro mais velho" : "Mostrando do mais velho pro mais novo";
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
  const ok = await confirmDialog({ title: `Mover ${ids.length} print(s) para a Lixeira?`, confirm: "Mover para a Lixeira", danger: true });
  if (ok && (await attempt(api.deletePrints(ids)))) state.selected.clear();
}

addEventListener("keydown", (e) => {
  if (document.querySelector("dialog[open], .menu")) return;
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
    state.selected.clear();
    paintSelection();
  } else if (e.key === "a" && e.ctrlKey) {
    e.preventDefault();
    state.selected = new Set(state.prints.map((p) => p.id));
    paintSelection();
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
