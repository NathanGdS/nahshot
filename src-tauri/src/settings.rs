//! User settings persisted as JSON in the app config directory.

use log::warn;
use nahshot_capture::hotkey::{parse_accelerator, HotkeyConfig};
use nahshot_capture::overlay::OverlayHandle;
use nahshot_capture::registry::{read_snipping, write_snipping, KEYBOARD_KEY};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Alternative capture hotkey used when none (or an invalid one) is stored.
pub const DEFAULT_ALT_HOTKEY: &str = "Ctrl+Shift+S";

/// Colour scheme of the app windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// The serialized name, also used as the windows' `?theme=` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Everything the user can configure. Missing fields take their default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Root folder of the screenshot library.
    pub prints_dir: PathBuf,
    /// Start with Windows.
    pub autostart: bool,
    /// Take Print Screen away from Windows and use it to capture.
    pub default_print_screen: bool,
    /// Extra capture accelerator, e.g. `"Ctrl+Shift+S"`.
    pub alt_hotkey: String,
    pub theme: Theme,
    /// The welcome screen was already shown.
    pub first_run_done: bool,
    /// The Snipping Tool registry value found before nahshot took Print Screen.
    pub saved_snipping_value: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        let pictures = dirs::picture_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Pictures")))
            .unwrap_or_default();
        Settings {
            prints_dir: pictures.join("nahshot"),
            autostart: true,
            default_print_screen: false,
            alt_hotkey: DEFAULT_ALT_HOTKEY.to_owned(),
            theme: Theme::System,
            first_run_done: false,
            saved_snipping_value: None,
        }
    }
}

impl Settings {
    /// Reads `path`; a missing or unreadable file yields the defaults.
    pub fn load(path: &Path) -> Settings {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Settings::default(),
            Err(e) => {
                warn!(
                    "settings {} unreadable, using defaults: {e}",
                    path.display()
                );
                return Settings::default();
            }
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            warn!("settings {} corrupt, using defaults: {e}", path.display());
            Settings::default()
        })
    }

    /// Writes `path` atomically (temporary file, then rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)
    }

    /// The capture triggers these settings ask for, with an invalid stored
    /// accelerator replaced by [`DEFAULT_ALT_HOTKEY`].
    pub fn hotkey_config(&self) -> HotkeyConfig {
        let alt = match parse_accelerator(&self.alt_hotkey) {
            Ok(_) => self.alt_hotkey.clone(),
            Err(e) => {
                warn!("{e}; using {DEFAULT_ALT_HOTKEY}");
                DEFAULT_ALT_HOTKEY.to_owned()
            }
        };
        HotkeyConfig {
            intercept_print_screen: self.default_print_screen,
            alt: Some(alt),
        }
    }
}

/// Turns "use nahshot as the default Print Screen" on or off for the real
/// Windows setting, applying the resulting triggers to `overlay`.
pub fn apply_print_screen_default(
    enable: bool,
    settings: &mut Settings,
    overlay: &OverlayHandle,
) -> io::Result<()> {
    apply_print_screen_default_at(KEYBOARD_KEY, enable, settings, &|cfg| {
        overlay.set_hotkeys(cfg)
    })
}

