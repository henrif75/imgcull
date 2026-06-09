use base64::Engine;
use image::GenericImageView;
use imgcull::preprocessing::preprocess_image;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_jpeg(dir: &std::path::Path, width: u32, height: u32, name: &str) -> PathBuf {
    let path = dir.join(name);
    let img = image::ImageBuffer::from_fn(width, height, |_, _| image::Rgb([128u8, 128, 128]));
    img.save(&path).unwrap();
    path
}

#[test]
fn test_preprocess_small_jpeg_no_resize() {
    let dir = TempDir::new().unwrap();
    let path = create_test_jpeg(dir.path(), 800, 600, "small.jpg");

    let result = preprocess_image(&path).unwrap();
    assert!(!result.base64.is_empty());
    assert!(!result.was_resized);
}

#[test]
fn test_preprocess_large_jpeg_resizes() {
    let dir = TempDir::new().unwrap();
    let path = create_test_jpeg(dir.path(), 4000, 3000, "large.jpg");

    let result = preprocess_image(&path).unwrap();
    assert!(result.was_resized);
    assert!(!result.base64.is_empty());

    let decoded = base64::prelude::BASE64_STANDARD
        .decode(&result.base64)
        .unwrap();
    let img = image::load_from_memory(&decoded).unwrap();
    let (w, h) = img.dimensions();
    assert!(
        w <= 2048 && h <= 2048,
        "Resized image should fit within 2048x2048, got {}x{}",
        w,
        h
    );
}

/// A JPEG already within the 2048px limit must be passed through with its
/// original bytes — no decode → re-encode round trip (which costs CPU and
/// loses quality to JPEG re-compression).
#[test]
fn test_preprocess_small_jpeg_passes_through_original_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("gradient.jpg");
    // Gradient content so a lossy re-encode cannot accidentally reproduce
    // the original bytes (as a flat single-colour image could).
    let img = image::ImageBuffer::from_fn(800, 600, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    img.save(&path).unwrap();

    let result = preprocess_image(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    assert_eq!(
        result.base64,
        base64::prelude::BASE64_STANDARD.encode(&original),
        "in-range JPEG should be base64 of the untouched file bytes"
    );
    assert!(!result.was_resized);
}

/// PNG bytes hiding behind a `.jpg` extension must NOT be passed through —
/// the LLM payload is declared as JPEG, so mislabelled files take the
/// decode + JPEG re-encode path.
#[test]
fn test_preprocess_mislabelled_png_is_reencoded_as_jpeg() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("actually-a-png.jpg");
    let img = image::ImageBuffer::from_fn(64, 64, |x, y| {
        image::Rgb([(x * 4) as u8, (y * 4) as u8, 0u8])
    });
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();

    let result = preprocess_image(&path).unwrap();
    let decoded = base64::prelude::BASE64_STANDARD
        .decode(&result.base64)
        .unwrap();
    assert!(
        image::guess_format(&decoded).unwrap() == image::ImageFormat::Jpeg,
        "output must be real JPEG bytes, not passed-through PNG"
    );
    assert!(!result.was_resized);
}

#[test]
fn test_preprocess_unreadable_file_returns_error() {
    let path = PathBuf::from("/nonexistent/photo.jpg");
    let result = preprocess_image(&path);
    assert!(result.is_err());
}

#[test]
fn test_preprocess_invalid_raw_returns_error() {
    // rawler requires a structurally valid RAW file; garbage data should fail gracefully.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fake.cr2");
    std::fs::write(&path, [0u8; 100]).unwrap();

    let result = preprocess_image(&path);
    assert!(result.is_err());
}

#[test]
fn test_preprocess_unsupported_format_returns_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("photo.bmp");
    // Create a dummy file so the error comes from the format check, not file read
    std::fs::write(&path, b"dummy").unwrap();
    let result = preprocess_image(&path);
    assert!(result.is_err());
}
