//! Report generation for scored images.
//!
//! Reads existing XMP sidecars and outputs a summary table or CSV to stdout.
//! No LLM calls or API keys required.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::discovery::{has_extension, is_supported};
use crate::scoring::DIMENSIONS;
use crate::xmp::{XmpSidecar, sidecar_for_image};

/// Output format for the report.
pub enum OutputFormat {
    /// Aligned terminal table.
    Table,
    /// Comma-separated values.
    Csv,
}

/// Sort order for report rows.
pub enum SortOrder {
    /// Sort by overall score.
    Score,
    /// Sort by filename.
    Filename,
    /// Sort by star rating.
    Rating,
}

/// One row of report data extracted from an XMP sidecar.
#[derive(Default)]
struct ReportRow {
    filename: String,
    xmp_file: String,
    overall_score: Option<f64>,
    rating: Option<u8>,
    /// Per-dimension scores, aligned with [`DIMENSIONS`].
    dims: [Option<f64>; DIMENSIONS.len()],
    keywords: String,
    scored_by: Option<String>,
    description: Option<String>,
}

/// Extract a report row from an XMP sidecar file.
fn row_from_xmp(xmp_path: &Path) -> Option<ReportRow> {
    let sidecar = match XmpSidecar::read(xmp_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Warning: cannot read {}: {e}", xmp_path.display());
            return None;
        }
    };

    let filename = sidecar
        .original_filename()
        .map(String::from)
        .unwrap_or_else(|| {
            xmp_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let xmp_file = xmp_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut dims = [None; DIMENSIONS.len()];
    for (name, val) in sidecar.dimension_scores() {
        if let Some(i) = DIMENSIONS.iter().position(|d| *d == name.as_str()) {
            dims[i] = Some(*val);
        }
    }

    Some(ReportRow {
        filename,
        xmp_file,
        overall_score: sidecar.overall_score(),
        rating: sidecar.rating(),
        dims,
        keywords: sidecar.keywords().join(", "),
        scored_by: sidecar.scored_by().map(String::from),
        description: sidecar.description().map(String::from),
    })
}

/// Discover all XMP sidecar paths from the given paths.
///
/// Finds sidecars in two ways:
/// 1. Directly — any `.xmp` file found in the given directories (scanned
///    recursively) or passed as a path.
/// 2. Via images — for each image file passed directly, looks for a matching
///    `.xmp` sidecar.  (Directories need no image pairing: the recursive scan
///    in step 1 already collects every `.xmp` they contain.)
///
/// Results are deduplicated and sorted.
fn discover_xmp_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut xmp_files = BTreeSet::new();

    for path in paths {
        if path.is_dir() {
            // The recursive scan collects every `.xmp` in the tree — a
            // superset of the image-paired sidecars — so directories need no
            // separate pairing pass.
            scan_dir_for_xmp(path, &mut xmp_files);
        } else if path.is_file() {
            if is_xmp(path) {
                xmp_files.insert(path.clone());
            } else if is_supported(path) {
                // Pair an image passed directly with its sidecar, if any.
                let sidecar_path = sidecar_for_image(path);
                if sidecar_path.exists() {
                    xmp_files.insert(sidecar_path);
                }
            }
        }
    }

    xmp_files.into_iter().collect()
}

/// Returns `true` if the file has an `.xmp` extension (case-insensitive).
fn is_xmp(path: &Path) -> bool {
    has_extension(path, &["xmp"])
}

/// Recursively scan a directory for `.xmp` files.
fn scan_dir_for_xmp(dir: &Path, results: &mut BTreeSet<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Warning: cannot read directory {}: {e}", dir.display());
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path.is_dir() {
            scan_dir_for_xmp(&path, results);
        } else if is_xmp(&path) {
            results.insert(path);
        }
    }
}

