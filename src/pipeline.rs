//! Concurrent image processing pipeline.
//!
//! [`run_pipeline`] iterates over a list of image paths, preprocesses each
//! image, optionally calls the LLM for a description and/or scores, and writes
//! an XMP sidecar.  Parallelism is bounded by a semaphore whose width is read
//! from the `concurrency` field in [`Config`]'s default settings.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, warn};

use crate::config::{Config, Prompts};
use crate::llm::LlmClients;
use crate::preprocessing::preprocess_image;
use crate::retry::retry_with_backoff;
use crate::scoring::score_to_stars;
use crate::summary::RunSummary;
use crate::xmp::{SidecarPath, XmpSidecar, backup_sidecar};

/// Options that control which pipeline stages run and how output is handled.
pub struct PipelineOptions {
    /// Skip the description stage entirely.
    pub no_description: bool,
    /// Skip writing the XMP star rating.
    pub no_rating: bool,
    /// Backup existing `.xmp` sidecars before overwriting.
    pub backup: bool,
    /// Re-process images even if a description or scores already exist.
    pub force: bool,
    /// Print what would be done without actually writing any files.
    pub dry_run: bool,
    /// Only write scores; skip the description stage.
    pub score_only: bool,
    /// Only write descriptions; skip the scoring stage.
    pub describe_only: bool,
}

/// Process a batch of images through the description and scoring pipeline.
///
/// For each image in `images` the pipeline:
/// 1. Acquires a semaphore slot to limit concurrent LLM requests.
/// 2. Preprocesses the image (decode, resize, base64-encode).
/// 3. Optionally calls the description provider and stores the result.
/// 4. Optionally calls the scoring provider, clamps scores, and stores the result.
/// 5. Backs up the existing XMP sidecar if `options.backup` is set.
/// 6. Writes the updated XMP sidecar to disk.
///
/// A progress bar is displayed during processing and a [`RunSummary`] is
/// printed to stderr when all tasks have completed.
///
/// # Errors
///
/// Returns an error if any Tokio task panics (i.e. if `handle.await` fails).
/// Per-image errors are logged via `warn!` / `error!` and counted in the
/// summary rather than propagating.
pub async fn run_pipeline(
    images: Vec<PathBuf>,
    config: &Config,
    prompts: &Prompts,
    clients: Arc<LlmClients>,
    options: PipelineOptions,
) -> Result<()> {
    let summary = Arc::new(RunSummary::new());
    let semaphore = Arc::new(Semaphore::new(config.default_settings.concurrency));
    let dimensions = config.scoring.dimensions.clone();

    let pb = ProgressBar::new(images.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("[{pos}/{len}] {msg} {bar:40.cyan/blue} {eta}").unwrap(),
    );

    summary
        .total
        .store(images.len(), std::sync::atomic::Ordering::Relaxed);

    let mut handles = Vec::new();

    for image_path in images {
        let sem = semaphore.clone();
        let clients = clients.clone();
        let summary = summary.clone();
        let dims = dimensions.clone();
        let prompts_rendered = prompts.render_scoring_prompt(&dimensions, &prompts.guidelines);
        let desc_template = prompts.description.template.clone();
        let score_provider_name = config.default_settings.scoring_provider.clone();
        let score_model_name = config
            .providers
            .get(&config.default_settings.scoring_provider)
            .map(|p| p.model.clone())
            .unwrap_or_default();
        let pb = pb.clone();
        let options_no_desc = options.no_description || options.score_only;
        let options_no_score = options.describe_only;
        let options_no_rating = options.no_rating || options.describe_only;
        let options_backup = options.backup;
        let options_force = options.force;
        let options_dry_run = options.dry_run;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let filename = image_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            pb.set_message(filename.clone());

            if options_dry_run {
                pb.println(format!("  [dry-run] Would process: {filename}"));
                pb.inc(1);
                return;
            }

            // Preprocess
            let preprocessed = match preprocess_image(&image_path) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Skipping {filename}: {e}");
                    summary
                        .skipped_unreadable
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    pb.inc(1);
                    return;
                }
            };

            // Read existing sidecar
            let sidecar_path = SidecarPath::for_image(&image_path);
            let mut sidecar = if sidecar_path.exists() {
                match XmpSidecar::read(&sidecar_path) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Malformed sidecar for {filename}, creating new: {e}");
                        XmpSidecar::new()
                    }
                }
            } else {
                XmpSidecar::new()
            };

            let needs_description =
                !options_no_desc && (options_force || !sidecar.has_description());
            let needs_scoring = !options_no_score && (options_force || !sidecar.has_scores());

            // Description
            if needs_description {
                let b64 = preprocessed.base64.clone();
                let tmpl = desc_template.clone();
                let c = clients.clone();
                let desc_result =
                    retry_with_backoff(2, || async { c.describe(&b64, &tmpl).await }).await;
                match desc_result {
                    Ok(desc) => {
                        sidecar.set_description(&desc);
                        summary
                            .described
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        warn!("Description failed for {filename}: {e}");
                        summary
                            .skipped_llm_error
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            } else if !options_no_desc && sidecar.has_description() {
                summary
                    .skipped_existing_description
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            // Scoring
            if needs_scoring {
                let b64 = preprocessed.base64.clone();
                let prompt = prompts_rendered.clone();
                let c = clients.clone();
                let score_result =
                    retry_with_backoff(3, || async { c.score(&b64, &prompt).await }).await;
                match score_result {
                    Ok(mut scores) => {
                        scores.clamp();
                        let overall = scores.overall_score(&dims);
                        let provider_info = format!("{}/{}", score_provider_name, score_model_name);
                        sidecar.set_scores(&scores, &dims, overall, &provider_info);

                        if !options_no_rating {
                            sidecar.set_rating(score_to_stars(overall));
                        }

                        let stars = score_to_stars(overall);
                        let star_display =
                            "★".repeat(stars as usize) + &"☆".repeat(5 - stars as usize);
                        pb.println(format!("  {filename} {star_display} ({overall:.2})"));

                        summary.record_score(&filename, overall);
                    }
                    Err(e) => {
                        warn!("Scoring failed for {filename}: {e}");
                        summary
                            .skipped_llm_error
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }

            // Backup & write
            if options_backup
                && sidecar_path.exists()
                && let Err(e) = backup_sidecar(&sidecar_path)
            {
                error!("Backup failed for {filename}: {e}");
            }

            if let Err(e) = sidecar.write(&sidecar_path) {
                error!("Failed to write sidecar for {filename}: {e}");
                eprintln!(
                    "XMP write failed for {filename} — description and scores lost. Re-run with --force to retry."
                );
            }

            pb.inc(1);
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    pb.finish_and_clear();
    summary.display();

    Ok(())
}
