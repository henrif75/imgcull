//! XMP sidecar read/write for image metadata and imgcull scores.
//!
//! Provides utilities for computing sidecar paths, reading and writing XMP
//! sidecar files, and backing up existing sidecars before modification.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use quick_xml::Reader;
use quick_xml::escape::escape;
use quick_xml::events::Event;

use crate::scoring::ScoringResult;

/// Utility for computing the XMP sidecar path that corresponds to an image.
pub struct SidecarPath;

impl SidecarPath {
    /// Replace the image file extension with `.xmp`.
    ///
    /// # Examples
    /// - `IMG_1234.jpg` becomes `IMG_1234.xmp`
    /// - `IMG_5678.CR2` becomes `IMG_5678.xmp`
    pub fn for_image(path: &Path) -> PathBuf {
        path.with_extension("xmp")
    }
}

/// In-memory representation of an XMP sidecar file.
#[derive(Debug, Clone, Default)]
pub struct XmpSidecar {
    /// The `dc:description` text, if present.
    description: Option<String>,
    /// XMP rating (1–5 stars).
    rating: Option<u8>,
    /// Overall imgcull score (0.0–1.0).
    overall_score: Option<f64>,
    /// Per-dimension scores, e.g. `[("sharpness", 0.9), ...]`.
    dimension_scores: Vec<(String, f64)>,
    /// ISO-8601 timestamp of when scoring was performed.
    scored_at: Option<String>,
    /// Identifier of the model that produced the scores.
    scored_by: Option<String>,
    /// Comma-separated list of scored dimensions.
    dimensions_list: Option<String>,
}

