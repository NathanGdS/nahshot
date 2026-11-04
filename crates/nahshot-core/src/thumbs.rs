//! On-disk cache of JPEG thumbnails for gallery grids.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::imageops::FilterType;

use crate::Result;

/// Longest side, in pixels, of a generated thumbnail.
const MAX_SIDE: u32 = 320;
/// JPEG quality used for thumbnails.
const JPEG_QUALITY: u8 = 80;

/// Makes temporary file names unique within the process.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A directory of thumbnails keyed by source path, mtime and size.
pub struct ThumbCache {
    dir: PathBuf,
}

impl ThumbCache {
    /// Opens the cache at `dir`, creating the directory if needed.
    pub fn new(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Returns the thumbnail path for `source`, generating it when missing or stale.
    pub fn get_or_create(&self, source: &Path) -> Result<PathBuf> {
        let thumb = self.dir.join(format!("{}.jpg", cache_key(source)?));
        if !thumb.exists() {
            let tmp = self.dir.join(format!(
                "{}.{}.{}.tmp",
                thumb.file_stem().unwrap_or_default().to_string_lossy(),
                std::process::id(),
                TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let written = write_thumb(source, &tmp).and_then(|()| Ok(fs::rename(&tmp, &thumb)?));
            if let Err(err) = written {
                let _ = fs::remove_file(&tmp);
                return Err(err);
            }
        }
        Ok(thumb)
    }

    /// Deletes cached `.jpg` thumbnails not listed in `keep`; returns how many were removed.
    pub fn prune(&self, keep: &HashSet<PathBuf>) -> Result<usize> {
        let mut removed = 0;
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let is_thumb = path.is_file() && path.extension().is_some_and(|ext| ext == "jpg");
            if is_thumb && !keep.contains(&path) {
                fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// Decodes `source`, downsizes it and writes a JPEG thumbnail to `dest`.
fn write_thumb(source: &Path, dest: &Path) -> Result<()> {
    let img = image::open(source)?;
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, FilterType::Triangle)
    } else {
        img
    };
    let file = fs::File::create(dest)?;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        JPEG_QUALITY,
    );
    img.to_rgb8().write_with_encoder(encoder)?;
    Ok(())
}

/// Hex hash of the lowercased absolute path, modification time and size.
fn cache_key(source: &Path) -> Result<String> {
    let meta = fs::metadata(source)?;
    let abs = std::path::absolute(source)?;
    let mut hasher = DefaultHasher::new();
    abs.to_string_lossy().to_lowercase().hash(&mut hasher);
    meta.modified()?.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn write_png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
            .save(path)
            .unwrap();
    }

    fn cache(dir: &tempfile::TempDir) -> ThumbCache {
        ThumbCache::new(dir.path().join("thumbs")).unwrap()
    }

    #[test]
    fn thumb_is_small_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        write_png(&src, 1920, 1080);
        let cache = cache(&dir);

        let first = cache.get_or_create(&src).unwrap();
        assert_eq!(image::image_dimensions(&first).unwrap(), (320, 180));
        let mtime = fs::metadata(&first).unwrap().modified().unwrap();

        sleep(Duration::from_millis(50));
        let second = cache.get_or_create(&src).unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::metadata(&second).unwrap().modified().unwrap(), mtime);
    }

    #[test]
    fn thumb_regenerates_when_source_changes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        write_png(&src, 800, 600);
        let cache = cache(&dir);
        let first = cache.get_or_create(&src).unwrap();

        write_png(&src, 400, 300);
        let second = cache.get_or_create(&src).unwrap();
        assert_ne!(first, second);
        assert_eq!(image::image_dimensions(&second).unwrap(), (320, 240));
    }

    #[test]
    fn small_images_are_not_upscaled() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("small.png");
        write_png(&src, 100, 50);
        let thumb = cache(&dir).get_or_create(&src).unwrap();
        assert_eq!(image::image_dimensions(&thumb).unwrap(), (100, 50));
    }

    #[test]
    fn prune_deletes_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
        write_png(&a, 50, 50);
        write_png(&b, 60, 60);
        let cache = cache(&dir);
        let thumb_a = cache.get_or_create(&a).unwrap();
        let thumb_b = cache.get_or_create(&b).unwrap();

        let keep = HashSet::from([thumb_a.clone()]);
        assert_eq!(cache.prune(&keep).unwrap(), 1);
        assert!(thumb_a.exists());
        assert!(!thumb_b.exists());
    }

    #[test]
    fn prune_ignores_subdirs_and_tmp_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.png");
        write_png(&src, 50, 50);
        let cache = cache(&dir);
        let thumb = cache.get_or_create(&src).unwrap();
        let cache_dir = dir.path().join("thumbs");
        fs::create_dir(cache_dir.join("sub")).unwrap();
        fs::write(cache_dir.join("x.1.0.tmp"), b"partial").unwrap();

        assert_eq!(cache.prune(&HashSet::new()).unwrap(), 1);
        assert!(!thumb.exists());
        assert!(cache_dir.join("sub").is_dir());
        assert!(cache_dir.join("x.1.0.tmp").exists());
    }

    #[test]
    fn failed_encode_leaves_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.png");
        fs::write(&bad, b"not an image").unwrap();
        let cache = cache(&dir);

        assert!(cache.get_or_create(&bad).is_err());
        assert_eq!(fs::read_dir(dir.path().join("thumbs")).unwrap().count(), 0);
    }
}
