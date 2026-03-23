//! File discovery for supported image formats.
//!
//! Walks directories and filters files by supported extensions (JPEG and common RAW formats).

use std::fs;
use std::path::{Path, PathBuf};

/// Supported image file extensions (lowercase).
pub const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "cr2", "nef", "arw", "dng", "orf"];

/// Returns `true` if the file at `path` has a supported image extension (case-insensitive).
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            let lower = ext.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
        })
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
        if path.is_dir() {
            scan_dir(&path, results);
        } else if is_supported(&path) {
            results.push(path);
        }
    }
}
