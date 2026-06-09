//! Image preprocessing for vision LLM API submission.
//!
//! This module handles loading, optionally resizing, and base64-encoding images
//! so they can be sent to a vision LLM. Supported inputs are standard JPEG files
//! and a wide range of RAW camera formats. RAW files are handled by extracting
//! the embedded preview using the `rawler` library.

use anyhow::{Context, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use image::GenericImageView;
use rawler::decoders::RawDecodeParams;
use std::io::Cursor;
use std::path::Path;

/// Maximum pixel dimension (width or height) before an image is resized.
///
/// Images larger than this in either dimension are downscaled while
/// preserving the original aspect ratio.
const MAX_DIMENSION: u32 = 2048;

/// The result of preprocessing an image for LLM submission.
///
/// Contains a base64-encoded JPEG string ready to embed in an API request,
/// along with a flag indicating whether the source image was downscaled.
pub struct PreprocessedImage {
    /// Base64-encoded JPEG bytes of the (possibly resized) image.
    pub base64: String,
    /// `true` if the image was resized because it exceeded the maximum dimension (2048 px).
    pub was_resized: bool,
}

/// Preprocess an image file for submission to a vision LLM.
///
/// Reads the image at `path`, decodes it (extracting an embedded preview for
/// RAW files), resizes it if either dimension exceeds `2048` pixels, and
/// returns a [`PreprocessedImage`] containing the base64-encoded JPEG and a
/// resize flag.
///
/// # Supported formats
///
/// | Extension | Handling |
/// |-----------|---------|
/// | `jpg`, `jpeg` | Read directly |
/// | `arw`, `cr2`, `cr3`, `dng`, `erf`, `mef`, `mos`, `mrw`, `nef`, `nrw`, `orf`, `pef`, `raf`, `rw2`, `sr2`, `srw` | Embedded preview extracted via `rawler` |
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read.
/// - No embedded preview is found in a RAW file.
/// - The image bytes cannot be decoded.
/// - Re-encoding the resized image fails.
/// - The file extension indicates an unsupported format.
pub fn preprocess_image(path: &Path) -> Result<PreprocessedImage> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "jpg" | "jpeg" => preprocess_jpeg(path),
        // Everything else in SUPPORTED_EXTENSIONS is a RAW format handled by
        // rawler — the jpg/jpeg arm above already consumed the only non-RAW
        // entries, so discovery's list stays the single source of truth.
        e if crate::discovery::SUPPORTED_EXTENSIONS.contains(&e) => {
            let img = rawler::analyze::extract_preview_pixels(path, &RawDecodeParams::default())
                .with_context(|| {
                    format!("Cannot extract preview from RAW file: {}", path.display())
                })?;
            resize_and_encode(img)
        }
        _ => anyhow::bail!("Unsupported format: {}", path.display()),
    }
}

/// Preprocess a JPEG file, passing the original bytes through untouched when
/// the image is already within [`MAX_DIMENSION`].
///
/// The dimensions are read from the file header alone, so the passthrough
/// path never pays for a full decode or a lossy re-encode.
fn preprocess_jpeg(path: &Path) -> Result<PreprocessedImage> {
    let bytes = std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;

    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .with_context(|| format!("Cannot read image header: {}", path.display()))?;
    // Only pass through real JPEG content — a mislabelled file (e.g. PNG bytes
    // in a `.jpg`) still needs the decode + JPEG re-encode below.
    let is_jpeg = reader.format() == Some(image::ImageFormat::Jpeg);
    let (width, height) = reader
        .into_dimensions()
        .with_context(|| format!("Cannot decode image: {}", path.display()))?;

    if is_jpeg && width <= MAX_DIMENSION && height <= MAX_DIMENSION {
        return Ok(PreprocessedImage {
            base64: BASE64_STANDARD.encode(&bytes),
            was_resized: false,
        });
    }

    let img = image::load_from_memory(&bytes)
        .with_context(|| format!("Cannot decode image: {}", path.display()))?;
    resize_and_encode(img)
}

/// Downscale `img` to fit [`MAX_DIMENSION`] if needed and encode it as a
/// base64 JPEG.
fn resize_and_encode(img: image::DynamicImage) -> Result<PreprocessedImage> {
    let (width, height) = img.dimensions();
    let needs_resize = width > MAX_DIMENSION || height > MAX_DIMENSION;

    // CatmullRom keeps enough detail for LLM scoring (including the sharpness
    // dimension) at a fraction of Lanczos3's cost.
    let final_img = if needs_resize {
        img.resize(
            MAX_DIMENSION,
            MAX_DIMENSION,
            image::imageops::FilterType::CatmullRom,
        )
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    final_img
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .context("Failed to encode image as JPEG")?;

    Ok(PreprocessedImage {
        base64: BASE64_STANDARD.encode(buf.get_ref()),
        was_resized: needs_resize,
    })
}
