// Placeholder build until the Vite UI lands: copies the HTML pages into dist/.
import { copyFileSync, mkdirSync, readdirSync } from "node:fs";

const here = new URL(".", import.meta.url);
const dist = new URL("dist/", here);
mkdirSync(dist, { recursive: true });
for (const f of readdirSync(here).filter((f) => f.endsWith(".html"))) {
  copyFileSync(new URL(f, here), new URL(f, dist));
}