impl XmpSidecar {
    /// Create an empty sidecar with no metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse an existing XMP sidecar file.
    ///
    /// Extracts `dc:description` text and `imgcull:score` if present.
    /// Returns an error for malformed XML.
    pub fn read(path: &Path) -> Result<Self> {
        let content =
            fs::read_to_string(path).with_context(|| format!("reading XMP: {}", path.display()))?;

        // Validate that the XML is well-formed by walking all events.
        let mut reader = Reader::from_str(&content);
        loop {
            match reader.read_event() {
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "malformed XML in {}: {}",
                        path.display(),
                        e
                    ));
                }
                _ => {}
            }
        }

        let mut sidecar = Self::new();

        // Extract dc:description text via simple string searching.
        if let Some(start) = content.find("<rdf:li xml:lang=\"x-default\">") {
            let text_start = start + "<rdf:li xml:lang=\"x-default\">".len();
            if let Some(end) = content[text_start..].find("</rdf:li>") {
                let desc = &content[text_start..text_start + end];
                if content[..start].contains("dc:description") {
                    sidecar.description = Some(desc.to_string());
                }
            }
        }

        // Extract imgcull:score.
        if let Some(score_str) = extract_element(&content, "imgcull:score")
            && let Ok(val) = score_str.parse::<f64>()
        {
            sidecar.overall_score = Some(val);
        }

        // Extract imgcull:scored_at.
        if let Some(val) = extract_element(&content, "imgcull:scored_at") {
            sidecar.scored_at = Some(val);
        }

        // Extract imgcull:scored_by.
        if let Some(val) = extract_element(&content, "imgcull:scored_by") {
            sidecar.scored_by = Some(val);
        }

        // Extract imgcull:dimensions.
        if let Some(val) = extract_element(&content, "imgcull:dimensions") {
            sidecar.dimensions_list = Some(val.clone());
            // Parse individual dimension scores.
            for dim_name in val.split(',') {
                let dim_name = dim_name.trim();
                if let Some(score_str) = extract_element(&content, &format!("imgcull:{dim_name}"))
                    && let Ok(v) = score_str.parse::<f64>()
                {
                    sidecar.dimension_scores.push((dim_name.to_string(), v));
                }
            }
        }

        // Extract xmp:Rating from attribute.
        if let Some(start) = content.find("xmp:Rating=\"") {
            let val_start = start + "xmp:Rating=\"".len();
            if let Some(end) = content[val_start..].find('"')
                && let Ok(r) = content[val_start..val_start + end].parse::<u8>()
            {
                sidecar.rating = Some(r);
            }
        }

        Ok(sidecar)
    }

    /// Returns `true` if a `dc:description` is present.
    pub fn has_description(&self) -> bool {
        self.description.is_some()
    }

    /// Returns `true` if imgcull scores have been set.
    pub fn has_scores(&self) -> bool {
        self.overall_score.is_some()
    }

    /// Returns the `dc:description` text, if any.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Set the `dc:description` text.
    pub fn set_description(&mut self, desc: &str) {
        self.description = Some(desc.to_string());
    }

    /// Store scoring results from an LLM evaluation.
    ///
    /// Records per-dimension scores, the overall score, a timestamp, and the
    /// model identifier. The `scored_at` field is set to the current UTC time.
    pub fn set_scores(
        &mut self,
        scores: &ScoringResult,
        dims: &[String],
        overall: f64,
        scored_by: &str,
    ) {
        self.overall_score = Some(overall);
        self.scored_by = Some(scored_by.to_string());
        self.scored_at = Some(Utc::now().to_rfc3339());
        self.dimensions_list = Some(dims.join(","));

        self.dimension_scores.clear();
        for dim in dims {
            if let Some(val) = scores.get(dim) {
                self.dimension_scores.push((dim.clone(), val));
            }
        }
    }

    /// Set the XMP star rating (1–5).
    pub fn set_rating(&mut self, stars: u8) {
        self.rating = Some(stars);
    }

    /// Write the complete XMP document to disk with proper namespaces.
    pub fn write(&self, path: &Path) -> Result<()> {
        let mut xml = String::new();
        xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
        xml.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
        xml.push_str("    <rdf:Description\n");
        xml.push_str("      xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n");
        xml.push_str("      xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
        xml.push_str("      xmlns:imgcull=\"http://imgcull.dev/ns/1.0/\"");

        if let Some(rating) = self.rating {
            xml.push_str(&format!("\n      xmp:Rating=\"{rating}\""));
        }

        xml.push_str(">\n");

        // dc:description
        if let Some(ref desc) = self.description {
            let escaped = escape(desc);
            xml.push_str("      <dc:description>\n");
            xml.push_str("        <rdf:Alt>\n");
            xml.push_str(&format!(
                "          <rdf:li xml:lang=\"x-default\">{escaped}</rdf:li>\n"
            ));
            xml.push_str("        </rdf:Alt>\n");
            xml.push_str("      </dc:description>\n");
        }

        // imgcull:score
        if let Some(score) = self.overall_score {
            xml.push_str(&format!(
                "      <imgcull:score>{score:.2}</imgcull:score>\n"
            ));
        }

        // Per-dimension scores
        for (name, val) in &self.dimension_scores {
            xml.push_str(&format!(
                "      <imgcull:{name}>{val:.2}</imgcull:{name}>\n"
            ));
        }

        // imgcull:scored_at
        if let Some(ref ts) = self.scored_at {
            xml.push_str(&format!(
                "      <imgcull:scored_at>{ts}</imgcull:scored_at>\n"
            ));
        }

        // imgcull:scored_by
        if let Some(ref model) = self.scored_by {
            xml.push_str(&format!(
                "      <imgcull:scored_by>{model}</imgcull:scored_by>\n"
            ));
        }

        // imgcull:dimensions
        if let Some(ref dims) = self.dimensions_list {
            xml.push_str(&format!(
                "      <imgcull:dimensions>{dims}</imgcull:dimensions>\n"
            ));
        }

        xml.push_str("    </rdf:Description>\n");
        xml.push_str("  </rdf:RDF>\n");
        xml.push_str("</x:xmpmeta>\n");

        fs::write(path, &xml).with_context(|| format!("writing XMP: {}", path.display()))?;
        Ok(())
    }
}

/// Copy an existing `.xmp` sidecar to `.xmp.bak`.
pub fn backup_sidecar(path: &Path) -> Result<()> {
    let backup = path.with_extension("xmp.bak");
    fs::copy(path, &backup)
        .with_context(|| format!("backing up {} to {}", path.display(), backup.display()))?;
    Ok(())
}

/// Extract text content between `<tag>` and `</tag>`.
fn extract_element(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = content.find(&open)?;
    let text_start = start + open.len();
    let end = content[text_start..].find(&close)?;
    Some(content[text_start..text_start + end].to_string())
}
