/** Grouping of prints into the gallery's day sections. */

/** Sort direction of the gallery. */
export type Order = "newest" | "oldest";

/** One day section: its heading and the prints taken that day. */
export interface DayGroup<T> {
  label: string;
  items: T[];
}

/** How day headings read: the words for today and yesterday, and the `Intl` locale for other dates. */
export interface DayLabels {
  today: string;
  yesterday: string;
  locale: string;
}

/** `YYYY-MM-DD` of a local date. */
function dayKey(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Today, yesterday or the long date for a `YYYY-MM-DD` key. */
function labelFor(key: string, now: Date, labels: DayLabels, longDate: Intl.DateTimeFormat): string {
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (key === dayKey(now)) return labels.today;
  if (key === dayKey(yesterday)) return labels.yesterday;
  const [y, m, d] = key.split("-").map(Number);
  return longDate.format(new Date(y, m - 1, d));
}

/**
 * Groups prints by the local day of `capturedAt` (ISO `YYYY-MM-DDTHH:MM:SS`),
 * sorting both the groups and the items inside them in `order`.
 */
export function groupByDay<T extends { capturedAt: string }>(
  prints: T[],
  now: Date,
  order: Order,
  labels: DayLabels,
): DayGroup<T>[] {
  const longDate = new Intl.DateTimeFormat(labels.locale, { day: "numeric", month: "long", year: "numeric" });
  const sign = order === "newest" ? -1 : 1;
  const sorted = [...prints].sort((a, b) => sign * a.capturedAt.localeCompare(b.capturedAt));
  const groups = new Map<string, T[]>();
  for (const p of sorted) {
    const key = p.capturedAt.slice(0, 10);
    const items = groups.get(key);
    if (items) items.push(p);
    else groups.set(key, [p]);
  }
  return [...groups].map(([key, items]) => ({ label: labelFor(key, now, labels, longDate), items }));
}
