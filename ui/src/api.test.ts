import { expect, test } from "vitest";
import { friendlyError } from "./api";
import { t } from "./i18n";

test("a hotkey Windows refused gets its own message", () => {
  expect(friendlyError("hotkey error: registering Control+Shift+KeyS: already registered")).toBe(t("error.hotkeyTaken"));
});

test("an invalid accelerator keeps its message", () => {
  expect(friendlyError('hotkey error: invalid accelerator "Ctrl+": missing key')).toBe(t("error.invalidHotkey"));
});
