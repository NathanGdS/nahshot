import { expect, test } from "vitest";
import { groupByDay } from "./group";

const now = new Date("2026-11-04T15:00:00");
const ps = [
  { capturedAt: "2026-11-04T09:00:00" },
  { capturedAt: "2026-11-03T23:59:00" },
  { capturedAt: "2026-10-01T10:00:00" },
  { capturedAt: "2026-11-04T14:00:00" },
];

test("labels and newest order", () => {
  const g = groupByDay(ps, now, "newest");
  expect(g.map((x) => x.label)).toEqual(["Hoje", "Ontem", "1 de outubro de 2026"]);
  expect(g[0].items.map((i) => i.capturedAt)).toEqual(["2026-11-04T14:00:00", "2026-11-04T09:00:00"]);
});

test("oldest order reverses groups and items", () => {
  expect(groupByDay(ps, now, "oldest").map((x) => x.label)).toEqual(["1 de outubro de 2026", "Ontem", "Hoje"]);
});
