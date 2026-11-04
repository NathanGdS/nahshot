/** Turns key events into accelerator strings in `parse_accelerator`'s vocabulary. */

/** The parts of a `KeyboardEvent` the conversion needs. */
export interface KeyInput {
  code: string;
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta", "OS", "AltGraph"]);

/** Whether `key` (a `KeyboardEvent.key`) is a modifier pressed on its own. */
export const isModifierKey = (key: string) => MODIFIER_KEYS.has(key);

/** Modifiers held during `e`, in the order and spelling the app stores them. */
export function heldModifiers(e: KeyInput): string[] {
  return [
    e.ctrlKey && "Ctrl",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Win",
  ].filter((m): m is string => !!m);
}

/** The key's accelerator name (A–Z, 0–9, F1–F24, PrintScreen), from its physical code. */
function keyName(code: string): string | null {
  const m = /^(?:Key([A-Z])|Digit([0-9])|(F(?:[1-9]|1[0-9]|2[0-4]))|(PrintScreen))$/.exec(code);
  return m ? (m[1] ?? m[2] ?? m[3] ?? m[4]) : null;
}

/** "Ctrl+Shift+S"-style accelerator for `e`, or `null` for lone modifiers and unsupported keys. */
export function accelerator(e: KeyInput): string | null {
  if (isModifierKey(e.key)) return null;
  const key = keyName(e.code);
  return key ? [...heldModifiers(e), key].join("+") : null;
}
