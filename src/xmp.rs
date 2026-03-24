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
///
/// Stores image metadata including ratings, descriptions, and imgcull scores
/// computed by LLM evaluation.
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
    /// Whether any field has been modified since construction or last read.
    ///
    /// Used by the pipeline to skip writing when nothing has changed.
    dirty: bool,
    /// Raw file content read from disk, used to merge our fields into the
    /// existing XML rather than regenerating the whole document from scratch.
    raw_content: Option<String>,
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
            // Parse individual dimension scores before moving val.
            for dim_name in val.split(',') {
                let dim_name = dim_name.trim();
                if let Some(score_str) = extract_element(&content, &format!("imgcull:{dim_name}"))
                    && let Ok(v) = score_str.parse::<f64>()
                {
                    sidecar.dimension_scores.push((dim_name.to_string(), v));
                }
            }
            sidecar.dimensions_list = Some(val);
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

        // Store raw content for merge-on-write (moved, not cloned).
        sidecar.raw_content = Some(content);

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

    /// Returns `true` if any field has been modified since construction or last read.
    ///
    /// Use this to skip writing the sidecar when nothing has changed.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Returns the `dc:description` text, if any.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Set the `dc:description` text.
    pub fn set_description(&mut self, desc: &str) {
        self.description = Some(desc.to_string());
        self.dirty = true;
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
        self.dirty = true;

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
        self.dirty = true;
    }

    /// Write the XMP document to disk.
    ///
    /// If the sidecar was previously read from disk (i.e. `raw_content` is
    /// set), the existing content is used as the base and only imgcull-managed
    /// fields (`dc:description`, `xmp:Rating`, and all `imgcull:*` elements)
    /// are replaced, preserving any other namespaces and fields written by
    /// third-party tools such as Lightroom.
    ///
    /// If no existing content is available, a new document is generated from
    /// scratch.
    pub fn write(&self, path: &Path) -> Result<()> {
        let xml = if let Some(ref base) = self.raw_content {
            merge_into_existing(base, self)
        } else {
            generate_from_scratch(self)
        };

        fs::write(path, &xml).with_context(|| format!("writing XMP: {}", path.display()))?;
        Ok(())
    }
}

/// Build the imgcull fields fragment that is inserted into `<rdf:Description>`.
fn build_imgcull_fields(sidecar: &XmpSidecar) -> String {
    let mut fields = String::new();

    // dc:description
    if let Some(ref desc) = sidecar.description {
        let escaped = escape(desc);
        fields.push_str("      <dc:description>\n");
        fields.push_str("        <rdf:Alt>\n");
        fields.push_str(&format!(
            "          <rdf:li xml:lang=\"x-default\">{escaped}</rdf:li>\n"
        ));
        fields.push_str("        </rdf:Alt>\n");
        fields.push_str("      </dc:description>\n");
    }

    // imgcull:score
    if let Some(score) = sidecar.overall_score {
        fields.push_str(&format!(
            "      <imgcull:score>{score:.2}</imgcull:score>\n"
        ));
    }

    // Per-dimension scores
    for (name, val) in &sidecar.dimension_scores {
        fields.push_str(&format!(
            "      <imgcull:{name}>{val:.2}</imgcull:{name}>\n"
        ));
    }

    // imgcull:scored_at
    if let Some(ref ts) = sidecar.scored_at {
        fields.push_str(&format!(
            "      <imgcull:scored_at>{ts}</imgcull:scored_at>\n"
        ));
    }

    // imgcull:scored_by
    if let Some(ref model) = sidecar.scored_by {
        fields.push_str(&format!(
            "      <imgcull:scored_by>{model}</imgcull:scored_by>\n"
        ));
    }

    // imgcull:dimensions
    if let Some(ref dims) = sidecar.dimensions_list {
        fields.push_str(&format!(
            "      <imgcull:dimensions>{dims}</imgcull:dimensions>\n"
        ));
    }

    fields
}

/// Generate an XMP document from scratch with all managed namespaces declared.
fn generate_from_scratch(sidecar: &XmpSidecar) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    xml.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    xml.push_str("    <rdf:Description\n");
    xml.push_str("      xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n");
    xml.push_str("      xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
    xml.push_str("      xmlns:imgcull=\"http://imgcull.dev/ns/1.0/\"");

    if let Some(rating) = sidecar.rating {
        xml.push_str(&format!("\n      xmp:Rating=\"{rating}\""));
    }

    xml.push_str(">\n");
    xml.push_str(&build_imgcull_fields(sidecar));
    xml.push_str("    </rdf:Description>\n");
    xml.push_str("  </rdf:RDF>\n");
    xml.push_str("</x:xmpmeta>\n");
    xml
}

