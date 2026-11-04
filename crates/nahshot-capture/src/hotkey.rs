//! Capture triggers: the Print Screen keyboard hook and an alternative global hotkey.

mod print_screen;

use crate::CaptureError;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use log::{error, info};
use print_screen::PrintScreenHook;
use std::sync::Arc;

/// Which keys open the overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HotkeyConfig {
    /// Take Print Screen (without modifiers) away from Windows.
    pub intercept_print_screen: bool,
    /// An extra accelerator such as `"Ctrl+Shift+S"`.
    pub alt: Option<String>,
}

/// The installed capture triggers. Lives on the overlay thread: both the hook and
/// the hotkey's hidden window deliver through that thread's message loop.
pub(crate) struct Hotkeys {
    /// Called by the Print Screen hook; must only post work, never block.
    on_print_screen: Arc<dyn Fn() + Send + Sync>,
    hook: Option<PrintScreenHook>,
    /// Created on first use, since it owns a hidden window.
    manager: Option<GlobalHotKeyManager>,
    alt: Option<HotKey>,
}

impl Hotkeys {
    /// No triggers installed yet; `on_print_screen` runs on each intercepted Print Screen.
    pub(crate) fn new(on_print_screen: impl Fn() + Send + Sync + 'static) -> Self {
        Hotkeys {
            on_print_screen: Arc::new(on_print_screen),
            hook: None,
            manager: None,
            alt: None,
        }
    }

    /// Installs, replaces or removes triggers to match `cfg`. Failures are logged and
    /// leave that trigger off; callers validate `cfg.alt` with [`parse_accelerator`].
    pub(crate) fn apply(&mut self, cfg: &HotkeyConfig) {
        if cfg.intercept_print_screen != self.hook.is_some() {
            self.hook = None;
            if cfg.intercept_print_screen {
                match PrintScreenHook::install(Arc::clone(&self.on_print_screen)) {
                    Ok(hook) => self.hook = Some(hook),
                    Err(e) => error!("print screen hook not installed: {e}"),
                }
            }
        }
        let wanted = cfg.alt.as_deref().and_then(|s| {
            parse_accelerator(s)
                .inspect_err(|e| error!("alternative hotkey ignored: {e}"))
                .ok()
        });
        if wanted != self.alt {
            self.set_alt(wanted);
        }
        info!(
            "hotkeys: print_screen_hook={} alt={:?}",
            self.hook.is_some(),
            self.alt.map(HotKey::into_string)
        );
    }

    fn set_alt(&mut self, wanted: Option<HotKey>) {
        if let (Some(old), Some(manager)) = (self.alt.take(), &self.manager) {
            if let Err(e) = manager.unregister(old) {
                error!("unregistering hotkey {}: {e}", old.into_string());
            }
        }
        let Some(new) = wanted else { return };
        let manager = match &self.manager {
            Some(m) => m,
            None => match GlobalHotKeyManager::new() {
                Ok(m) => self.manager.insert(m),
                Err(e) => return error!("global hotkey manager: {e}"),
            },
        };
        match manager.register(new) {
            Ok(()) => self.alt = Some(new),
            Err(e) => error!("registering hotkey {}: {e}", new.into_string()),
        }
    }

    /// Drains pending hotkey events; `true` if the alternative hotkey was pressed.
    pub(crate) fn poll(&self) -> bool {
        let id = self.alt.map(|h| h.id());
        GlobalHotKeyEvent::receiver()
            .try_iter()
            .filter(|e| Some(e.id()) == id && e.state() == HotKeyState::Pressed)
            .count()
            > 0
    }
}

/// Parses `Ctrl|Alt|Shift|Win` modifiers plus one key (A–Z, 0–9, F1–F24, `PrintScreen`).
///
/// Matching is case-insensitive and ignores spaces around `+`.
pub fn parse_accelerator(s: &str) -> Result<HotKey, CaptureError> {
    let invalid = |why: &str| CaptureError::Hotkey(format!("invalid accelerator {s:?}: {why}"));
    let mut parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let key = parts
        .pop()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| invalid("missing key"))?;
    let mut mods = Modifiers::empty();
    for part in parts {
        let m = modifier(part).ok_or_else(|| invalid("unknown modifier"))?;
        if mods.contains(m) {
            return Err(invalid("repeated modifier"));
        }
        mods |= m;
    }
    let code = key_code(key).ok_or_else(|| invalid("unknown key"))?;
    Ok(HotKey::new((!mods.is_empty()).then_some(mods), code))
}

fn modifier(name: &str) -> Option<Modifiers> {
    match name.to_ascii_lowercase().as_str() {
        "ctrl" => Some(Modifiers::CONTROL),
        "alt" => Some(Modifiers::ALT),
        "shift" => Some(Modifiers::SHIFT),
        "win" => Some(Modifiers::META),
        _ => None,
    }
}

fn key_code(name: &str) -> Option<Code> {
    let upper = name.to_ascii_uppercase();
    // Spell the key as its W3C `Code` name, which `Code` already knows how to parse.
    let code = match upper.as_bytes() {
        [c @ b'A'..=b'Z'] => format!("Key{}", char::from(*c)),
        [c @ b'0'..=b'9'] => format!("Digit{}", char::from(*c)),
        _ if upper == "PRINTSCREEN" => "PrintScreen".to_owned(),
        [b'F', ..] => {
            let n: u8 = upper[1..].parse().ok()?;
            // Only canonical numbers ("F5", not "F05" or "F+5") name a key.
            if !(1..=24).contains(&n) || upper[1..] != n.to_string() {
                return None;
            }
            upper
        }
        _ => return None,
    };
    code.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    #[test]
    fn parse_accelerators() {
        assert!(parse_accelerator("Ctrl+Shift+S").is_ok());
        assert!(parse_accelerator("PrintScreen").is_ok());
        assert!(parse_accelerator("Ctrl+").is_err());
        assert!(parse_accelerator("Banana").is_err());
    }

    #[test]
    fn parsed_accelerator_has_expected_modifiers_and_key() {
        let hk = parse_accelerator("Ctrl+Shift+S").unwrap();
        assert_eq!(
            hk,
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS)
        );
        let hk = parse_accelerator(" win + alt + f24 ").unwrap();
        assert_eq!(
            hk,
            HotKey::new(Some(Modifiers::META | Modifiers::ALT), Code::F24)
        );
        assert_eq!(
            parse_accelerator("7").unwrap(),
            HotKey::new(None, Code::Digit7)
        );
        assert_eq!(
            parse_accelerator("Shift+PrintScreen").unwrap(),
            HotKey::new(Some(Modifiers::SHIFT), Code::PrintScreen)
        );
    }

    #[test]
    fn rejects_malformed_accelerators() {
        for bad in [
            "",
            "Ctrl",
            "Ctrl+Shift",
            "S+Ctrl",
            "Ctrl+S+T",
            "F25",
            "F0",
            "Ctrl++S",
            "Meta+S",
            "Ctrl+Ctrl+S",
            "F01",
            "F+5",
            "Ctrl+Fx",
        ] {
            assert!(
                parse_accelerator(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }
}
