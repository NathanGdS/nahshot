/** Typed wrappers around the app's Tauri commands, one per command. */

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** A print with its thumbnail, as `list_prints` returns it. */
export interface Print {
  id: number;
  path: string;
  fileName: string;
  /** Local capture time, ISO `YYYY-MM-DDTHH:MM:SS`. */
  capturedAt: string;
  favorite: boolean;
  album: string | null;
  tags: string[];
  /** Cached thumbnail file; `null` until `ensureThumbs` has built it. */
  thumb: string | null;
}

/** A thumbnail built by `ensure_thumbs`. */
export interface Thumb {
  id: number;
  thumb: string;
}

/** Which prints a query selects. */
export type Filter =
  | { kind: "all" }
  | { kind: "favorites" }
  | { kind: "album"; value: string }
  | { kind: "tag"; value: string };

export type SortOrder = "newestFirst" | "oldestFirst";

export interface Query {
  filter: Filter;
  search: string | null;
  order: SortOrder;
}

export interface Sidebar {
  albums: string[];
  tags: string[];
  total: number;
  favorites: number;
}

export const listPrints = (query: Query) => invoke<Print[]>("list_prints", { query });
export const ensureThumbs = (ids: number[]) => invoke<Thumb[]>("ensure_thumbs", { ids });
export const getSidebar = () => invoke<Sidebar>("get_sidebar");
export const renamePrint = (id: number, stem: string) => invoke<Print>("rename_print", { id, stem });
export const setFavorite = (id: number, value: boolean) => invoke<void>("set_favorite", { id, value });
export const addTag = (id: number, tag: string) => invoke<void>("add_tag", { id, tag });
export const removeTag = (id: number, tag: string) => invoke<void>("remove_tag", { id, tag });
export const createAlbum = (name: string) => invoke<string>("create_album", { name });
export const renameAlbum = (old: string, newName: string) => invoke<string>("rename_album", { old, new: newName });
export const deleteAlbum = (name: string) => invoke<void>("delete_album", { name });
export const moveToAlbum = (ids: number[], album: string) => invoke<void>("move_to_album", { ids, album });
export const removeFromAlbum = (ids: number[]) => invoke<void>("remove_from_album", { ids });
export const deletePrints = (ids: number[]) => invoke<void>("delete_prints", { ids });
export const copyPrint = (id: number) => invoke<void>("copy_print", { id });
export const revealPrint = (id: number) => invoke<void>("reveal_print", { id });
export const openSettings = () => invoke<void>("open_settings");

export type Theme = "system" | "light" | "dark";

/** The user's settings, as `get_settings` returns them. */
export interface Settings {
  printsDir: string;
  autostart: boolean;
  defaultPrintScreen: boolean;
  altHotkey: string;
  theme: Theme;
  firstRunDone: boolean;
}

/** What moving the prints folder did. */
export interface ChangeDirReport {
  moved: number;
  failed: number;
}

export const getSettings = () => invoke<Settings>("get_settings");
export const setAutostart = (enabled: boolean) => invoke<void>("set_autostart", { enabled });
export const setDefaultPrintScreen = (enabled: boolean) => invoke<void>("set_default_print_screen", { enabled });
export const setAltHotkey = (accelerator: string) => invoke<string>("set_alt_hotkey", { accelerator });
export const setTheme = (theme: Theme) => invoke<void>("set_theme", { theme });
export const pickPrintsDir = () => invoke<string | null>("pick_prints_dir");
export const changePrintsDir = (path: string, moveExisting: boolean) =>
  invoke<ChangeDirReport>("change_prints_dir", { new: path, moveExisting });
export const finishWelcome = (enableDefault: boolean) => invoke<void>("finish_welcome", { enableDefault });

/** Calls `cb` with the new settings whenever any window changes them. */
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));

/** Calls `cb` whenever the library changes (capture, edit, or the folder on disk). */
export const onLibraryChanged = (cb: () => void): Promise<UnlistenFn> => listen("library-changed", cb);

/** Webview URL of a file on disk. */
export const fileSrc = (path: string) => convertFileSrc(path);

const reasons: [string, string][] = [
  ["name is empty", "o nome está vazio"],
  ["longer than 100", "o nome passa de 100 caracteres"],
  ["forbidden character", "tire os caracteres \\ / : * ? \" < > |"],
  ["ends with a dot", "o nome não pode terminar com ponto"],
  ["reserved Windows name", "esse nome é reservado pelo Windows"],
];

/** Turns a command error (a `CoreError` message) into a short pt-BR sentence. */
export function friendlyError(err: unknown): string {
  const msg = String(err);
  if (msg.startsWith("invalid name")) {
    const hit = reasons.find(([en]) => msg.includes(en));
    return hit ? `Nome inválido: ${hit[1]}.` : "Nome inválido.";
  }
  if (msg.startsWith("album already exists")) return "Já existe um álbum com esse nome.";
  if (msg.startsWith("album not found")) return "Esse álbum não existe mais.";
  if (msg.startsWith("print not found")) return "Esse print não existe mais.";
  if (msg.includes("invalid accelerator")) return "Essa combinação de teclas não pode ser usada.";
  if (msg.startsWith("hotkey error")) return "Não foi possível registrar esse atalho; talvez outro programa já o use.";
  return msg;
}
