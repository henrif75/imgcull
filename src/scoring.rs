//! Scoring types and star-mapping utilities for image quality assessment.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Result of LLM quality scoring across multiple dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ScoringResult {
    /// Focus quality, motion blur, camera shake (0.0--1.0).
    #[serde(default)]
    pub sharpness: Option<f64>,
    /// Exposure accuracy (0.0--1.0).
    #[serde(default)]
    pub exposure: Option<f64>,
    /// Framing and visual balance (0.0--1.0).
    #[serde(default)]
    pub composition: Option<f64>,
    /// Subject identification and separation (0.0--1.0).
    #[serde(default)]
    pub subject_clarity: Option<f64>,
    /// Emotional impact and storytelling (0.0--1.0).
    #[serde(default)]
    pub aesthetics: Option<f64>,
    /// Free-text narrative critique from the LLM.
    #[serde(default)]
    pub critique: Option<String>,
}

impl ScoringResult {
    /// Look up a scoring dimension by name.
    ///
    /// Recognised names: `"sharpness"`, `"exposure"`, `"composition"`,
    /// `"subject_clarity"`, `"aesthetics"`.  Returns `None` for unknown
    /// names or when the dimension has not been set.
    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "sharpness" => self.sharpness,
            "exposure" => self.exposure,
            "composition" => self.composition,
            "subject_clarity" => self.subject_clarity,
            "aesthetics" => self.aesthetics,
            _ => None,
        }
    }

    /// Compute the equal-weighted average of the requested dimensions.
    ///
    /// Only dimensions that are both listed in `dimensions` *and* present
    /// (`Some`) contribute.  Returns `0.0` when no matching dimensions are
    /// available.
    pub fn overall_score(&self, dimensions: &[String]) -> f64 {
        let (sum, count) = dimensions
            .iter()
            .filter_map(|d| self.get(d))
            .fold((0.0f64, 0usize), |(s, n), v| (s + v, n + 1));
        if count == 0 { 0.0 } else { sum / count as f64 }
    }

    /// Clamp every present score to the 0.0--1.0 range.
    pub fn clamp(&mut self) {
        if let Some(v) = &mut self.sharpness {
            *v = v.clamp(0.0, 1.0);
        }
        if let Some(v) = &mut self.exposure {
            *v = v.clamp(0.0, 1.0);
        }
        if let Some(v) = &mut self.composition {
            *v = v.clamp(0.0, 1.0);
        }
        if let Some(v) = &mut self.subject_clarity {
            *v = v.clamp(0.0, 1.0);
        }
        if let Some(v) = &mut self.aesthetics {
            *v = v.clamp(0.0, 1.0);
        }
    }
}

/// Map a 0.0--1.0 score to a 1--5 star rating.
///
/// | Score range | Stars |
/// |-------------|-------|
/// | 0.00--0.20  | 1     |
/// | 0.21--0.40  | 2     |
/// | 0.41--0.60  | 3     |
/// | 0.61--0.80  | 4     |
/// | 0.81--1.00  | 5     |
pub fn score_to_stars(score: f64) -> u8 {
    match score {
        s if s <= 0.20 => 1,
        s if s <= 0.40 => 2,
        s if s <= 0.60 => 3,
        s if s <= 0.80 => 4,
        _ => 5,
    }
}
