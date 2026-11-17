/** Pure selection rules for the gallery grid: clicks, day checks, batch favorite and the drag rectangle. */

export interface Selection {
  selected: Set<number>;
  /** Last toggled or plainly clicked print: the fixed end of a Shift range. */
  anchor: number | null;
}

export interface ClickMods {
  ctrl: boolean;
  shift: boolean;
  /** The click landed on the tile's check circle. */
  onCheck: boolean;
}

/**
 * Applies a click on tile `id` (`order` is the grid's reading order).
 *
 * - Shift with an anchor adds the anchor..id range to the selection.
 * - Ctrl, the check circle, or any click while something is selected (selection mode)
 *   toggles just that print.
 * - Otherwise the print becomes the only one selected.
 */
export function clickTile(sel: Selection, order: number[], id: number, mods: ClickMods): Selection {
  if (mods.shift && sel.anchor !== null && order.includes(sel.anchor)) {
    const [a, b] = [order.indexOf(sel.anchor), order.indexOf(id)].sort((x, y) => x - y);
    const selected = new Set(sel.selected);
    for (const sid of order.slice(a, b + 1)) selected.add(sid);
    return { selected, anchor: sel.anchor };
  }
  if (mods.ctrl || mods.onCheck || sel.selected.size > 0) {
    const selected = new Set(sel.selected);
    if (!selected.delete(id)) selected.add(id);
    return { selected, anchor: id };
  }
  return { selected: new Set([id]), anchor: id };
}

export type DayState = "none" | "some" | "all";

/** How much of one day's prints is selected. */
export function dayState(dayIds: number[], selected: Set<number>): DayState {
  const n = dayIds.filter((id) => selected.has(id)).length;
  return n === 0 ? "none" : n === dayIds.length ? "all" : "some";
}

/** Clicking a day's check: selects the whole day, or clears it when it was all selected. */
export function toggleDay(selected: Set<number>, dayIds: number[]): Set<number> {
  const out = new Set(selected);
  if (dayState(dayIds, selected) === "all") for (const id of dayIds) out.delete(id);
  else for (const id of dayIds) out.add(id);
  return out;
}

/** The value "Favoritar" sets: favorite all unless every selected print already is. */
export function favoriteTarget(prints: { favorite: boolean }[]): boolean {
  return !prints.every((p) => p.favorite);
}

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The rectangle spanned by two corner points, whatever direction the drag went. */
export function rectFrom(x1: number, y1: number, x2: number, y2: number): Rect {
  return { left: Math.min(x1, x2), top: Math.min(y1, y2), right: Math.max(x1, x2), bottom: Math.max(y1, y2) };
}

/** Whether two rectangles overlap (touching edges count). */
export function intersects(a: Rect, b: Rect): boolean {
  return a.left <= b.right && b.left <= a.right && a.top <= b.bottom && b.top <= a.bottom;
}

/** Selection while dragging a rectangle: `base` (kept with Ctrl/Shift) plus every tile it touches. */
export function marqueeSelect(base: Set<number>, tiles: { id: number; rect: Rect }[], box: Rect): Set<number> {
  const out = new Set(base);
  for (const t of tiles) if (intersects(t.rect, box)) out.add(t.id);
  return out;
}

/** Whether two sets hold the same ids. */
export function sameSet(a: Set<number>, b: Set<number>): boolean {
  if (a.size !== b.size) return false;
  for (const x of a) if (!b.has(x)) return false;
  return true;
}
