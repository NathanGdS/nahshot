/** Typed wrappers around the app's Tauri commands, one per command. */

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t, type Lang } from "./i18n";

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
export const setFavorite = (ids: number[], value: boolean) => invoke<void>("set_favorite", { ids, value });
export const addTag = (ids: number[], tag: string) => invoke<void>("add_tag", { ids, tag });
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

/** `"system"` follows Windows' display language. */
export type Language = "system" | Lang;

/** The user's settings, as `get_settings` returns them. */
export interface Settings {
  printsDir: string;
  autostart: boolean;
  defaultPrintScreen: boolean;
  altHotkey: string;
  theme: Theme;
  language: Language;
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
/** Saves the language; the app then reloads every window in it. */
export const setLanguage = (language: Language) => invoke<void>("set_language", { language });
export const pickPrintsDir = () => invoke<string | null>("pick_prints_dir");
export const changePrintsDir = (path: string, moveExisting: boolean) =>
  invoke<ChangeDirReport>("change_prints_dir", { new: path, moveExisting });
export const finishWelcome = (enableDefault: boolean) => invoke<void>("finish_welcome", { enableDefault });

/** What `check_update` found. */
export interface UpdateInfo {
  current: string;
  /** Newer version on GitHub; `null` when this one is the latest. */
  latest: string | null;
  /** A debug build: it can check, but never installs. */
  devBuild: boolean;
}

export const checkUpdate = () => invoke<UpdateInfo>("check_update");
/** Downloads and runs the installer; on success nahshot closes and reopens. */
export const installUpdate = () => invoke<void>("install_update");

/** Calls `cb` with the download progress (0–100, `null` if the size is unknown). */
export const onUpdateProgress = (cb: (pct: number | null) => void): Promise<UnlistenFn> =>
  listen<number | null>("update-progress", (e) => cb(e.payload));

/** Calls `cb` with the new settings whenever any window changes them. */
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));

/** Calls `cb` whenever the library changes (capture, edit, or the folder on disk). */
export const onLibraryChanged = (cb: () => void): Promise<UnlistenFn> => listen("library-changed", cb);

/** Webview URL of a file on disk. */
export const fileSrc = (path: string) => convertFileSrc(path);

/** `CoreError` "invalid name" reasons and the key of their explanation. */
const reasons: [string, string][] = [
  ["name is empty", "error.reason.empty"],
  ["longer than 100", "error.reason.tooLong"],
  ["forbidden character", "error.reason.forbidden"],
  ["ends with a dot", "error.reason.trailingDot"],
  ["reserved Windows name", "error.reason.reserved"],
];

/** Turns a command error (a `CoreError` message) into a short sentence in the UI language. */
export function friendlyError(err: unknown): string {
  const msg = String(err);
  if (msg.startsWith("invalid name")) {
    const hit = reasons.find(([en]) => msg.includes(en));
    return hit ? t("error.invalidNameReason", { reason: t(hit[1]) }) : t("error.invalidName");
  }
  if (msg.startsWith("album already exists")) return t("error.albumExists");
  if (msg.startsWith("album not found")) return t("error.albumNotFound");
  if (msg.startsWith("print not found")) return t("error.printNotFound");
  if (msg.includes("invalid accelerator")) return t("error.invalidHotkey");
  if (msg.startsWith("hotkey error")) return t("error.hotkeyTaken");
  return msg;
}
