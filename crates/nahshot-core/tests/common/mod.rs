//! Helpers shared by the integration tests.

use nahshot_core::library::Library;

/// A library in a fresh temp directory; keep the guard alive for the test.
pub fn lib() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(dir.path().join("root"), &dir.path().join("db.sqlite")).unwrap();
    (dir, lib)
}

/// A tiny valid PNG.
pub fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