/// Sort rows by the given order.
fn sort_rows(rows: &mut [ReportRow], order: &SortOrder, ascending: bool) {
    rows.sort_by(|a, b| {
        let cmp = match order {
            SortOrder::Score => a
                .overall_score
                .unwrap_or(-1.0)
                .partial_cmp(&b.overall_score.unwrap_or(-1.0))
                .unwrap_or(std::cmp::Ordering::Equal),
            SortOrder::Rating => a.rating.unwrap_or(0).cmp(&b.rating.unwrap_or(0)),
            SortOrder::Filename => a.filename.cmp(&b.filename),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Truncate a string to `max` characters, appending ".." if truncated.
///
/// Operates on `char`s rather than bytes so multi-byte UTF-8 (accented
/// filenames, CJK text, LLM-generated keywords) cannot land the cut on a
/// non-char boundary and panic.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let keep = max.saturating_sub(2);
        format!("{}..", s.chars().take(keep).collect::<String>())
    }
}

/// Format an optional f64 score as a fixed-width string.
fn fmt_score(val: Option<f64>) -> String {
    val.map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "-".to_string())
}

/// Render an aligned terminal table to stdout.
fn render_table(rows: &[ReportRow]) {
    let headers = [
        "Filename", "XMP File", "Score", "Stars", "Sharp", "Expos", "Compo", "Subj", "Aesth",
        "Keywords", "Model",
    ];

    // Build formatted cell values for each row.
    let formatted: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut cells = vec![
                truncate(&r.filename, 36),
                truncate(&r.xmp_file, 36),
                fmt_score(r.overall_score),
                r.rating
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ];
            cells.extend(r.dims.iter().map(|d| fmt_score(*d)));
            cells.push(truncate(&r.keywords, 24));
            cells.push(truncate(r.scored_by.as_deref().unwrap_or("-"), 22));
            cells
        })
        .collect();

    // Compute column widths.
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &formatted {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    // Print header.
    let header_line: String = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<w$}", h, w = widths[i]))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{header_line}");

    // Separator.
    let total_width = widths.iter().sum::<usize>() + (widths.len() - 1) * 2;
    println!("{}", "─".repeat(total_width));

    // Data rows.
    for row in &formatted {
        let line: String = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                // Right-align numeric columns: Score, Stars, and the
                // per-dimension scores that follow them.
                if (2..4 + DIMENSIONS.len()).contains(&i) {
                    format!("{:>w$}", cell, w = widths[i])
                } else {
                    format!("{:<w$}", cell, w = widths[i])
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        println!("{line}");
    }
}

/// Escape a string for CSV output (RFC 4180).
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Render CSV output to stdout.
fn render_csv(rows: &[ReportRow]) {
    println!(
        "filename,xmp_file,score,rating,{},keywords,model,description",
        DIMENSIONS.join(",")
    );

    for r in rows {
        let mut fields = vec![
            csv_escape(&r.filename),
            csv_escape(&r.xmp_file),
            fmt_score(r.overall_score),
            r.rating
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ];
        fields.extend(r.dims.iter().map(|d| fmt_score(*d)));
        fields.push(csv_escape(&r.keywords));
        fields.push(csv_escape(r.scored_by.as_deref().unwrap_or("")));
        fields.push(csv_escape(r.description.as_deref().unwrap_or("")));
        println!("{}", fields.join(","));
    }
}

