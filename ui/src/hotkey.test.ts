import { expect, test } from "vitest";
import { accelerator, heldModifiers, isModifierKey } from "./hotkey";

const ev = (code: string, key: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  key,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

test("builds parse_accelerator strings in Ctrl, Alt, Shift, Win order", () => {
  expect(accelerator(ev("KeyS", "S", { shiftKey: true, ctrlKey: true }))).toBe("Ctrl+Shift+S");
  expect(accelerator(ev("KeyP", "p", { altKey: true, ctrlKey: true }))).toBe("Ctrl+Alt+P");
  expect(accelerator(ev("Digit4", "$", { metaKey: true, shiftKey: true }))).toBe("Shift+Win+4");
  expect(accelerator(ev("F12", "F12"))).toBe("F12");
  expect(accelerator(ev("PrintScreen", "PrintScreen", { altKey: true }))).toBe("Alt+PrintScreen");
});

test("lone modifiers and unsupported keys give no accelerator", () => {
  expect(accelerator(ev("ControlLeft", "Control", { ctrlKey: true }))).toBeNull();
  expect(accelerator(ev("ShiftRight", "Shift", { shiftKey: true }))).toBeNull();
  expect(accelerator(ev("Space", " ", { ctrlKey: true }))).toBeNull();
  expect(accelerator(ev("F25", "F25"))).toBeNull();
  expect(accelerator(ev("Numpad1", "1", { ctrlKey: true }))).toBeNull();
  expect(isModifierKey("Meta")).toBe(true);
  expect(isModifierKey("a")).toBe(false);
});

test("held modifiers are listed while the combination is incomplete", () => {
  expect(heldModifiers(ev("AltLeft", "Alt", { altKey: true, ctrlKey: true }))).toEqual(["Ctrl", "Alt"]);
});
