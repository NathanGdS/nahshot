import { describe, expect, it } from "vitest";
import { clickTile, dayState, favoriteTarget, intersects, marqueeSelect, rectFrom, sameSet, toggleDay, type Selection } from "./selection";

const order = [1, 2, 3, 4, 5];
const none = { ctrl: false, shift: false, onCheck: false };
const sel = (ids: number[], anchor: number | null = null): Selection => ({ selected: new Set(ids), anchor });
const ids = (s: Selection | Set<number>) => [...(s instanceof Set ? s : s.selected)].sort((a, b) => a - b);

describe("clickTile", () => {
  it("plain click with nothing selected selects only that print", () => {
    const out = clickTile(sel([]), order, 3, none);
    expect(ids(out)).toEqual([3]);
    expect(out.anchor).toBe(3);
  });

  it("check circle toggles without touching the others", () => {
    const on = clickTile(sel([]), order, 2, { ...none, onCheck: true });
    expect(ids(on)).toEqual([2]);
    const more = clickTile(on, order, 4, { ...none, onCheck: true });
    expect(ids(more)).toEqual([2, 4]);
    const off = clickTile(more, order, 2, { ...none, onCheck: true });
    expect(ids(off)).toEqual([4]);
  });

  it("plain click in selection mode toggles", () => {
    expect(ids(clickTile(sel([1], 1), order, 3, none))).toEqual([1, 3]);
    expect(ids(clickTile(sel([1, 3], 3), order, 3, none))).toEqual([1]);
  });

  it("ctrl click toggles", () => {
    expect(ids(clickTile(sel([]), order, 3, { ...none, ctrl: true }))).toEqual([3]);
    expect(ids(clickTile(sel([3], 3), order, 3, { ...none, ctrl: true }))).toEqual([]);
  });

  it("shift click adds the range from the anchor, in either direction", () => {
    expect(ids(clickTile(sel([2], 2), order, 4, { ...none, shift: true }))).toEqual([2, 3, 4]);
    expect(ids(clickTile(sel([4, 5], 4), order, 1, { ...none, shift: true }))).toEqual([1, 2, 3, 4, 5]);
    expect(clickTile(sel([2], 2), order, 4, { ...none, shift: true }).anchor).toBe(2);
  });

  it("shift click without a usable anchor acts like a plain click", () => {
    expect(ids(clickTile(sel([]), order, 4, { ...none, shift: true }))).toEqual([4]);
    expect(ids(clickTile(sel([], 99), order, 4, { ...none, shift: true }))).toEqual([4]);
  });
});

describe("day checks", () => {
  it("reports none, some and all", () => {
    expect(dayState([1, 2], new Set())).toBe("none");
    expect(dayState([1, 2], new Set([2, 9]))).toBe("some");
    expect(dayState([1, 2], new Set([1, 2]))).toBe("all");
  });

  it("selects the whole day, or clears it when already full", () => {
    expect(ids(toggleDay(new Set([9, 1]), [1, 2]))).toEqual([1, 2, 9]);
    expect(ids(toggleDay(new Set([9, 1, 2]), [1, 2]))).toEqual([9]);
  });
});

describe("favoriteTarget", () => {
  it("favorites unless every print already is", () => {
    expect(favoriteTarget([{ favorite: true }, { favorite: false }])).toBe(true);
    expect(favoriteTarget([{ favorite: false }])).toBe(true);
    expect(favoriteTarget([{ favorite: true }, { favorite: true }])).toBe(false);
  });
});

describe("drag rectangle", () => {
  const tile = (id: number, left: number, top: number) => ({ id, rect: { left, top, right: left + 100, bottom: top + 60 } });
  const tiles = [tile(1, 0, 0), tile(2, 110, 0), tile(3, 0, 70), tile(4, 110, 70)];

  it("normalises the corners", () => {
    expect(rectFrom(50, 80, 10, 20)).toEqual({ left: 10, top: 20, right: 50, bottom: 80 });
  });

  it("detects overlap, including touching edges", () => {
    const a = { left: 0, top: 0, right: 10, bottom: 10 };
    expect(intersects(a, { left: 10, top: 10, right: 20, bottom: 20 })).toBe(true);
    expect(intersects(a, { left: 11, top: 0, right: 20, bottom: 10 })).toBe(false);
  });

  it("selects every tile the box touches, keeping the base", () => {
    expect(ids(marqueeSelect(new Set(), tiles, rectFrom(105, 65, 50, 30)))).toEqual([1]);
    expect(ids(marqueeSelect(new Set(), tiles, rectFrom(50, 30, 150, 90)))).toEqual([1, 2, 3, 4]);
    expect(ids(marqueeSelect(new Set([9]), tiles, rectFrom(120, 10, 130, 20)))).toEqual([2, 9]);
    expect(ids(marqueeSelect(new Set(), tiles, rectFrom(102, 62, 108, 68)))).toEqual([]);
  });

  it("compares sets by content", () => {
    expect(sameSet(new Set([1, 2]), new Set([2, 1]))).toBe(true);
    expect(sameSet(new Set([1]), new Set([2]))).toBe(false);
  });
});
