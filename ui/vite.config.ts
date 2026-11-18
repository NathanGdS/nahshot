import { resolve } from "node:path";
import { defineConfig } from "vite";

/** One entry per webview window; add a page here and it gets built into dist/. */
const pages = ["gallery", "settings", "welcome"];

export default defineConfig({
  clearScreen: false,
  // The texts live in ../locales, shared with the Rust side.
  server: { port: 5173, strictPort: true, fs: { allow: [".."] } },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: Object.fromEntries(pages.map((p) => [p, resolve(import.meta.dirname, `${p}.html`)])),
    },
  },
});