/// [`apply_print_screen_default`] against `HKCU\<key_path>`. Enabling remembers the
/// Snipping Tool value and clears it; disabling restores what was remembered.
pub fn apply_print_screen_default_at(
    key_path: &str,
    enable: bool,
    settings: &mut Settings,
    set_hotkeys: &dyn Fn(HotkeyConfig),
) -> io::Result<()> {
    match (enable, settings.default_print_screen) {
        (true, false) => {
            settings.saved_snipping_value = read_snipping(key_path)?;
            write_snipping(key_path, Some(0))?;
        }
        (false, true) => {
            write_snipping(key_path, settings.saved_snipping_value)?;
            settings.saved_snipping_value = None;
        }
        // Already in the requested state: the registry holds nahshot's value.
        _ => {}
    }
    settings.default_print_screen = enable;
    set_hotkeys(settings.hotkey_config());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const TEST_ROOT: &str = r"Software\nahshot-app-test";
    const TEST_KEY: &str = r"Software\nahshot-app-test\Keyboard";

    /// Deletes `HKCU\Software\nahshot-app-test` even when an assertion fails.
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            use windows_sys::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
            let root: Vec<u16> = TEST_ROOT.encode_utf16().chain([0]).collect();
            // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
            unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, root.as_ptr()) };
        }
    }

    #[test]
    fn print_screen_default_restores_previous_value() {
        let _cleanup = Cleanup;
        for initial in [Some(1), None] {
            write_snipping(TEST_KEY, initial).unwrap();
            let mut settings = Settings::default();
            let applied = RefCell::new(Vec::new());
            let record = |cfg: HotkeyConfig| applied.borrow_mut().push(cfg);

            apply_print_screen_default_at(TEST_KEY, true, &mut settings, &record).unwrap();
            assert_eq!(read_snipping(TEST_KEY).unwrap(), Some(0));
            assert!(settings.default_print_screen);
            assert_eq!(settings.saved_snipping_value, initial);
            // Enabling twice must not remember nahshot's own `0`.
            apply_print_screen_default_at(TEST_KEY, true, &mut settings, &record).unwrap();
            assert_eq!(settings.saved_snipping_value, initial);

            apply_print_screen_default_at(TEST_KEY, false, &mut settings, &record).unwrap();
            assert_eq!(read_snipping(TEST_KEY).unwrap(), initial);
            assert!(!settings.default_print_screen);
            assert_eq!(settings.saved_snipping_value, None);

            let intercepts: Vec<bool> = applied
                .borrow()
                .iter()
                .map(|c| c.intercept_print_screen)
                .collect();
            assert_eq!(intercepts, [true, true, false]);
            assert!(applied
                .borrow()
                .iter()
                .all(|c| c.alt.as_deref() == Some(DEFAULT_ALT_HOTKEY)));
        }
    }

    fn temp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        (dir, path)
    }

    #[test]
    fn settings_defaults_when_missing() {
        let (_dir, path) = temp();
        let s = Settings::load(&path);
        assert_eq!(s, Settings::default());
        assert!(s.autostart);
        assert!(!s.default_print_screen);
        assert_eq!(s.alt_hotkey, "Ctrl+Shift+S");
        assert_eq!(s.theme, Theme::System);
        assert!(!s.first_run_done);
        assert!(s.prints_dir.ends_with("nahshot"));
    }

    #[test]
    fn settings_defaults_when_corrupt() {
        let (_dir, path) = temp();
        fs::write(&path, "{lixo").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn settings_roundtrip_and_unknown_fields_default() {
        let (_dir, path) = temp();
        fs::write(&path, r#"{"theme":"dark"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!(
            s,
            Settings {
                theme: Theme::Dark,
                ..Settings::default()
            }
        );

        let changed = Settings {
            prints_dir: PathBuf::from(r"D:\shots"),
            autostart: false,
            first_run_done: true,
            saved_snipping_value: Some(1),
            ..s
        };
        changed.save(&path).unwrap();
        assert_eq!(Settings::load(&path), changed);
        let json = fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"firstRunDone\": true"), "{json}");
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn invalid_alt_hotkey_falls_back_to_default() {
        let s = Settings {
            alt_hotkey: "Banana".into(),
            default_print_screen: true,
            ..Settings::default()
        };
        assert_eq!(
            s.hotkey_config(),
            HotkeyConfig {
                intercept_print_screen: true,
                alt: Some(DEFAULT_ALT_HOTKEY.into()),
            }
        );
        let s = Settings {
            alt_hotkey: "Ctrl+Alt+P".into(),
            ..Settings::default()
        };
        assert_eq!(s.hotkey_config().alt.as_deref(), Some("Ctrl+Alt+P"));
    }
}