/// Generate a report from scored images.
///
/// Discovers XMP sidecars in the given paths (both standalone `.xmp` files and
/// sidecars matching discovered images) and outputs a summary in the requested
/// format to stdout.  Individual read or parse failures are logged to stderr
/// and skipped, so this function does not fail.
pub fn run_report(paths: &[PathBuf], format: OutputFormat, sort: SortOrder, ascending: bool) {
    let xmp_files = discover_xmp_files(paths);
    if xmp_files.is_empty() {
        eprintln!("No XMP sidecars found.");
        return;
    }

    let mut rows: Vec<ReportRow> = xmp_files.iter().filter_map(|p| row_from_xmp(p)).collect();
    if rows.is_empty() {
        eprintln!("No readable XMP sidecars found.");
        return;
    }

    sort_rows(&mut rows, &sort, ascending);

    match format {
        OutputFormat::Table => render_table(&rows),
        OutputFormat::Csv => render_csv(&rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::ScoringResult;
    use tempfile::TempDir;

    #[test]
    fn test_csv_escape_plain() {
        assert_eq!(csv_escape("hello"), "hello");
    }

    #[test]
    fn test_csv_escape_with_comma() {
        assert_eq!(csv_escape("a, b"), "\"a, b\"");
    }

    #[test]
    fn test_csv_escape_with_quotes() {
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 7), "hello..");
    }

    #[test]
    fn test_truncate_multibyte_does_not_panic() {
        // Cut index would land mid-character if sliced by bytes.
        assert_eq!(truncate("café résumé naïve", 7), "café ..");
        // CJK: each char is 3 bytes in UTF-8.
        assert_eq!(truncate("写真撮影技術論", 5), "写真撮..");
        // 5 chars / 7 bytes: fits by char count, so it is returned whole even
        // though its byte length exceeds `max`.
        assert_eq!(truncate("caféé", 6), "caféé");
    }

    #[test]
    fn test_row_from_xmp() {
        let tmp = TempDir::new().unwrap();
        let xmp_path = tmp.path().join("photo.xmp");
        let scores = ScoringResult {
            sharpness: Some(0.90),
            exposure: Some(0.75),
            ..Default::default()
        };
        let mut sidecar = XmpSidecar::new();
        sidecar.set_description("A test photo");
        sidecar.set_scores(&scores, 0.825, "test/model");
        sidecar.set_rating(4);
        sidecar.set_original_filename("photo.jpg");
        sidecar.set_keywords(&["portrait".to_string(), "outdoors".to_string()]);
        sidecar.write(&xmp_path).unwrap();

        let row = row_from_xmp(&xmp_path).expect("should produce a row");
        assert_eq!(row.filename, "photo.jpg");
        assert_eq!(row.xmp_file, "photo.xmp");
        assert!((row.overall_score.unwrap() - 0.82).abs() < 0.01);
        assert_eq!(row.rating, Some(4));
        // dims is aligned with DIMENSIONS: [sharpness, exposure, ...].
        assert!((row.dims[0].unwrap() - 0.90).abs() < 1e-9);
        assert!((row.dims[1].unwrap() - 0.75).abs() < 1e-9);
        assert_eq!(row.keywords, "portrait, outdoors");
    }

    #[test]
    fn test_discover_xmp_finds_standalone_xmp_files() {
        let tmp = TempDir::new().unwrap();
        // Create .xmp files without corresponding images.
        let xmp1 = tmp.path().join("photo-gemma.xmp");
        let xmp2 = tmp.path().join("photo-claude.xmp");
        // Minimal valid XMP so read() won't fail.
        let minimal_xmp = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description/>
  </rdf:RDF>
</x:xmpmeta>
"#;
        std::fs::write(&xmp1, minimal_xmp).unwrap();
        std::fs::write(&xmp2, minimal_xmp).unwrap();

        let xmp_files = discover_xmp_files(&[tmp.path().to_path_buf()]);
        assert_eq!(xmp_files.len(), 2);
    }

    #[test]
    fn test_discover_xmp_pairs_directly_passed_image_file() {
        let tmp = TempDir::new().unwrap();
        let image = tmp.path().join("photo.jpg");
        let sidecar = tmp.path().join("photo.xmp");
        std::fs::write(&image, b"not really a jpeg").unwrap();
        let minimal_xmp = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description/>
  </rdf:RDF>
</x:xmpmeta>
"#;
        std::fs::write(&sidecar, minimal_xmp).unwrap();

        // Passing the image file directly should surface its matching sidecar.
        let xmp_files = discover_xmp_files(&[image]);
        assert_eq!(xmp_files, vec![sidecar]);
    }

    #[test]
    fn test_sort_by_score_descending() {
        let mut rows = vec![
            ReportRow {
                filename: "a.jpg".into(),
                xmp_file: "a.xmp".into(),
                overall_score: Some(0.5),
                ..Default::default()
            },
            ReportRow {
                filename: "b.jpg".into(),
                xmp_file: "b.xmp".into(),
                overall_score: Some(0.9),
                ..Default::default()
            },
        ];
        sort_rows(&mut rows, &SortOrder::Score, false);
        assert_eq!(rows[0].filename, "b.jpg");
        assert_eq!(rows[1].filename, "a.jpg");
    }

    #[test]
    fn test_sort_by_filename_ascending() {
        let mut rows = vec![
            ReportRow {
                filename: "zebra.jpg".into(),
                xmp_file: "zebra.xmp".into(),
                ..Default::default()
            },
            ReportRow {
                filename: "apple.jpg".into(),
                xmp_file: "apple.xmp".into(),
                ..Default::default()
            },
        ];
        sort_rows(&mut rows, &SortOrder::Filename, true);
        assert_eq!(rows[0].filename, "apple.jpg");
        assert_eq!(rows[1].filename, "zebra.jpg");
    }
}
