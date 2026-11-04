//! System font used to draw text annotations.

use crate::CaptureError;
use std::io;

const CANDIDATES: [&str; 2] = [
    r"C:\Windows\Fonts\segoeui.ttf",
    r"C:\Windows\Fonts\arial.ttf",
];

/// A loaded font face.
pub struct Font(fontdue::Font);

impl Font {
    /// Loads Segoe UI, falling back to Arial; errors if neither is usable.
    pub fn system() -> Result<Font, CaptureError> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no system font found");
        for path in CANDIDATES {
            match load(path) {
                Ok(font) => return Ok(font),
                Err(e) => last = e,
            }
        }
        Err(last.into())
    }

    pub(crate) fn raw(&self) -> &fontdue::Font {
        &self.0
    }
}

fn load(path: &str) -> io::Result<Font> {
    let bytes = std::fs::read(path)?;
    fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
        .map(Font)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_font_loads_on_dev_machine() {
        assert!(Font::system().is_ok());
    }
}
