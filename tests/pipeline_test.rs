//! Integration tests for the processing pipeline.
//!
//! These tests use an Ollama provider config pointing at an unreachable
//! address: constructing `LlmClients` for Ollama needs no API key and makes
//! no network calls, and the scenarios below never reach an LLM stage.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use imgcull::config::{Config, DefaultSettings, Prompts, ProviderConfig};
use imgcull::llm::LlmClients;
use imgcull::pipeline::{PipelineOptions, run_pipeline};
use imgcull::scoring::ScoringResult;
use imgcull::xmp::XmpSidecar;
use tempfile::TempDir;

/// Config whose description and scoring providers are both Ollama at an
/// unreachable local address — client construction succeeds offline.
fn offline_config() -> Config {
    let mut providers = HashMap::new();
    providers.insert(
        "ollama".to_string(),
        ProviderConfig {
            model: "test-model".to_string(),
            api_key_env: None,
            base_url: Some("http://127.0.0.1:9".to_string()),
        },
    );
    Config {
        default_settings: DefaultSettings {
            description_provider: "ollama".to_string(),
            scoring_provider: "ollama".to_string(),
            ..Default::default()
        },
        providers,
    }
}

fn default_options() -> PipelineOptions {
    PipelineOptions {
        no_description: false,
        no_rating: false,
        backup: false,
        force: false,
        describe_only: false,
    }
}

/// Write a sidecar that already has a description and scores (but no
/// original filename) next to `image_path`.
fn write_complete_sidecar(image_path: &Path) {
    let scores = ScoringResult {
        sharpness: Some(0.9),
        exposure: Some(0.8),
        ..Default::default()
    };
    let mut sidecar = XmpSidecar::new();
    sidecar.set_description("already described");
    sidecar.set_scores(&scores, 0.85, "test/model");
    sidecar
        .write(&image_path.with_extension("xmp"))
        .expect("should write sidecar");
}

/// When the sidecar already carries a description and scores, no LLM stage
/// runs and the pipeline must skip preprocessing entirely: an unreadable
/// image file is not an error, and the run still completes far enough to
/// stamp the original filename into the sidecar.
#[tokio::test]
async fn pipeline_skips_preprocess_when_sidecar_is_complete() {
    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("photo.jpg");
    // Garbage bytes: preprocessing this file would fail.
    std::fs::write(&image, b"not a real jpeg").unwrap();
    write_complete_sidecar(&image);

    let config = offline_config();
    let prompts = Prompts::default();
    let clients = Arc::new(LlmClients::new(&config, &prompts).expect("offline client build"));

    run_pipeline(
        vec![image.clone()],
        &config,
        &prompts,
        clients,
        default_options(),
    )
    .await
    .expect("pipeline should not fail");

    let sidecar = XmpSidecar::read(&image.with_extension("xmp")).expect("should re-read");
    assert_eq!(sidecar.description(), Some("already described"));
    assert!(sidecar.has_scores());
    // Only reachable if the unreadable image was never preprocessed.
    assert_eq!(sidecar.original_filename(), Some("photo.jpg"));
}

/// With --force the LLM stages are needed again, so preprocessing runs and
/// an unreadable image is skipped before the sidecar is ever rewritten.
#[tokio::test]
async fn pipeline_force_still_fails_unreadable_images_early() {
    let tmp = TempDir::new().unwrap();
    let image = tmp.path().join("photo.jpg");
    std::fs::write(&image, b"not a real jpeg").unwrap();
    write_complete_sidecar(&image);
    let sidecar_path = image.with_extension("xmp");
    let before = std::fs::read_to_string(&sidecar_path).unwrap();

    let config = offline_config();
    let prompts = Prompts::default();
    let clients = Arc::new(LlmClients::new(&config, &prompts).expect("offline client build"));

    let options = PipelineOptions {
        force: true,
        ..default_options()
    };
    run_pipeline(vec![image.clone()], &config, &prompts, clients, options)
        .await
        .expect("pipeline should not fail");

    let after = std::fs::read_to_string(&sidecar_path).unwrap();
    assert_eq!(
        before, after,
        "unreadable image must leave sidecar untouched"
    );
}
