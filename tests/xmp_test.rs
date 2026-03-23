use std::path::Path;

use imgcull::scoring::ScoringResult;
use imgcull::xmp::{SidecarPath, XmpSidecar, backup_sidecar};
use tempfile::TempDir;

#[test]
fn sidecar_path_from_jpeg() {
    let path = Path::new("photos/IMG_1234.jpg");
    assert_eq!(
        SidecarPath::for_image(path),
        Path::new("photos/IMG_1234.xmp")
    );
}

#[test]
fn sidecar_path_from_raw() {
    let path = Path::new("photos/IMG_5678.CR2");
    assert_eq!(
        SidecarPath::for_image(path),
        Path::new("photos/IMG_5678.xmp")
    );
}

#[test]
fn read_existing_with_description() {
    let sidecar =
        XmpSidecar::read(Path::new("tests/fixtures/with_description.xmp")).expect("should parse");
    assert!(sidecar.has_description());
    assert_eq!(sidecar.description(), Some("Existing description"));
}

#[test]
fn read_existing_without_description() {
    let sidecar = XmpSidecar::read(Path::new("tests/fixtures/existing.xmp")).expect("should parse");
    assert!(!sidecar.has_description());
    assert_eq!(sidecar.description(), None);
}

#[test]
fn read_malformed_returns_error() {
    let result = XmpSidecar::read(Path::new("tests/fixtures/malformed.xmp"));
    assert!(result.is_err(), "malformed XML should produce an error");
}

#[test]
fn write_and_read_back() {
    let tmp = TempDir::new().unwrap();
    let xmp_path = tmp.path().join("test.xmp");

    let dims = vec![
        "sharpness".to_string(),
        "exposure".to_string(),
        "composition".to_string(),
    ];

    let scores = ScoringResult {
        sharpness: Some(0.90),
        exposure: Some(0.75),
        composition: Some(0.80),
        subject_clarity: None,
        aesthetics: None,
    };

    let overall = scores.overall_score(&dims);

    let mut sidecar = XmpSidecar::new();
    sidecar.set_description("A beautiful mountain landscape");
    sidecar.set_scores(&scores, &dims, overall, "claude/test-model");
    sidecar.set_rating(4);
    sidecar.write(&xmp_path).expect("should write");

    // Read back and verify.
    let read_back = XmpSidecar::read(&xmp_path).expect("should parse written XMP");
    assert!(read_back.has_description());
    assert_eq!(
        read_back.description(),
        Some("A beautiful mountain landscape")
    );
    assert!(read_back.has_scores());

    // Verify the raw XML contains expected elements.
    let raw = std::fs::read_to_string(&xmp_path).unwrap();
    assert!(raw.contains("xmp:Rating=\"4\""));
    assert!(raw.contains("<imgcull:score>"));
    assert!(raw.contains("<imgcull:sharpness>0.90</imgcull:sharpness>"));
    assert!(raw.contains("<imgcull:scored_by>claude/test-model</imgcull:scored_by>"));
    assert!(
        raw.contains("<imgcull:dimensions>sharpness,exposure,composition</imgcull:dimensions>")
    );
}

#[test]
fn backup_creates_bak_file() {
    let tmp = TempDir::new().unwrap();
    let xmp_path = tmp.path().join("photo.xmp");
    std::fs::write(&xmp_path, "<xmp/>").unwrap();

    backup_sidecar(&xmp_path).expect("backup should succeed");

    let bak_path = tmp.path().join("photo.xmp.bak");
    assert!(bak_path.exists());
    assert_eq!(std::fs::read_to_string(bak_path).unwrap(), "<xmp/>");
}

#[test]
fn has_scores_after_set_scores() {
    let dims = vec!["sharpness".to_string()];
    let scores = ScoringResult {
        sharpness: Some(0.85),
        exposure: None,
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };

    let mut sidecar = XmpSidecar::new();
    assert!(!sidecar.has_scores());

    sidecar.set_scores(&scores, &dims, 0.85, "test");
    assert!(sidecar.has_scores());
}
