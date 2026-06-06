//! Concurrent image processing pipeline.
//!
//! [`run_pipeline`] iterates over a list of image paths, preprocesses each
//! image, optionally calls the LLM for a description and/or scores, and writes
//! an XMP sidecar.  Parallelism is bounded by a semaphore whose width is read
//! from the `concurrency` field in [`Config`]'s default settings.

use crate::attach_progress_bar;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, warn};

use crate::config::{Config, Prompts};
use crate::llm::LlmClients;
use crate::preprocessing::preprocess_image;
use crate::retry::retry_with_backoff;
use crate::scoring::score_to_stars;
use crate::summary::RunSummary;
use crate::xmp::{XmpSidecar, backup_sidecar, sidecar_for_image};

/// Options that control which pipeline stages run and how output is handled.
#[derive(Debug, Clone, Copy)]
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

    let pb = ProgressBar::new(images.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("[{pos}/{len}] {msg} {bar:40.cyan/blue} {eta}").unwrap(),
    );
    // Route tracing warn/error output above the progress bar.
    attach_progress_bar(pb.clone());

    summary
        .total
        .store(images.len(), std::sync::atomic::Ordering::Relaxed);

    // Compute run-level constants once before the loop.  Arc-wrapped so that
    // each spawn only bumps a refcount instead of cloning the full String.
    let prompts_rendered: Arc<str> = prompts.render_scoring_prompt(&prompts.guidelines).into();
    let desc_template: Arc<str> = prompts.description.template.clone().into();
    // `provider/model` string stamped into every scored sidecar.  Identical for
    // the whole run, so build it once rather than re-formatting per image.
    let score_provider_name = &config.default_settings.scoring_provider;
    let score_model = config
        .providers
        .get(score_provider_name)
        .map(|p| p.model.as_str())
        .unwrap_or_default();
    let provider_info: Arc<str> = format!("{score_provider_name}/{score_model}").into();

    let mut handles = Vec::new();

    for image_path in images {
        let sem = semaphore.clone();
        let clients = clients.clone();
        let summary = summary.clone();
        let prompts_rendered = prompts_rendered.clone();
        let desc_template = desc_template.clone();
        let provider_info = provider_info.clone();
        let pb = pb.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let filename = image_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            pb.set_message(filename.clone());

            if options.dry_run {
                pb.println(format!("  [dry-run] Would process: {filename}"));
                pb.inc(1);
                return;
            }

            // Preprocess on a blocking worker — fs read, RAW decode, resize
            // and JPEG re-encode are all CPU/IO bound and would otherwise
            // stall tokio workers.
            let preprocess_path = image_path.clone();
            let preprocessed =
                match tokio::task::spawn_blocking(move || preprocess_image(&preprocess_path)).await
                {
                    Ok(Ok(p)) => p,
                    Ok(Err(e)) => {
                        warn!("Skipping {filename}: {e}");
                        summary
                            .skipped_unreadable
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        pb.inc(1);
                        return;
                    }
                    Err(e) => {
                        error!("Preprocess task panicked for {filename}: {e}");
                        summary
                            .skipped_unreadable
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        pb.inc(1);
                        return;
                    }
                };

            debug!(
                "{filename}: preprocessed ({}KB base64{})",
                preprocessed.base64.len() / 1024,
                if preprocessed.was_resized {
                    ", resized"
                } else {
                    ""
                }
            );

            // Read existing sidecar
            let sidecar_path = sidecar_for_image(&image_path);
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

            sidecar.set_original_filename(&filename);

            let needs_description =
                !options.no_description && (options.force || !sidecar.has_description());
            let needs_scoring = !options.describe_only && (options.force || !sidecar.has_scores());
            let mut had_llm_error = false;

            // Description
            if needs_description {
                let desc_result = retry_with_backoff(2, || async {
                    clients.describe(&preprocessed.base64, &desc_template).await
                })
                .await;
                match desc_result {
                    Ok(ref desc) => {
                        debug!("{filename}: description received ({} chars)", desc.len());
                        debug!("{filename}: description = {desc}");
                        sidecar.set_description(desc);
                        summary
                            .described
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        warn!("Description failed for {filename}: {e:#}");
                        had_llm_error = true;
                    }
                }
            } else if !options.no_description && sidecar.has_description() {
                summary
                    .skipped_existing_description
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            // Scoring
            if needs_scoring {
                let score_result = retry_with_backoff(3, || async {
                    clients.score(&preprocessed.base64, &prompts_rendered).await
                })
                .await;
                match score_result {
                    Ok(mut scores) => {
                        scores.clamp();
                        let overall = scores.overall_score();
                        debug!("{filename}: scored {overall:.2} — {scores:?}");
                        if let Some(ref critique) = scores.critique {
                            debug!("{filename}: critique = {critique}");
                            sidecar.set_scoring_response(critique);
                        }
                        if let Some(ref keywords) = scores.keywords
                            && !keywords.is_empty()
                        {
                            sidecar.set_keywords(keywords);
                        }
                        sidecar.set_scores(&scores, overall, &provider_info);

                        let stars = score_to_stars(overall);
                        if !(options.no_rating || options.describe_only) {
                            sidecar.set_rating(stars);
                        }
                        let star_display =
                            "★".repeat(stars as usize) + &"☆".repeat(5 - stars as usize);
                        pb.println(format!("  {filename} {star_display} ({overall:.2})"));

                        summary.record_score(&filename, overall);
                    }
                    Err(e) => {
                        warn!("Scoring failed for {filename}: {e:#}");
                        had_llm_error = true;
                    }
                }
            }

            // Count at most one LLM skip per image, not per failed stage.
            if had_llm_error {
                summary
                    .skipped_llm_error
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            // Backup & write — only when something actually changed.
            if sidecar.is_dirty() {
                if options.backup
                    && sidecar_path.exists()
                    && let Err(e) = backup_sidecar(&sidecar_path)
                {
                    error!("Backup failed for {filename}: {e}");
                }

                if let Err(e) = sidecar.write(&sidecar_path) {
                    error!(
                        "Failed to write sidecar for {filename}: {e} — description and scores lost, re-run with --force to retry"
                    );
                }
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
