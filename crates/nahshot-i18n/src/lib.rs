//! The app's texts in every supported language, shared with the web UI.
//!
//! The texts live in `locales/<code>.json` at the repository root: flat objects of
//! `"dotted.key": "text"`, with `{name}` placeholders. English is the reference: a key
//! missing from another language falls back to English, and one missing from English
//! shows as the key itself.
//!
//! The current language is process-wide, since the overlay, the toast, the tray and
//! the commands all run on different threads and read it when they draw.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// A supported language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lang {
    #[default]
    En,
    Pt,
    Es,
}

impl Lang {
    /// Every supported language, English first.
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Pt, Lang::Es];

    /// The short code used in settings, URLs and file names: `en`, `pt`, `es`.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pt => "pt",
            Lang::Es => "es",
        }
    }

    /// The language for `code`, or `None` when it is not supported.
    pub fn from_code(code: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    fn source(self) -> &'static str {
        match self {
            Lang::En => include_str!("../../../locales/en.json"),
            Lang::Pt => include_str!("../../../locales/pt.json"),
            Lang::Es => include_str!("../../../locales/es.json"),
        }
    }

    fn index(self) -> u8 {
        match self {
            Lang::En => 0,
            Lang::Pt => 1,
            Lang::Es => 2,
        }
    }
}

type Table = HashMap<String, String>;

/// The parsed texts of each language, in [`Lang::ALL`] order.
fn tables() -> &'static [Table; 3] {
    static TABLES: OnceLock<[Table; 3]> = OnceLock::new();
    TABLES.get_or_init(|| Lang::ALL.map(|l| parse(l.source())))
}

/// Parses a locale file. The files are embedded and checked by the tests, so a broken
/// one is a build bug: it panics in debug builds and leaves the language empty (all
/// English) in release ones.
fn parse(source: &str) -> Table {
    match serde_json::from_str(source) {
        Ok(table) => table,
        Err(e) => {
            debug_assert!(false, "invalid locale file: {e}");
            Table::new()
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Switches the whole process to `lang`.
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

/// The current language.
pub fn lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Pt,
        2 => Lang::Es,
        _ => Lang::En,
    }
}

/// The text for `key` in `lang`, falling back to English and then to the key.
pub fn text_in(lang: Lang, key: &str) -> &str {
    let tables = tables();
    tables[lang.index() as usize]
        .get(key)
        .or_else(|| tables[0].get(key))
        .map_or(key, String::as_str)
}

/// The text for `key` in the current language.
pub fn t(key: &str) -> String {
    text_in(lang(), key).to_owned()
}

/// [`t`] with each `{name}` replaced by its value.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    fill(text_in(lang(), key), args)
}

fn fill(template: &str, args: &[(&str, &str)]) -> String {
    args.iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// The Windows display language, when it is a supported one; otherwise English.
pub fn system_lang() -> Lang {
    lang_for_primary_id(system_primary_lang_id())
}

/// Maps a Windows primary language id (the low 10 bits of a LANGID) to a language.
fn lang_for_primary_id(id: u16) -> Lang {
    const LANG_SPANISH: u16 = 0x0a;
    const LANG_PORTUGUESE: u16 = 0x16;
    match id {
        LANG_PORTUGUESE => Lang::Pt,
        LANG_SPANISH => Lang::Es,
        _ => Lang::En,
    }
}

#[cfg(windows)]
fn system_primary_lang_id() -> u16 {
    // SAFETY: takes no arguments and only reads the user's language setting.
    let langid = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    langid & 0x3ff
}

#[cfg(not(windows))]
fn system_primary_lang_id() -> u16 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_parses_and_has_texts() {
        for l in Lang::ALL {
            let table: Table = serde_json::from_str(l.source()).expect("valid JSON");
            assert!(!table.is_empty(), "{} is empty", l.code());
        }
    }

    #[test]
    fn codes_round_trip() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(Lang::from_code("fr"), None);
        assert_eq!(Lang::from_code("system"), None);
    }

    #[test]
    fn looks_up_each_language() {
        assert_eq!(text_in(Lang::En, "tray.quit"), "Quit");
        assert_eq!(text_in(Lang::Pt, "tray.quit"), "Sair");
        assert_eq!(text_in(Lang::Es, "tray.quit"), "Salir");
    }

    #[test]
    fn unknown_key_shows_the_key() {
        assert_eq!(text_in(Lang::Pt, "no.such.key"), "no.such.key");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(
            fill("{a} and {b}, {a}", &[("a", "x"), ("b", "y")]),
            "x and y, x"
        );
        assert_eq!(fill("no args", &[("a", "x")]), "no args");
    }

    #[test]
    fn current_language_is_global() {
        set_lang(Lang::Es);
        assert_eq!(lang(), Lang::Es);
        assert_eq!(t("tray.quit"), "Salir");
        set_lang(Lang::En);
        assert_eq!(lang(), Lang::En);
    }

    #[test]
    fn maps_windows_language_ids() {
        assert_eq!(lang_for_primary_id(0x16), Lang::Pt);
        assert_eq!(lang_for_primary_id(0x0a), Lang::Es);
        assert_eq!(lang_for_primary_id(0x09), Lang::En);
        assert_eq!(lang_for_primary_id(0x0c), Lang::En);
    }
}
