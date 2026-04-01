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
/// Images larger than this in either dimension are downscaled using Lanczos3
/// filtering while preserving the original aspect ratio.
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

    let img = match ext.as_str() {
        "jpg" | "jpeg" => {
            let bytes =
                std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
            image::load_from_memory(&bytes)
                .with_context(|| format!("Cannot decode image: {}", path.display()))?
        }
        "arw" | "cr2" | "cr3" | "dng" | "erf" | "mef" | "mos" | "mrw" | "nef" | "nrw" | "orf"
        | "pef" | "raf" | "rw2" | "sr2" | "srw" => {
            rawler::analyze::extract_preview_pixels(path, &RawDecodeParams::default())
                .with_context(|| {
                    format!("Cannot extract preview from RAW file: {}", path.display())
                })?
        }
        _ => anyhow::bail!("Unsupported format: {}", path.display()),
    };

    let (width, height) = img.dimensions();
    let needs_resize = width > MAX_DIMENSION || height > MAX_DIMENSION;

    let final_img = if needs_resize {
        img.resize(
            MAX_DIMENSION,
            MAX_DIMENSION,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let mut buf = Cursor::new(Vec::new());
    final_img
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .context("Failed to encode image as JPEG")?;
    let final_bytes = buf.into_inner();

    Ok(PreprocessedImage {
        base64: BASE64_STANDARD.encode(&final_bytes),
        was_resized: needs_resize,
    })
}
