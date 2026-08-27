use std::fs::File;
use std::path::{Path, PathBuf};

use imgcull::discovery::discover_images;
use tempfile::TempDir;

fn touch(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    File::create(&path).expect("failed to create test file");
    path
}

#[test]
fn discovers_jpeg_files() {
    let dir = TempDir::new().unwrap();
    touch(&dir, "a.jpg");
    touch(&dir, "b.jpeg");

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|p| p.ends_with("a.jpg")));
    assert!(found.iter().any(|p| p.ends_with("b.jpeg")));
}

#[test]
fn discovers_raw_files() {
    let dir = TempDir::new().unwrap();
    let raw_exts = [
        "arw", "cr2", "cr3", "dng", "erf", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf",
        "rw2", "sr2", "srw",
    ];
    for ext in raw_exts {
        touch(&dir, &format!("sample.{ext}"));
    }

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), raw_exts.len());
}

#[test]
fn case_insensitive_extensions() {
    let dir = TempDir::new().unwrap();
    touch(&dir, "upper.JPG");
    touch(&dir, "raw.CR2");

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 2);
}

#[test]
fn skips_unsupported_formats() {
    let dir = TempDir::new().unwrap();
    touch(&dir, "photo.jpg");
    touch(&dir, "icon.png");
    touch(&dir, "anim.webp");

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("photo.jpg"));
}

#[test]
fn handles_individual_file_paths() {
    let dir = TempDir::new().unwrap();
    let supported = touch(&dir, "good.jpg");
    let unsupported = touch(&dir, "bad.png");

    // Supported file is included
    let found = discover_images(&[supported]);
    assert_eq!(found.len(), 1);

    // Unsupported file is excluded (with a warning)
    let found = discover_images(&[unsupported]);
    assert!(found.is_empty());
}

#[test]
fn is_supported_checks_extension() {
    use imgcull::discovery::is_supported;
    assert!(is_supported(Path::new("photo.jpg")));
    assert!(is_supported(Path::new("photo.JPEG")));
    assert!(is_supported(Path::new("raw.DNG")));
    assert!(!is_supported(Path::new("icon.png")));
    assert!(!is_supported(Path::new("noext")));
}

#[test]
fn recurses_into_nested_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("level1").join("level2");
    std::fs::create_dir_all(&nested).unwrap();
    touch(&dir, "top.jpg");
    File::create(nested.join("deep.nef")).unwrap();

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|p| p.ends_with("top.jpg")));
    assert!(found.iter().any(|p| p.ends_with("deep.nef")));
}

#[cfg(unix)]
#[test]
fn follows_symlinked_directories() {
    // The scan trusts the readdir file type for regular entries and falls
    // back to the link-following Path checks for symlinks — a symlinked
    // subdirectory must still be scanned.
    let real = TempDir::new().unwrap();
    touch(&real, "linked.jpg");
    let root = TempDir::new().unwrap();
    std::os::unix::fs::symlink(real.path(), root.path().join("alias")).unwrap();

    let found = discover_images(&[root.path().to_path_buf()]);
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("linked.jpg"));
}

#[cfg(unix)]
#[test]
fn discovers_symlinked_files() {
    let real = TempDir::new().unwrap();
    let target = touch(&real, "target.jpg");
    let root = TempDir::new().unwrap();
    std::os::unix::fs::symlink(&target, root.path().join("link.jpg")).unwrap();

    let found = discover_images(&[root.path().to_path_buf()]);
    assert_eq!(found.len(), 1);
    assert!(found[0].ends_with("link.jpg"));
}

#[test]
fn results_are_sorted() {
    let dir = TempDir::new().unwrap();
    touch(&dir, "c.jpg");
    touch(&dir, "a.jpg");
    touch(&dir, "b.jpg");

    let found = discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(found.len(), 3);
    let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap()).collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]));
}
