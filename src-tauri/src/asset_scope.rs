//! Which folders the webview may load images from over the asset protocol.

use nahshot_core::library::Library;
use nahshot_core::model::{Filter, Query, SortOrder};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Allows the thumbnails, the whole library root and every folder outside it that
/// still holds a registered print (left behind by a folder change).
pub fn allow_library(
    app: &AppHandle,
    lib: &Library,
    thumbs_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope = app.asset_protocol_scope();
    scope.allow_directory(thumbs_dir, true)?;
    scope.allow_directory(lib.root(), true)?;
    let all = Query {
        filter: Filter::All,
        search: None,
        order: SortOrder::OldestFirst,
    };
    let prints = lib.query(&all)?;
    for dir in dirs_outside(lib.root(), prints.iter().map(|p| p.path.as_path())) {
        scope.allow_directory(dir, false)?;
    }
    Ok(())
}

/// The distinct parent folders of `paths` that are not inside `root`, sorted.
pub fn dirs_outside<'a>(root: &Path, paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|p| !p.starts_with(root))
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_outside_root_are_distinct_parents() {
        let root = Path::new(r"D:\novo");
        let paths = [
            Path::new(r"D:\novo\2026-11\a.png"),
            Path::new(r"D:\velho\2026-11\b.png"),
            Path::new(r"D:\velho\2026-11\c.png"),
            Path::new(r"D:\velho\Álbuns\Bugs\d.png"),
            Path::new(r"D:\novo-irmao\e.png"),
        ];
        assert_eq!(
            dirs_outside(root, paths),
            vec![
                PathBuf::from(r"D:\novo-irmao"),
                PathBuf::from(r"D:\velho\2026-11"),
                PathBuf::from(r"D:\velho\Álbuns\Bugs"),
            ]
        );
    }

    #[test]
    fn nothing_outside_when_all_under_root() {
        let root = Path::new(r"C:\p");
        assert!(dirs_outside(root, [Path::new(r"C:\p\x\a.png")]).is_empty());
    }
}
