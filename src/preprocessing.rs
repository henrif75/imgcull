//! Image preprocessing for vision LLM API submission.
//!
//! This module handles loading, optionally resizing, and base64-encoding images
//! so they can be sent to a vision LLM. Supported inputs are standard JPEG files
//! and common RAW camera formats (CR2, NEF, ARW, DNG, ORF). RAW files are handled
//! by extracting the embedded JPEG preview before further processing.

use anyhow::{Context, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use image::GenericImageView;
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
/// Reads the image at `path`, decodes it (extracting an embedded JPEG preview
/// for RAW files), resizes it if either dimension exceeds `2048` pixels, and
/// returns a [`PreprocessedImage`] containing the base64-encoded JPEG and a
/// resize flag.
///
/// # Supported formats
///
/// | Extension | Handling |
/// |-----------|---------|
/// | `jpg`, `jpeg` | Read directly |
/// | `cr2`, `nef`, `arw`, `dng`, `orf` | Embedded JPEG preview extracted |
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read.
/// - No embedded JPEG preview is found in a RAW file.
/// - The image bytes cannot be decoded.
/// - Re-encoding the resized image fails.
/// - The file extension indicates an unsupported format.
pub fn preprocess_image(path: &Path) -> Result<PreprocessedImage> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let image_bytes = match ext.as_str() {
        "jpg" | "jpeg" => {
            std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?
        }
        "cr2" | "nef" | "arw" | "dng" | "orf" => extract_raw_preview(path)?,
        _ => anyhow::bail!("Unsupported format: {}", path.display()),
    };

    let img = image::load_from_memory(&image_bytes)
        .with_context(|| format!("Cannot decode image: {}", path.display()))?;

    let (width, height) = img.dimensions();
    let needs_resize = width > MAX_DIMENSION || height > MAX_DIMENSION;

    let final_bytes = if needs_resize {
        let resized = img.resize(
            MAX_DIMENSION,
            MAX_DIMENSION,
            image::imageops::FilterType::Lanczos3,
        );
        let mut buf = Cursor::new(Vec::new());
        resized
            .write_to(&mut buf, image::ImageFormat::Jpeg)
            .context("Failed to encode resized image")?;
        buf.into_inner()
    } else {
        image_bytes
    };

    Ok(PreprocessedImage {
        base64: BASE64_STANDARD.encode(&final_bytes),
        was_resized: needs_resize,
    })
}

/// Extract the embedded JPEG preview from a RAW camera file.
///
/// Searches the raw byte stream for a JPEG SOI marker (`FF D8`) and uses the
/// last JPEG EOI marker (`FF D9`) to delimit the largest contiguous preview.
/// Most manufacturer RAW formats (Canon CR2, Nikon NEF, Sony ARW, Adobe DNG,
/// Olympus ORF) embed a full-resolution or near-full-resolution JPEG preview
/// in this way.
///
/// # Errors
///
/// Returns an error if the file cannot be read, no SOI marker is found, no EOI
/// marker is found after the SOI, or the computed boundaries are invalid.
fn extract_raw_preview(path: &Path) -> Result<Vec<u8>> {
    let data =
        std::fs::read(path).with_context(|| format!("Cannot read RAW file: {}", path.display()))?;

    // Find the first JPEG SOI marker (FF D8)
    let start = data
        .windows(2)
        .position(|w| w == [0xFF, 0xD8])
        .with_context(|| format!("No JPEG preview found in RAW file: {}", path.display()))?;

    // Find the last JPEG EOI marker (FF D9) to capture the largest preview
    let end = data
        .windows(2)
        .rposition(|w| w == [0xFF, 0xD9])
        .map(|p| p + 2)
        .with_context(|| format!("Malformed JPEG preview in RAW file: {}", path.display()))?;

    if end <= start {
        anyhow::bail!("Invalid JPEG preview boundaries in: {}", path.display());
    }

    Ok(data[start..end].to_vec())
}
