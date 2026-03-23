use imgcull::scoring::{ScoringResult, score_to_stars};

fn all_dimensions() -> Vec<String> {
    vec![
        "sharpness".into(),
        "exposure".into(),
        "composition".into(),
        "subject_clarity".into(),
        "aesthetics".into(),
    ]
}

#[test]
fn overall_score_all_dimensions() {
    let result = ScoringResult {
        sharpness: Some(0.8),
        exposure: Some(0.6),
        composition: Some(0.7),
        subject_clarity: Some(0.9),
        aesthetics: Some(0.5),
    };
    let score = result.overall_score(&all_dimensions());
    let expected = (0.8 + 0.6 + 0.7 + 0.9 + 0.5) / 5.0;
    assert!((score - expected).abs() < f64::EPSILON);
}

#[test]
fn overall_score_subset_of_dimensions() {
    let result = ScoringResult {
        sharpness: Some(0.8),
        exposure: Some(0.6),
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };
    let dims: Vec<String> = vec!["sharpness".into(), "exposure".into()];
    let score = result.overall_score(&dims);
    let expected = (0.8 + 0.6) / 2.0;
    assert!((score - expected).abs() < f64::EPSILON);
}

#[test]
fn overall_score_no_matching_dimensions() {
    let result = ScoringResult {
        sharpness: None,
        exposure: None,
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };
    assert_eq!(result.overall_score(&all_dimensions()), 0.0);
}

#[test]
fn star_mapping_boundaries() {
    assert_eq!(score_to_stars(0.0), 1);
    assert_eq!(score_to_stars(0.20), 1);
    assert_eq!(score_to_stars(0.21), 2);
    assert_eq!(score_to_stars(0.40), 2);
    assert_eq!(score_to_stars(0.41), 3);
    assert_eq!(score_to_stars(0.60), 3);
    assert_eq!(score_to_stars(0.61), 4);
    assert_eq!(score_to_stars(0.80), 4);
    assert_eq!(score_to_stars(0.81), 5);
    assert_eq!(score_to_stars(1.0), 5);
}

#[test]
fn clamp_out_of_range_values() {
    let mut result = ScoringResult {
        sharpness: Some(1.5),
        exposure: Some(-0.3),
        composition: Some(0.5),
        subject_clarity: None,
        aesthetics: Some(2.0),
    };
    result.clamp();
    assert_eq!(result.sharpness, Some(1.0));
    assert_eq!(result.exposure, Some(0.0));
    assert_eq!(result.composition, Some(0.5));
    assert_eq!(result.subject_clarity, None);
    assert_eq!(result.aesthetics, Some(1.0));
}

#[test]
fn get_by_dimension_name() {
    let result = ScoringResult {
        sharpness: Some(0.8),
        exposure: None,
        composition: Some(0.6),
        subject_clarity: Some(0.9),
        aesthetics: None,
    };
    assert_eq!(result.get("sharpness"), Some(0.8));
    assert_eq!(result.get("exposure"), None);
    assert_eq!(result.get("composition"), Some(0.6));
    assert_eq!(result.get("subject_clarity"), Some(0.9));
    assert_eq!(result.get("aesthetics"), None);
    assert_eq!(result.get("unknown_dimension"), None);
}
