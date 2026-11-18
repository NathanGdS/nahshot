import { expect, test } from "vitest";
import { groupByDay, type DayLabels } from "./group";

const now = new Date("2026-11-04T15:00:00");
const ps = [
  { capturedAt: "2026-11-04T09:00:00" },
  { capturedAt: "2026-11-03T23:59:00" },
  { capturedAt: "2026-10-01T10:00:00" },
  { capturedAt: "2026-11-04T14:00:00" },
];
const pt: DayLabels = { today: "Hoje", yesterday: "Ontem", locale: "pt-BR" };
const en: DayLabels = { today: "Today", yesterday: "Yesterday", locale: "en-US" };
const es: DayLabels = { today: "Hoy", yesterday: "Ayer", locale: "es" };

test("labels and newest order", () => {
  const g = groupByDay(ps, now, "newest", pt);
  expect(g.map((x) => x.label)).toEqual(["Hoje", "Ontem", "1 de outubro de 2026"]);
  expect(g[0].items.map((i) => i.capturedAt)).toEqual(["2026-11-04T14:00:00", "2026-11-04T09:00:00"]);
});

test("oldest order reverses groups and items", () => {
  expect(groupByDay(ps, now, "oldest", pt).map((x) => x.label)).toEqual(["1 de outubro de 2026", "Ontem", "Hoje"]);
});

test("labels follow the language", () => {
  expect(groupByDay(ps, now, "newest", en).map((x) => x.label)).toEqual(["Today", "Yesterday", "October 1, 2026"]);
  expect(groupByDay(ps, now, "newest", es).map((x) => x.label)).toEqual(["Hoy", "Ayer", "1 de octubre de 2026"]);
});
