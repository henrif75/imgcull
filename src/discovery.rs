//! File discovery for supported image formats.
//!
//! Walks directories and filters files by supported extensions (JPEG and common RAW formats).

use std::fs;
use std::path::{Path, PathBuf};

/// Supported image file extensions (lowercase).
///
/// Single source of truth: [`crate::preprocessing::preprocess_image`] treats
/// `jpg`/`jpeg` as JPEG and every other entry here as a rawler-handled RAW
/// format.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", // JPEG
    "arw", "cr2", "cr3", "dng", "erf", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf",
    "rw2", "sr2", "srw", // RAW formats handled via rawler
];

/// Returns `true` when `path`'s extension matches any of `extensions`
/// case-insensitively.  Returns `false` when the path has no extension or
/// the extension is not valid UTF-8.
pub fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| extensions.iter().any(|&s| s.eq_ignore_ascii_case(ext)))
}

/// Returns `true` if the file at `path` has a supported image extension (case-insensitive).
pub fn is_supported(path: &Path) -> bool {
    has_extension(path, SUPPORTED_EXTENSIONS)
}

/// Discover image files from the given paths.
///
/// Directories are scanned recursively for supported files. Individual file paths are
/// checked directly against the supported extensions. Unsupported or unreadable paths
/// produce a warning via `tracing::warn`. Results are sorted lexicographically.
pub fn discover_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut results = Vec::new();

    for path in paths {
        if path.is_dir() {
            scan_dir(path, &mut results);
        } else if path.is_file() {
            if is_supported(path) {
                results.push(path.clone());
            } else {
                tracing::warn!("skipping unsupported file: {}", path.display());
            }
        } else {
            tracing::warn!("skipping unreadable path: {}", path.display());
        }
    }

    results.sort();
    results
}

/// Recursively scan a directory, collecting supported image files.
fn scan_dir(dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("cannot read directory {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("error reading directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();
        // `DirEntry::file_type` comes straight from the readdir entry on most
        // platforms, avoiding a stat syscall per file.  Symlinks (and the rare
        // error) fall back to the stat-based `Path::is_dir`, which follows
        // links like the original behaviour.
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => scan_dir(&path, results),
            Ok(ft) if !ft.is_symlink() => {
                if is_supported(&path) {
                    results.push(path);
                }
            }
            _ => {
                if path.is_dir() {
                    scan_dir(&path, results);
                } else if is_supported(&path) {
                    results.push(path);
                }
            }
        }
    }
}
