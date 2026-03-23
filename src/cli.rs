//! Command-line interface definitions for imgcull.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// AI-powered image culling tool.
#[derive(Parser, Debug)]
#[command(name = "imgcull", version, about = "AI-powered image culling tool")]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze images: generate descriptions and quality scores
    Score(ProcessArgs),
    /// Generate scene descriptions only (no scoring)
    Describe(ProcessArgs),
    /// Create default config files
    Init,
}

/// Arguments shared by score and describe subcommands.
#[derive(clap::Args, Debug)]
pub struct ProcessArgs {
    /// Image files or directories to process.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Override both description and scoring provider.
    #[arg(long)]
    pub provider: Option<String>,

    /// Override description provider only.
    #[arg(long)]
    pub description_provider: Option<String>,

    /// Override scoring provider only.
    #[arg(long)]
    pub scoring_provider: Option<String>,

    /// Max parallel LLM requests [default: from config or 4].
    #[arg(long)]
    pub concurrency: Option<usize>,

    /// Comma-separated dimensions to score.
    #[arg(long, value_delimiter = ',')]
    pub dimensions: Option<Vec<String>>,

    /// Skip description generation.
    #[arg(long)]
    pub no_description: bool,

    /// Don't write star rating to xmp:Rating.
    #[arg(long)]
    pub no_rating: bool,

    /// Backup existing .xmp sidecars to .xmp.bak before modifying.
    #[arg(long)]
    pub backup: bool,

    /// Re-process even if already scored/described.
    #[arg(long)]
    pub force: bool,

    /// Show what would be processed without calling LLMs.
    #[arg(long)]
    pub dry_run: bool,

    /// Write detailed log to file.
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Use alternative prompts file.
    #[arg(long)]
    pub prompts: Option<PathBuf>,

    /// Verbose terminal output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Only show errors.
    #[arg(short, long)]
    pub quiet: bool,
}
