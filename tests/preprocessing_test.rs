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

#[test]
fn test_preprocess_unreadable_file_returns_error() {
    let path = PathBuf::from("/nonexistent/photo.jpg");
    let result = preprocess_image(&path);
    assert!(result.is_err());
}

#[test]
fn test_preprocess_raw_with_embedded_jpeg() {
    // Test RAW extraction through preprocess_image using a fake .cr2 file
    // that contains a real, valid embedded JPEG.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fake.cr2");

    // Generate a real tiny JPEG in memory using the image crate
    let img = image::ImageBuffer::from_fn(10u32, 10u32, |_, _| image::Rgb([64u8, 128, 192]));
    let mut jpeg_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg_bytes);
    img.write_to(&mut cursor, image::ImageFormat::Jpeg).unwrap();

    // Build a fake RAW file: 100 bytes of garbage, then the real JPEG
    let mut raw_data: Vec<u8> = vec![0x00; 100];
    raw_data.extend_from_slice(&jpeg_bytes);
    std::fs::write(&path, &raw_data).unwrap();

    let result = preprocess_image(&path).unwrap();
    assert!(!result.base64.is_empty());
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
