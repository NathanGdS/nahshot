import { expect, test } from "vitest";
import { friendlyError } from "./api";

test("a hotkey Windows refused gets its own message", () => {
  expect(friendlyError("hotkey error: registering Control+Shift+KeyS: already registered")).toBe(
    "Não foi possível registrar esse atalho; talvez outro programa já o use.",
  );
});

test("an invalid accelerator keeps its message", () => {
  expect(friendlyError('hotkey error: invalid accelerator "Ctrl+": missing key')).toBe(
    "Essa combinação de teclas não pode ser usada.",
  );
});
