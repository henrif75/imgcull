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
    /// Show a summary report of scored images
    Report(ReportArgs),
    /// Create default config files
    Init,
}

/// Arguments for the report subcommand.
#[derive(clap::Args, Debug)]
pub struct ReportArgs {
    /// Image files or directories to report on.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Output format.
    #[arg(long, value_enum, default_value = "table")]
    pub format: ReportFormat,

    /// Sort order.
    #[arg(long, value_enum, default_value = "score")]
    pub sort: SortBy,

    /// Sort ascending instead of descending.
    #[arg(long)]
    pub asc: bool,
}

/// Output format for the report command.
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum ReportFormat {
    /// Aligned terminal table.
    Table,
    /// Comma-separated values.
    Csv,
}

/// Sort order for the report command.
#[derive(clap::ValueEnum, Debug, Clone)]
pub enum SortBy {
    /// Sort by overall score.
    Score,
    /// Sort by filename.
    Filename,
    /// Sort by star rating.
    Rating,
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
