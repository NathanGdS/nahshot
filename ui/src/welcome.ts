/** First-run welcome: offers to take over Print Screen, then closes itself. */

import "./theme.css";
import "./pages.css";
import { finishWelcome, friendlyError } from "./api";
import { applyTheme } from "./theme";

applyTheme();

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T;
const yes = $<HTMLButtonElement>("#yes");
const later = $<HTMLButtonElement>("#later");
const error = $<HTMLElement>("#error");

/** Sends the answer; the app closes this window once it is saved. */
async function answer(enableDefault: boolean): Promise<void> {
  yes.disabled = later.disabled = true;
  error.textContent = "";
  try {
    await finishWelcome(enableDefault);
  } catch (e) {
    error.textContent = `Não foi possível ativar: ${friendlyError(e)}`;
    yes.disabled = later.disabled = false;
  }
}

yes.addEventListener("click", () => answer(true));
later.addEventListener("click", () => answer(false));
yes.focus();