/// Merge imgcull-managed fields into an existing XMP document string.
///
/// Preserves all content not managed by imgcull (e.g. Lightroom `lr:*`,
/// `photoshop:*`, `tiff:*` fields).  The strategy is:
///
/// 1. Strip any existing `xmp:Rating` attribute from `<rdf:Description`.
/// 2. Remove existing `<dc:description>` blocks and `<imgcull:*>` elements.
/// 3. Inject the new imgcull fields just before `</rdf:Description>`.
/// 4. Ensure the `xmlns:imgcull` namespace is declared on `<rdf:Description`.
fn merge_into_existing(base: &str, sidecar: &XmpSidecar) -> String {
    let mut xml = base.to_string();

    // --- Strip existing xmp:Rating attribute ---
    remove_attribute(&mut xml, "xmp:Rating");

    // --- Inject xmp:Rating as an attribute if set ---
    if let Some(rating) = sidecar.rating {
        // Find the closing `>` of the opening <rdf:Description ... > tag.
        // The tag may end with `>` or `/>`, but in our generated XML it always
        // ends with `>`.  We search for the first `>` after `<rdf:Description`.
        if let Some(desc_pos) = xml.find("<rdf:Description")
            && let Some(rel_close) = xml[desc_pos..].find('>')
        {
            let close_pos = desc_pos + rel_close;
            // Insert the attribute before the `>`.
            let attr = format!("\n      xmp:Rating=\"{rating}\"");
            xml.insert_str(close_pos, &attr);
        }
    }

    // --- Remove existing dc:description block ---
    remove_element_block(&mut xml, "<dc:description>", "</dc:description>");

    // --- Remove existing imgcull:* elements ---
    // We iteratively remove any element whose tag starts with `<imgcull:`.
    while let Some(start) = xml.find("<imgcull:") {
        // Find the tag name to build the matching close tag.
        let tag_start = start + 1; // skip '<'
        let tag_end = xml[tag_start..]
            .find(['>', ' ', '\n', '\r'])
            .map(|p| tag_start + p)
            .unwrap_or(xml.len());
        let tag_name = xml[tag_start..tag_end].to_string();
        let close = format!("</{tag_name}>");
        let open_tag = xml[start..tag_end + 1].to_string();
        remove_element_block(&mut xml, &open_tag, &close);
    }

    // --- Ensure required namespace declarations are present ---
    ensure_namespace(
        &mut xml,
        "xmlns:imgcull",
        "xmlns:imgcull=\"http://imgcull.dev/ns/1.0/\"",
    );
    if sidecar.description.is_some() {
        ensure_namespace(
            &mut xml,
            "xmlns:dc",
            "xmlns:dc=\"http://purl.org/dc/elements/1.1/\"",
        );
    }
    if sidecar.rating.is_some() {
        ensure_namespace(
            &mut xml,
            "xmlns:xmp",
            "xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"",
        );
    }

    // --- Insert our new fields just before </rdf:Description> ---
    let new_fields = build_imgcull_fields(sidecar);
    if !new_fields.is_empty()
        && let Some(close_pos) = xml.find("</rdf:Description>")
    {
        xml.insert_str(close_pos, &new_fields);
    }

    xml
}

/// Inject `decl` onto `<rdf:Description` if `check` attribute is not already present.
fn ensure_namespace(xml: &mut String, check: &str, decl: &str) {
    if xml.contains(check) {
        return;
    }
    if let Some(desc_pos) = xml.find("<rdf:Description")
        && let Some(rel_close) = xml[desc_pos..].find('>')
    {
        xml.insert_str(desc_pos + rel_close, &format!("\n      {decl}"));
    }
}

/// Remove an XML element block identified by its literal open and close tags, in place.
///
/// Strips any leading whitespace / newline before the open tag so that the
/// surrounding indentation is not left dangling.
fn remove_element_block(xml: &mut String, open: &str, close: &str) {
    if let Some(start) = xml.find(open)
        && let Some(rel_end) = xml[start..].find(close)
    {
        let end = start + rel_end + close.len();
        // Also consume a trailing newline if present.
        let end = if xml[end..].starts_with('\n') {
            end + 1
        } else {
            end
        };
        // Also strip the leading whitespace on the same line.
        let trimmed_start = xml[..start].rfind('\n').map(|p| p + 1).unwrap_or(start);
        let actual_start = if xml[trimmed_start..start].chars().all(char::is_whitespace) {
            trimmed_start
        } else {
            start
        };
        xml.replace_range(actual_start..end, "");
    }
}

/// Remove an XML attribute `name="value"` from a string in place (including leading
/// whitespace / newline before the attribute name).
fn remove_attribute(xml: &mut String, attr_name: &str) {
    let search = format!("{attr_name}=\"");
    if let Some(start) = xml.find(&search)
        && let Some(rel_end) = xml[start + search.len()..].find('"')
    {
        let end = start + search.len() + rel_end + 1; // +1 for closing "
        // Strip leading whitespace/newline before the attribute.
        let stripped_start = xml[..start]
            .rfind(|c: char| !c.is_whitespace())
            .map(|p| p + 1)
            .unwrap_or(start);
        xml.replace_range(stripped_start..end, "");
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
