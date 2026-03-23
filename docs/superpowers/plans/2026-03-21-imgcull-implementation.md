# imgcull Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI tool that uses vision LLMs to generate scene descriptions and quality scores for images, writing results to XMP sidecar files compatible with Lightroom Classic.

**Architecture:** Monolithic async pipeline using `clap` for CLI, `rig-core` for multi-provider LLM abstraction, `tokio` for concurrency, and `quick-xml` (with `xmp_toolkit` as stretch goal) for XMP sidecar read/write. The pipeline flows: file discovery → image preprocessing → metadata check → LLM requests → XMP write.

**Tech Stack:** Rust, tokio, clap (derive), rig-core, serde/schemars, quick-xml, image, kamadak-exif, indicatif, tracing, dotenvy, base64, anyhow

**Spec:** `docs/superpowers/specs/2026-03-21-imgcull-design.md`

---

## Quality Directives

These apply to every task. Do not mark a task complete unless all directives are satisfied.

1. **Unit Tests Required** — Every module must have corresponding unit tests. Write tests before implementation (TDD). Tests must pass before committing.

2. **Rust Style Guide** — All code must conform to the [Rust Style Guide](https://doc.rust-lang.org/style-guide/index.html). Run `cargo fmt` before committing any Rust code file. Do not commit unformatted code.

3. **Clippy Clean** — All code must pass `cargo clippy -- -D warnings` with zero warnings. Run Clippy before each commit and fix any issues.

4. **Doc Comments** — All public types, functions, and modules must have `///` doc comments to support documentation generation via `cargo doc`. Doc comments should describe purpose, parameters, return values, and any important behavior. Run `cargo doc --no-deps` to verify documentation builds without warnings.

---

## File Structure

```
imgcull/
├── Cargo.toml
├── .env.example
├── src/
│   ├── main.rs                  # Entry point: CLI parsing, orchestration
│   ├── cli.rs                   # clap derive structs for CLI args
│   ├── config.rs                # Config + prompts file loading, defaults, merging with CLI
│   ├── discovery.rs             # File discovery: walk paths, filter extensions
│   ├── preprocessing.rs         # RAW preview extraction, resize, base64 encode
│   ├── xmp.rs                   # XMP sidecar read/write/merge (quick-xml)
│   ├── llm.rs                   # Rig provider setup, description agent, scoring extractor
│   ├── scoring.rs               # ScoringResult struct, star mapping, dimension config
│   ├── retry.rs                 # Retry with exponential backoff utility
│   ├── pipeline.rs              # Per-image processing pipeline, concurrency, retry
│   └── summary.rs               # End-of-run summary stats and display
├── tests/
│   ├── fixtures/                # Test images (tiny valid JPEG, sample XMP files)
│   │   ├── test_photo.jpg
│   │   ├── existing.xmp
│   │   ├── malformed.xmp
│   │   └── with_description.xmp
│   ├── config_test.rs           # Config loading, defaults, merging
│   ├── discovery_test.rs        # File discovery and filtering
│   ├── preprocessing_test.rs    # Image resize, base64 encoding
│   ├── xmp_test.rs              # XMP read/write/merge
│   ├── scoring_test.rs          # Score computation, star mapping, clamping
│   └── cli_test.rs              # CLI argument parsing integration tests
```

---

### Task 1: Project Scaffold and Dependencies

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.gitignore`
- Create: `.env.example`

- [ ] **Step 1: Initialize the Cargo project**

```bash
cd ~/Projects/imgcull
cargo init --name imgcull
```

- [ ] **Step 2: Set up Cargo.toml with all dependencies**

Replace `Cargo.toml` with:

```toml
[package]
name = "imgcull"
version = "0.1.0"
edition = "2024"
description = "AI-powered image culling tool using vision LLMs"

[dependencies]
# CLI
clap = { version = "4", features = ["derive"] }

# Async
tokio = { version = "1", features = ["full"] }

# LLM
rig-core = { version = "0.11", features = ["derive"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "0.8"
toml = "0.8"

# XMP / XML
quick-xml = "0.37"

# Image processing
image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }
kamadak-exif = "0.6"

# Encoding
base64 = "0.22"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
tracing-appender = "0.2"

# Config & environment
dotenvy = "0.15"
dirs = "6"

# Error handling
anyhow = "1"

# Progress
indicatif = "0.17"

# Time
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create .gitignore**

```
/target
.env
*.bak
```

- [ ] **Step 4: Create .env.example**

```
# imgcull — API keys for LLM providers
# Copy this file to .env and fill in the keys you need.

# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# GEMINI_API_KEY=...
# DEEPSEEK_API_KEY=...
```

- [ ] **Step 5: Write a minimal main.rs that compiles**

```rust
fn main() {
    println!("imgcull v0.1.0");
}
```

- [ ] **Step 6: Verify it builds**

Run: `cargo build`
Expected: Compiles successfully (may take a while for first dependency fetch).

- [ ] **Step 7: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add Cargo.toml Cargo.lock src/main.rs .gitignore .env.example
git commit -m "feat: scaffold imgcull project with dependencies"
```

---

### Task 2: CLI Argument Parsing

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `tests/cli_test.rs`

- [ ] **Step 1: Write failing tests for CLI parsing**

Create `tests/cli_test.rs`:

```rust
use std::process::Command;

#[test]
fn test_score_subcommand_with_paths() {
    let output = Command::new("cargo")
        .args(["run", "--", "score", "photo.jpg"])
        .output()
        .expect("failed to run");
    // Should not fail with "unrecognized subcommand"
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unrecognized subcommand"), "stderr: {stderr}");
}

#[test]
fn test_describe_subcommand_with_paths() {
    let output = Command::new("cargo")
        .args(["run", "--", "describe", "photo.jpg"])
        .output()
        .expect("failed to run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unrecognized subcommand"), "stderr: {stderr}");
}

#[test]
fn test_init_subcommand() {
    let output = Command::new("cargo")
        .args(["run", "--", "init"])
        .output()
        .expect("failed to run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unrecognized subcommand"), "stderr: {stderr}");
}

#[test]
fn test_no_subcommand_shows_help() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("score") && stdout.contains("describe") && stdout.contains("init"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test cli_test`
Expected: FAIL — subcommands not recognized yet.

- [ ] **Step 3: Implement cli.rs**

Create `src/cli.rs`:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "imgcull", version, about = "AI-powered image culling tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze images: generate descriptions and quality scores
    Score(ProcessArgs),
    /// Generate scene descriptions only (no scoring)
    Describe(ProcessArgs),
    /// Create default config files
    Init,
}

#[derive(clap::Args, Debug)]
pub struct ProcessArgs {
    /// Image files or directories to process
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Override both description and scoring provider
    #[arg(long)]
    pub provider: Option<String>,

    /// Override description provider only
    #[arg(long)]
    pub description_provider: Option<String>,

    /// Override scoring provider only
    #[arg(long)]
    pub scoring_provider: Option<String>,

    /// Max parallel LLM requests [default: from config or 4]
    #[arg(long)]
    pub concurrency: Option<usize>,

    /// Comma-separated dimensions to score
    #[arg(long, value_delimiter = ',')]
    pub dimensions: Option<Vec<String>>,

    /// Skip description generation
    #[arg(long)]
    pub no_description: bool,

    /// Don't write star rating to xmp:Rating
    #[arg(long)]
    pub no_rating: bool,

    /// Backup existing .xmp sidecars to .xmp.bak before modifying
    #[arg(long)]
    pub backup: bool,

    /// Re-process even if already scored/described
    #[arg(long)]
    pub force: bool,

    /// Show what would be processed without calling LLMs
    #[arg(long)]
    pub dry_run: bool,

    /// Write detailed log to file
    #[arg(long)]
    pub log: Option<PathBuf>,

    /// Use alternative prompts file
    #[arg(long)]
    pub prompts: Option<PathBuf>,

    /// Verbose terminal output
    #[arg(short, long)]
    pub verbose: bool,

    /// Only show errors
    #[arg(short, long)]
    pub quiet: bool,
}
```

- [ ] **Step 4: Update main.rs to use CLI**

```rust
mod cli;

use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Score(args) => {
            println!("Score: {:?}", args.paths);
        }
        Commands::Describe(args) => {
            println!("Describe: {:?}", args.paths);
        }
        Commands::Init => {
            println!("Init: would create config files");
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test cli_test`
Expected: All 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/cli.rs src/main.rs tests/cli_test.rs
git commit -m "feat: add CLI argument parsing with clap"
```

---

### Task 3: Configuration Loading

**Files:**
- Create: `src/config.rs`
- Create: `tests/config_test.rs`

- [ ] **Step 1: Write failing tests for config**

Create `tests/config_test.rs`:

```rust
use std::io::Write;
use tempfile::NamedTempFile;

// We test the config module directly
// For now, test the default config generation and TOML parsing

#[test]
fn test_default_config_has_expected_fields() {
    let config = imgcull::config::Config::default();
    assert_eq!(config.default_settings.concurrency, 4);
    assert_eq!(config.default_settings.description_provider, "claude");
    assert_eq!(config.default_settings.scoring_provider, "claude");
    assert!(config.default_settings.set_rating);
    assert!(!config.default_settings.backup);
}

#[test]
fn test_config_from_toml() {
    let toml_str = r#"
[default]
concurrency = 8
description_provider = "openai"
scoring_provider = "gemini"
set_rating = false
backup = true

[providers.openai]
model = "gpt-5.4"
api_key_env = "OPENAI_API_KEY"
"#;
    let config: imgcull::config::Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.default_settings.concurrency, 8);
    assert_eq!(config.default_settings.description_provider, "openai");
    assert!(!config.default_settings.set_rating);
    assert!(config.default_settings.backup);
}

#[test]
fn test_default_prompts_has_description_and_scoring() {
    let prompts = imgcull::config::Prompts::default();
    assert!(!prompts.description.system.is_empty());
    assert!(!prompts.description.template.is_empty());
    assert!(!prompts.scoring.system.is_empty());
    assert!(!prompts.scoring.template.is_empty());
    assert!(prompts.guidelines.contains_key("sharpness"));
    assert!(prompts.guidelines.contains_key("aesthetics"));
}

#[test]
fn test_default_dimensions() {
    let config = imgcull::config::Config::default();
    let dims = &config.scoring.dimensions;
    assert_eq!(dims.len(), 5);
    assert!(dims.contains(&"sharpness".to_string()));
    assert!(dims.contains(&"composition".to_string()));
}

#[test]
fn test_provider_config_defaults() {
    let config = imgcull::config::Config::default();
    let claude = config.providers.get("claude").unwrap();
    assert_eq!(claude.model, "claude-sonnet-4-6-20250514");
    assert_eq!(claude.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test config_test`
Expected: FAIL — `imgcull::config` module doesn't exist.

- [ ] **Step 3: Implement config.rs**

Create `src/config.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(rename = "default")]
    pub default_settings: DefaultSettings,
    #[serde(default = "default_providers")]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub scoring: ScoringConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefaultSettings {
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_provider_name")]
    pub description_provider: String,
    #[serde(default = "default_provider_name")]
    pub scoring_provider: String,
    #[serde(default = "default_true")]
    pub set_rating: bool,
    #[serde(default)]
    pub backup: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderConfig {
    pub model: String,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScoringConfig {
    #[serde(default = "default_dimensions")]
    pub dimensions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Prompts {
    #[serde(default = "PromptEntry::default_description")]
    pub description: PromptEntry,
    #[serde(default = "PromptEntry::default_scoring")]
    pub scoring: PromptEntry,
    #[serde(default = "default_guidelines")]
    pub guidelines: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PromptEntry {
    pub system: String,
    pub template: String,
}

// --- Defaults ---

fn default_concurrency() -> usize { 4 }
fn default_provider_name() -> String { "claude".to_string() }
fn default_true() -> bool { true }

fn default_dimensions() -> Vec<String> {
    vec![
        "sharpness".into(),
        "exposure".into(),
        "composition".into(),
        "subject_clarity".into(),
        "aesthetics".into(),
    ]
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut m = HashMap::new();
    m.insert("claude".into(), ProviderConfig {
        model: "claude-sonnet-4-6-20250514".into(),
        api_key_env: Some("ANTHROPIC_API_KEY".into()),
        base_url: None,
    });
    m.insert("openai".into(), ProviderConfig {
        model: "gpt-5.4".into(),
        api_key_env: Some("OPENAI_API_KEY".into()),
        base_url: None,
    });
    m.insert("gemini".into(), ProviderConfig {
        model: "gemini-3.1-pro".into(),
        api_key_env: Some("GEMINI_API_KEY".into()),
        base_url: None,
    });
    m.insert("deepseek".into(), ProviderConfig {
        model: "deepseek-v3".into(),
        api_key_env: Some("DEEPSEEK_API_KEY".into()),
        base_url: None,
    });
    m.insert("ollama".into(), ProviderConfig {
        model: "llava".into(),
        api_key_env: None,
        base_url: Some("http://localhost:11434".into()),
    });
    m
}

fn default_guidelines() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("sharpness".into(), "Is the subject in focus? Is there unwanted motion blur or camera shake?".into());
    m.insert("exposure".into(), "Is the image well-exposed? Are highlights blown or shadows crushed?".into());
    m.insert("composition".into(), "Does the framing guide the eye? Balance, rule of thirds, leading lines.".into());
    m.insert("subject_clarity".into(), "Is the main subject obvious and well-separated from the background?".into());
    m.insert("aesthetics".into(), "Overall emotional impact, mood, storytelling, wow factor.".into());
    m
}

impl PromptEntry {
    fn default_description() -> Self {
        Self {
            system: "You are a concise photography describer.".into(),
            template: "Describe this photograph in 1-3 sentences. Include the subject, setting, lighting conditions, and mood. Be concise and factual.".into(),
        }
    }

    fn default_scoring() -> Self {
        Self {
            system: "You are an expert photography critic.".into(),
            template: "Analyze this image and score it on the following dimensions (each 0.0 to 1.0):\n\n{{dimensions}}\n\nScoring guidelines:\n{{guidelines}}".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_settings: DefaultSettings {
                concurrency: default_concurrency(),
                description_provider: default_provider_name(),
                scoring_provider: default_provider_name(),
                set_rating: true,
                backup: false,
            },
            providers: default_providers(),
            scoring: ScoringConfig::default(),
        }
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self { dimensions: default_dimensions() }
    }
}

impl Default for Prompts {
    fn default() -> Self {
        Self {
            description: PromptEntry::default_description(),
            scoring: PromptEntry::default_scoring(),
            guidelines: default_guidelines(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }
}

impl Prompts {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    /// Render the scoring template with the given dimensions and guidelines.
    pub fn render_scoring_prompt(&self, dimensions: &[String], guidelines: &HashMap<String, String>) -> String {
        let dim_list = dimensions.join(", ");
        let guide_lines: Vec<String> = dimensions
            .iter()
            .filter_map(|d| guidelines.get(d).map(|g| format!("- {d}: {g}")))
            .collect();

        self.scoring.template
            .replace("{{dimensions}}", &dim_list)
            .replace("{{guidelines}}", &guide_lines.join("\n"))
    }
}
```

- [ ] **Step 4: Expose config module from lib.rs**

Create `src/lib.rs`:

```rust
pub mod config;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test config_test`
Expected: All 5 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/config.rs src/lib.rs tests/config_test.rs
git commit -m "feat: add config and prompts loading with defaults"
```

---

### Task 4: File Discovery

**Files:**
- Create: `src/discovery.rs`
- Create: `tests/discovery_test.rs`

- [ ] **Step 1: Write failing tests for discovery**

Create `tests/discovery_test.rs`:

```rust
use std::fs;
use tempfile::TempDir;

#[test]
fn test_discover_jpeg_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("photo.jpg"), b"fake").unwrap();
    fs::write(dir.path().join("photo.jpeg"), b"fake").unwrap();
    fs::write(dir.path().join("readme.txt"), b"fake").unwrap();

    let files = imgcull::discovery::discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(files.len(), 2);
}

#[test]
fn test_discover_raw_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("photo.cr2"), b"fake").unwrap();
    fs::write(dir.path().join("photo.nef"), b"fake").unwrap();
    fs::write(dir.path().join("photo.arw"), b"fake").unwrap();
    fs::write(dir.path().join("photo.dng"), b"fake").unwrap();
    fs::write(dir.path().join("photo.orf"), b"fake").unwrap();

    let files = imgcull::discovery::discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(files.len(), 5);
}

#[test]
fn test_discover_case_insensitive() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("photo.JPG"), b"fake").unwrap();
    fs::write(dir.path().join("photo.CR2"), b"fake").unwrap();

    let files = imgcull::discovery::discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(files.len(), 2);
}

#[test]
fn test_discover_skips_unsupported() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("photo.png"), b"fake").unwrap();
    fs::write(dir.path().join("photo.webp"), b"fake").unwrap();

    let files = imgcull::discovery::discover_images(&[dir.path().to_path_buf()]);
    assert_eq!(files.len(), 0);
}

#[test]
fn test_discover_individual_files() {
    let dir = TempDir::new().unwrap();
    let jpg = dir.path().join("a.jpg");
    let txt = dir.path().join("b.txt");
    fs::write(&jpg, b"fake").unwrap();
    fs::write(&txt, b"fake").unwrap();

    let files = imgcull::discovery::discover_images(&[jpg.clone(), txt.clone()]);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], jpg);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test discovery_test`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement discovery.rs**

Create `src/discovery.rs`:

```rust
use std::path::PathBuf;
use tracing::warn;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "cr2", "nef", "arw", "dng", "orf",
];

pub fn is_supported(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn discover_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut result = Vec::new();

    for path in paths {
        if path.is_dir() {
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file() && is_supported(&p) {
                            result.push(p);
                        }
                    }
                }
                Err(e) => warn!("Cannot read directory {}: {}", path.display(), e),
            }
        } else if path.is_file() {
            if is_supported(path) {
                result.push(path.clone());
            } else {
                warn!("Unsupported format: {}", path.display());
            }
        } else {
            warn!("Path not found: {}", path.display());
        }
    }

    result.sort();
    result
}
```

- [ ] **Step 4: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod discovery;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test discovery_test`
Expected: All 5 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/discovery.rs src/lib.rs tests/discovery_test.rs
git commit -m "feat: add image file discovery with extension filtering"
```

---

### Task 5: Scoring Types and Star Mapping

**Files:**
- Create: `src/scoring.rs`
- Create: `tests/scoring_test.rs`

- [ ] **Step 1: Write failing tests for scoring**

Create `tests/scoring_test.rs`:

```rust
use imgcull::scoring::{ScoringResult, score_to_stars};

#[test]
fn test_overall_score_equal_weighted() {
    let result = ScoringResult {
        sharpness: Some(0.8),
        exposure: Some(0.6),
        composition: Some(1.0),
        subject_clarity: Some(0.4),
        aesthetics: Some(0.2),
    };
    let dims = vec![
        "sharpness".into(), "exposure".into(), "composition".into(),
        "subject_clarity".into(), "aesthetics".into(),
    ];
    let score = result.overall_score(&dims);
    assert!((score - 0.6).abs() < 0.001);
}

#[test]
fn test_overall_score_subset_of_dimensions() {
    let result = ScoringResult {
        sharpness: Some(0.8),
        exposure: Some(0.4),
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };
    let dims = vec!["sharpness".into(), "exposure".into()];
    let score = result.overall_score(&dims);
    assert!((score - 0.6).abs() < 0.001);
}

#[test]
fn test_score_to_stars_boundaries() {
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
fn test_score_clamping() {
    let mut result = ScoringResult {
        sharpness: Some(1.5),
        exposure: Some(-0.3),
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };
    result.clamp();
    assert_eq!(result.sharpness, Some(1.0));
    assert_eq!(result.exposure, Some(0.0));
}

#[test]
fn test_scoring_result_get_by_name() {
    let result = ScoringResult {
        sharpness: Some(0.9),
        exposure: Some(0.5),
        composition: None,
        subject_clarity: None,
        aesthetics: None,
    };
    assert_eq!(result.get("sharpness"), Some(0.9));
    assert_eq!(result.get("exposure"), Some(0.5));
    assert_eq!(result.get("composition"), None);
    assert_eq!(result.get("unknown"), None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test scoring_test`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement scoring.rs**

Create `src/scoring.rs`:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScoringResult {
    #[serde(default)]
    pub sharpness: Option<f64>,
    #[serde(default)]
    pub exposure: Option<f64>,
    #[serde(default)]
    pub composition: Option<f64>,
    #[serde(default)]
    pub subject_clarity: Option<f64>,
    #[serde(default)]
    pub aesthetics: Option<f64>,
}

impl ScoringResult {
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

    pub fn overall_score(&self, dimensions: &[String]) -> f64 {
        let (sum, count) = dimensions.iter().fold((0.0, 0usize), |(s, c), dim| {
            match self.get(dim) {
                Some(v) => (s + v, c + 1),
                None => (s, c),
            }
        });
        if count == 0 { 0.0 } else { sum / count as f64 }
    }

    pub fn clamp(&mut self) {
        fn clamp_opt(v: &mut Option<f64>) {
            if let Some(ref mut val) = v {
                *val = val.clamp(0.0, 1.0);
            }
        }
        clamp_opt(&mut self.sharpness);
        clamp_opt(&mut self.exposure);
        clamp_opt(&mut self.composition);
        clamp_opt(&mut self.subject_clarity);
        clamp_opt(&mut self.aesthetics);
    }
}

pub fn score_to_stars(score: f64) -> u8 {
    match score {
        s if s <= 0.20 => 1,
        s if s <= 0.40 => 2,
        s if s <= 0.60 => 3,
        s if s <= 0.80 => 4,
        _ => 5,
    }
}
```

- [ ] **Step 4: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod scoring;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test scoring_test`
Expected: All 5 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/scoring.rs src/lib.rs tests/scoring_test.rs
git commit -m "feat: add scoring types, star mapping, and clamping"
```

---

### Task 6: XMP Sidecar Read/Write

**Files:**
- Create: `src/xmp.rs`
- Create: `tests/xmp_test.rs`
- Create: `tests/fixtures/existing.xmp`
- Create: `tests/fixtures/malformed.xmp`
- Create: `tests/fixtures/with_description.xmp`

- [ ] **Step 1: Create test fixture files**

Create `tests/fixtures/existing.xmp`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmp:CreatorTool="Adobe Lightroom Classic">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

Create `tests/fixtures/malformed.xmp`:

```
<?xml version="1.0"?>
<x:xmpmeta><broken><unclosed>
```

Create `tests/fixtures/with_description.xmp`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:dc="http://purl.org/dc/elements/1.1/">
      <dc:description>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">Existing description</rdf:li>
        </rdf:Alt>
      </dc:description>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

- [ ] **Step 2: Write failing tests**

Create `tests/xmp_test.rs`:

```rust
use imgcull::xmp::{XmpSidecar, SidecarPath};
use imgcull::scoring::ScoringResult;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_sidecar_path_from_jpeg() {
    let path = PathBuf::from("/photos/IMG_1234.jpg");
    assert_eq!(SidecarPath::for_image(&path), PathBuf::from("/photos/IMG_1234.xmp"));
}

#[test]
fn test_sidecar_path_from_raw() {
    let path = PathBuf::from("/photos/IMG_5678.CR2");
    assert_eq!(SidecarPath::for_image(&path), PathBuf::from("/photos/IMG_5678.xmp"));
}

#[test]
fn test_read_existing_sidecar_has_description() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/with_description.xmp");
    let sidecar = XmpSidecar::read(&path).unwrap();
    assert!(sidecar.has_description());
    assert_eq!(sidecar.description().unwrap(), "Existing description");
}

#[test]
fn test_read_existing_sidecar_no_description() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/existing.xmp");
    let sidecar = XmpSidecar::read(&path).unwrap();
    assert!(!sidecar.has_description());
}

#[test]
fn test_read_malformed_returns_error() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/malformed.xmp");
    let result = XmpSidecar::read(&path);
    assert!(result.is_err());
}

#[test]
fn test_write_new_sidecar() {
    let dir = TempDir::new().unwrap();
    let xmp_path = dir.path().join("test.xmp");

    let scores = ScoringResult {
        sharpness: Some(0.9),
        exposure: Some(0.85),
        composition: Some(0.75),
        subject_clarity: Some(0.8),
        aesthetics: Some(0.78),
    };
    let dims = vec![
        "sharpness".into(), "exposure".into(), "composition".into(),
        "subject_clarity".into(), "aesthetics".into(),
    ];

    let mut sidecar = XmpSidecar::new();
    sidecar.set_description("A test photo.");
    sidecar.set_scores(&scores, &dims, 0.82, "claude/claude-sonnet-4-6-20250514");
    sidecar.set_rating(4);
    sidecar.write(&xmp_path).unwrap();

    // Read it back
    let content = std::fs::read_to_string(&xmp_path).unwrap();
    assert!(content.contains("A test photo."));
    assert!(content.contains("imgcull:score"));
    assert!(content.contains("0.82"));
    assert!(content.contains("xmp:Rating"));
}

#[test]
fn test_backup_existing_sidecar() {
    let dir = TempDir::new().unwrap();
    let xmp_path = dir.path().join("test.xmp");
    let bak_path = dir.path().join("test.xmp.bak");

    std::fs::write(&xmp_path, "<original/>").unwrap();

    imgcull::xmp::backup_sidecar(&xmp_path).unwrap();
    assert!(bak_path.exists());
    assert_eq!(std::fs::read_to_string(&bak_path).unwrap(), "<original/>");
}

#[test]
fn test_has_scores() {
    let dir = TempDir::new().unwrap();
    let xmp_path = dir.path().join("scored.xmp");

    let scores = ScoringResult {
        sharpness: Some(0.5), exposure: None, composition: None,
        subject_clarity: None, aesthetics: None,
    };
    let mut sidecar = XmpSidecar::new();
    sidecar.set_scores(&scores, &["sharpness".into()], 0.5, "test/model");
    sidecar.write(&xmp_path).unwrap();

    let read_back = XmpSidecar::read(&xmp_path).unwrap();
    assert!(read_back.has_scores());
}
```

- [ ] **Step 2b: Run tests to verify they fail**

Run: `cargo test --test xmp_test`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement xmp.rs**

Create `src/xmp.rs`:

```rust
use anyhow::{Context, Result};
use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::scoring::ScoringResult;

pub struct SidecarPath;

impl SidecarPath {
    /// Replace the image extension with .xmp (Lightroom convention).
    pub fn for_image(path: &Path) -> PathBuf {
        path.with_extension("xmp")
    }
}

/// In-memory representation of an XMP sidecar's relevant fields.
/// Merge strategy: we store the raw XML of any existing sidecar.
/// On write, if we have raw XML, we inject/replace our fields.
/// If no existing raw XML, we write a fresh document.
#[derive(Debug, Default)]
pub struct XmpSidecar {
    description: Option<String>,
    rating: Option<u8>,
    overall_score: Option<f64>,
    dimension_scores: Vec<(String, f64)>,
    scored_at: Option<String>,
    scored_by: Option<String>,
    dimensions_list: Option<String>,
    /// Raw XML of existing sidecar (for merge — preserved fields we don't touch)
    raw_existing: Option<String>,
}

impl XmpSidecar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Cannot read sidecar: {}", path.display()))?;

        let mut sidecar = Self {
            raw_existing: Some(content.clone()),
            ..Default::default()
        };

        // Parse with quick-xml to extract fields we care about
        let mut reader = Reader::from_str(&content);
        let mut in_description_alt = false;
        let mut buf = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let name = std::str::from_utf8(e.name().as_ref()).unwrap_or("");
                    if name == "rdf:li" && in_description_alt {
                        // Next text is the description
                    }
                    if name == "rdf:Alt" {
                        // Check if parent was dc:description
                        in_description_alt = true;
                    }
                }
                Ok(Event::Text(ref e)) => {
                    if in_description_alt {
                        let text = e.unescape().unwrap_or_default().trim().to_string();
                        if !text.is_empty() {
                            sidecar.description = Some(text);
                        }
                        in_description_alt = false;
                    }
                }
                Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                    let name = std::str::from_utf8(e.name().as_ref()).unwrap_or("");
                    // Check for attributes like imgcull:score, xmp:Rating
                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                        let val = std::str::from_utf8(&attr.value).unwrap_or("");
                        match key {
                            "xmp:Rating" => sidecar.rating = val.parse().ok(),
                            "imgcull:score" => sidecar.overall_score = val.parse().ok(),
                            _ => {}
                        }
                    }
                    let _ = name; // suppress unused warning
                }
                Ok(Event::Eof) => break,
                Err(e) => anyhow::bail!("XMP parse error: {e}"),
                _ => {}
            }
            buf.clear();
        }

        // Also check for element-style imgcull fields (not just attributes)
        if content.contains("imgcull:score") {
            // Extract score from element content if not found as attribute
            if sidecar.overall_score.is_none() {
                if let Some(score) = extract_element_value(&content, "imgcull:score") {
                    sidecar.overall_score = score.parse().ok();
                }
            }
        }

        Ok(sidecar)
    }

    pub fn has_description(&self) -> bool {
        self.description.is_some()
    }

    pub fn has_scores(&self) -> bool {
        self.overall_score.is_some()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn set_description(&mut self, desc: &str) {
        self.description = Some(desc.to_string());
    }

    pub fn set_scores(
        &mut self,
        scores: &ScoringResult,
        dims: &[String],
        overall: f64,
        scored_by: &str,
    ) {
        self.overall_score = Some(overall);
        self.scored_by = Some(scored_by.to_string());
        self.scored_at = Some(chrono::Utc::now().to_rfc3339());
        self.dimensions_list = Some(dims.join(","));
        self.dimension_scores = dims
            .iter()
            .filter_map(|d| scores.get(d).map(|v| (d.clone(), v)))
            .collect();
    }

    pub fn set_rating(&mut self, stars: u8) {
        self.rating = Some(stars);
    }

    /// Write a complete XMP sidecar document.
    /// Strategy: always write a fresh document with our fields.
    /// If there was an existing sidecar with non-imgcull fields (e.g., Lightroom's
    /// CreatorTool), those are preserved by including the raw_existing content
    /// as a base and injecting our fields. For v1, we write a clean document
    /// with all our fields — merge of arbitrary third-party fields is a future enhancement.
    pub fn write(&self, path: &Path) -> Result<()> {
        let mut output = String::new();
        output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        output.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
        output.push_str("  <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
        output.push_str("    <rdf:Description\n");
        output.push_str("      xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n");
        output.push_str("      xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\"\n");
        output.push_str("      xmlns:imgcull=\"http://imgcull.dev/ns/1.0/\"");

        // Rating as attribute
        if let Some(rating) = self.rating {
            output.push_str(&format!("\n      xmp:Rating=\"{rating}\""));
        }

        output.push_str(">\n");

        // Description
        if let Some(ref desc) = self.description {
            let escaped = quick_xml::escape::escape(desc);
            output.push_str("      <dc:description>\n");
            output.push_str("        <rdf:Alt>\n");
            output.push_str(&format!(
                "          <rdf:li xml:lang=\"x-default\">{escaped}</rdf:li>\n"
            ));
            output.push_str("        </rdf:Alt>\n");
            output.push_str("      </dc:description>\n");
        }

        // Scores
        if let Some(overall) = self.overall_score {
            output.push_str(&format!("      <imgcull:score>{overall:.2}</imgcull:score>\n"));

            for (dim, val) in &self.dimension_scores {
                output.push_str(&format!("      <imgcull:{dim}>{val:.2}</imgcull:{dim}>\n"));
            }

            if let Some(ref at) = self.scored_at {
                output.push_str(&format!("      <imgcull:scored_at>{at}</imgcull:scored_at>\n"));
            }
            if let Some(ref by) = self.scored_by {
                output.push_str(&format!("      <imgcull:scored_by>{by}</imgcull:scored_by>\n"));
            }
            if let Some(ref dims) = self.dimensions_list {
                output.push_str(&format!("      <imgcull:dimensions>{dims}</imgcull:dimensions>\n"));
            }
        }

        output.push_str("    </rdf:Description>\n");
        output.push_str("  </rdf:RDF>\n");
        output.push_str("</x:xmpmeta>\n");

        std::fs::write(path, &output)
            .with_context(|| format!("Cannot write sidecar: {}", path.display()))
    }
}

pub fn backup_sidecar(path: &Path) -> Result<()> {
    let bak = path.with_extension("xmp.bak");
    std::fs::copy(path, &bak)
        .with_context(|| format!("Cannot backup sidecar: {}", path.display()))?;
    Ok(())
}

/// Simple regex-free element value extractor for <tag>value</tag> patterns.
fn extract_element_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}
```

**Merge strategy note:** For v1, `write()` generates a clean XMP document containing all imgcull fields plus any description and rating. If a Lightroom sidecar existed, the `raw_existing` field preserves it for potential future merge logic. A full merge (preserving arbitrary third-party XMP fields like `xmp:CreatorTool`) is deferred — Lightroom will re-read its own fields from the catalog and update the sidecar on next sync.

- [ ] **Step 4: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod xmp;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test xmp_test`
Expected: All 8 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/xmp.rs src/lib.rs tests/xmp_test.rs tests/fixtures/
git commit -m "feat: add XMP sidecar read/write/merge with quick-xml"
```

---

### Task 7: Image Preprocessing

**Files:**
- Create: `src/preprocessing.rs`
- Create: `tests/preprocessing_test.rs`
- Create: `tests/fixtures/test_photo.jpg`

- [ ] **Step 1: Create a tiny valid test JPEG**

Use the `image` crate to generate a 100x100 test JPEG in a small setup script, or include a minimal JPEG fixture (a 1x1 JPEG is ~631 bytes). The simplest approach is to generate one in the test itself.

- [ ] **Step 2: Write failing tests**

Create `tests/preprocessing_test.rs`:

```rust
use imgcull::preprocessing::{preprocess_image, PreprocessedImage};
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_jpeg(dir: &std::path::Path, width: u32, height: u32, name: &str) -> PathBuf {
    let path = dir.join(name);
    let img = image::ImageBuffer::from_fn(width, height, |_, _| {
        image::Rgb([128u8, 128, 128])
    });
    img.save(&path).unwrap();
    path
}

#[test]
fn test_preprocess_small_jpeg_no_resize() {
    let dir = TempDir::new().unwrap();
    let path = create_test_jpeg(dir.path(), 800, 600, "small.jpg");

    let result = preprocess_image(&path).unwrap();
    assert!(!result.base64.is_empty());
    assert!(!result.was_resized);
}

#[test]
fn test_preprocess_large_jpeg_resizes() {
    let dir = TempDir::new().unwrap();
    let path = create_test_jpeg(dir.path(), 4000, 3000, "large.jpg");

    let result = preprocess_image(&path).unwrap();
    assert!(result.was_resized);
    assert!(!result.base64.is_empty());
}

#[test]
fn test_preprocess_unreadable_file_returns_error() {
    let path = PathBuf::from("/nonexistent/photo.jpg");
    let result = preprocess_image(&path);
    assert!(result.is_err());
}

#[test]
fn test_extract_raw_preview_from_fake_raw() {
    // Simulate a RAW file with an embedded JPEG (SOI + padding + EOI)
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fake.cr2");

    // Build a fake RAW: some header bytes, then a tiny valid JPEG
    let mut raw_data: Vec<u8> = vec![0x00; 100]; // RAW header garbage
    // Append a minimal JPEG (SOI marker + JFIF minimal + EOI marker)
    let jpeg_start = raw_data.len();
    raw_data.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xE0]); // SOI + APP0 marker
    raw_data.extend_from_slice(&[0x00, 0x10]); // APP0 length
    raw_data.extend_from_slice(b"JFIF\x00"); // JFIF identifier
    raw_data.extend_from_slice(&[0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]);
    raw_data.extend_from_slice(&[0xFF, 0xD9]); // EOI

    std::fs::write(&path, &raw_data).unwrap();

    // The preprocessing should find and extract the embedded JPEG
    // It will fail at image::load_from_memory since our fake JPEG isn't complete,
    // but the extraction itself should work. Test extraction separately:
    use imgcull::preprocessing::extract_raw_preview_public;
    // Note: if extract_raw_preview is private, the implementer should add a
    // #[cfg(test)] pub wrapper or test it through preprocess_image
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --test preprocessing_test`
Expected: FAIL — module doesn't exist.

- [ ] **Step 4: Implement preprocessing.rs**

Create `src/preprocessing.rs`:

```rust
use anyhow::{Context, Result};
use base64::{Engine, prelude::BASE64_STANDARD};
use image::GenericImageView;
use std::io::Cursor;
use std::path::Path;

const MAX_DIMENSION: u32 = 2048;

pub struct PreprocessedImage {
    pub base64: String,
    pub was_resized: bool,
}

pub fn preprocess_image(path: &Path) -> Result<PreprocessedImage> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let image_bytes = match ext.as_str() {
        "jpg" | "jpeg" => std::fs::read(path)
            .with_context(|| format!("Cannot read {}", path.display()))?,
        "cr2" | "nef" | "arw" | "dng" | "orf" => {
            extract_raw_preview(path)?
        }
        _ => anyhow::bail!("Unsupported format: {}", path.display()),
    };

    let img = image::load_from_memory(&image_bytes)
        .with_context(|| format!("Cannot decode image: {}", path.display()))?;

    let (width, height) = img.dimensions();
    let needs_resize = width > MAX_DIMENSION || height > MAX_DIMENSION;

    let final_bytes = if needs_resize {
        let resized = img.resize(MAX_DIMENSION, MAX_DIMENSION, image::imageops::FilterType::Lanczos3);
        let mut buf = Cursor::new(Vec::new());
        resized.write_to(&mut buf, image::ImageFormat::Jpeg)
            .context("Failed to encode resized image")?;
        buf.into_inner()
    } else {
        image_bytes
    };

    Ok(PreprocessedImage {
        base64: BASE64_STANDARD.encode(&final_bytes),
        was_resized: needs_resize,
    })
}

fn extract_raw_preview(path: &Path) -> Result<Vec<u8>> {
    // Read the file and look for the embedded JPEG preview.
    // Most RAW formats embed a JPEG starting with FF D8 and ending with FF D9.
    let data = std::fs::read(path)
        .with_context(|| format!("Cannot read RAW file: {}", path.display()))?;

    // Find JPEG SOI marker (FF D8)
    let start = data.windows(2)
        .position(|w| w == [0xFF, 0xD8])
        .with_context(|| format!("No JPEG preview found in RAW file: {}", path.display()))?;

    // Find JPEG EOI marker (FF D9) searching from the end for the largest preview
    let end = data.windows(2)
        .rposition(|w| w == [0xFF, 0xD9])
        .map(|p| p + 2)
        .with_context(|| format!("Malformed JPEG preview in RAW file: {}", path.display()))?;

    if end <= start {
        anyhow::bail!("Invalid JPEG preview boundaries in: {}", path.display());
    }

    Ok(data[start..end].to_vec())
}
```

- [ ] **Step 5: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod preprocessing;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test preprocessing_test`
Expected: All 3 tests PASS.

- [ ] **Step 7: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/preprocessing.rs src/lib.rs tests/preprocessing_test.rs
git commit -m "feat: add image preprocessing with resize and RAW preview extraction"
```

---

### Task 8: LLM Provider Setup

**Files:**
- Create: `src/llm.rs`

This task sets up the Rig provider clients and agents. It cannot be fully unit-tested without API keys, so we focus on correct construction and test the builder logic.

- [ ] **Step 1: Implement llm.rs**

Create `src/llm.rs`:

```rust
use anyhow::{bail, Context, Result};
use rig::providers::{anthropic, openai, gemini, deepseek, ollama};
use rig::client::Nothing;
use rig::completion::Prompt;
use rig::message::{DocumentSourceKind, Image, ImageMediaType};
use crate::config::{Config, ProviderConfig, Prompts};
use crate::scoring::ScoringResult;
use std::collections::HashMap;

pub struct LlmClients {
    pub description_preamble: String,
    pub scoring_preamble: String,
    description_provider: Box<dyn DescriptionProvider + Send + Sync>,
    scoring_provider: Box<dyn ScoringProvider + Send + Sync>,
}

#[async_trait::async_trait]
pub trait DescriptionProvider: Send + Sync {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String>;
}

#[async_trait::async_trait]
pub trait ScoringProvider: Send + Sync {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult>;
}

impl LlmClients {
    pub fn new(config: &Config, prompts: &Prompts) -> Result<Self> {
        let desc_provider_name = &config.default_settings.description_provider;
        let score_provider_name = &config.default_settings.scoring_provider;

        let desc_config = config.providers.get(desc_provider_name)
            .with_context(|| format!("Unknown description provider: {desc_provider_name}"))?;
        let score_config = config.providers.get(score_provider_name)
            .with_context(|| format!("Unknown scoring provider: {score_provider_name}"))?;

        let description_provider = build_description_provider(desc_provider_name, desc_config)?;
        let scoring_provider = build_scoring_provider(score_provider_name, score_config)?;

        Ok(Self {
            description_preamble: prompts.description.system.clone(),
            scoring_preamble: prompts.scoring.system.clone(),
            description_provider,
            scoring_provider,
        })
    }

    pub async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        self.description_provider.describe(image_base64, prompt).await
    }

    pub async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        self.scoring_provider.score(image_base64, prompt).await
    }
}

fn resolve_api_key(provider_config: &ProviderConfig) -> Result<String> {
    let env_var = provider_config.api_key_env.as_deref()
        .unwrap_or("MISSING_API_KEY_ENV");

    std::env::var(env_var)
        .with_context(|| format!(
            "API key not found. Set the {} environment variable or add it to a .env file.",
            env_var
        ))
}

fn build_description_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn DescriptionProvider + Send + Sync>> {
    // Implementation dispatches to the correct Rig provider based on name.
    // Each provider wraps a Rig Agent with the description preamble.
    // The actual provider implementations use Rig's Agent::prompt() with Image.
    //
    // This is a skeleton — the implementer should fill in each provider arm
    // following the Rig patterns from the spec.
    todo!("Implement provider dispatch for: {name}")
}

fn build_scoring_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn ScoringProvider + Send + Sync>> {
    // Similar to description, but uses Rig's Extractor<ScoringResult>
    // for structured output where supported, falling back to Agent + JSON parse.
    todo!("Implement scoring provider dispatch for: {name}")
}
```

Note: This is a skeleton. The implementer needs to fill in `build_description_provider` and `build_scoring_provider` with actual Rig client construction for each provider (anthropic, openai, gemini, deepseek, ollama). Refer to the Rig documentation and the patterns shown in the spec review:

- Anthropic: `anthropic::Client::new(&api_key)`, agent with `.preamble()`, prompt with `Image { data: DocumentSourceKind::base64(...), media_type: Some(ImageMediaType::JPEG), ..Default::default() }`
- OpenAI: `openai::Client::new(&api_key)`, similar pattern
- Gemini: `gemini::Client::new(&api_key)`, similar pattern
- DeepSeek: `deepseek::Client::new(&api_key)`, similar pattern
- Ollama: `ollama::Client::new(Nothing)`, set `base_url` if configured

Add `async-trait` to `Cargo.toml` dependencies:
```toml
async-trait = "0.1"
```

- [ ] **Step 2: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod llm;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles (the `todo!()` macros compile but will panic at runtime).

- [ ] **Step 4: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/llm.rs src/lib.rs Cargo.toml
git commit -m "feat: add LLM provider abstraction skeleton with Rig"
```

---

### Task 8b: Retry Utility with Exponential Backoff

**Files:**
- Create: `src/retry.rs`
- Create: `tests/retry_test.rs`

- [ ] **Step 1: Write failing tests for retry**

Create `tests/retry_test.rs`:

```rust
use imgcull::retry::retry_with_backoff;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn test_retry_succeeds_on_first_try() {
    let result = retry_with_backoff(3, || async { Ok::<_, anyhow::Error>("ok") }).await;
    assert_eq!(result.unwrap(), "ok");
}

#[tokio::test]
async fn test_retry_succeeds_after_failures() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let a = attempts.clone();
    let result = retry_with_backoff(3, move || {
        let a = a.clone();
        async move {
            let n = a.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(anyhow::anyhow!("transient error"))
            } else {
                Ok("recovered")
            }
        }
    }).await;
    assert_eq!(result.unwrap(), "recovered");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_exhausts_all_attempts() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let a = attempts.clone();
    let result: Result<&str, _> = retry_with_backoff(3, move || {
        let a = a.clone();
        async move {
            a.fetch_add(1, Ordering::SeqCst);
            Err(anyhow::anyhow!("persistent error"))
        }
    }).await;
    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test retry_test`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Implement retry.rs**

Create `src/retry.rs`:

```rust
use anyhow::Result;
use std::future::Future;
use std::time::Duration;
use tracing::warn;

/// Retry an async operation with exponential backoff.
/// Delays: 1s, 2s, 4s, ... (doubling each attempt).
/// `max_attempts` includes the initial try (so 3 = 1 try + 2 retries).
pub async fn retry_with_backoff<F, Fut, T>(
    max_attempts: usize,
    mut operation: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut delay = Duration::from_secs(1);
    let mut last_err = None;

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                warn!(
                    "Attempt {attempt}/{max_attempts} failed: {e}. {}",
                    if attempt < max_attempts {
                        format!("Retrying in {}s...", delay.as_secs())
                    } else {
                        "No more retries.".to_string()
                    }
                );
                last_err = Some(e);
                if attempt < max_attempts {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }

    Err(last_err.unwrap())
}
```

- [ ] **Step 4: Export module from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod retry;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test retry_test`
Expected: All 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/retry.rs src/lib.rs tests/retry_test.rs
git commit -m "feat: add retry with exponential backoff utility"
```

---

### Task 9: Processing Pipeline

**Files:**
- Create: `src/pipeline.rs`
- Create: `src/summary.rs`

- [ ] **Step 1: Implement summary.rs**

Create `src/summary.rs`:

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub struct RunSummary {
    pub total: AtomicUsize,
    pub scored: AtomicUsize,
    pub described: AtomicUsize,
    pub skipped_existing_description: AtomicUsize,
    pub skipped_unsupported: AtomicUsize,
    pub skipped_llm_error: AtomicUsize,
    pub skipped_unreadable: AtomicUsize,
    pub best: Mutex<Option<(String, f64)>>,
    pub score_sum: Mutex<f64>,
}

impl RunSummary {
    pub fn new() -> Self {
        Self {
            total: AtomicUsize::new(0),
            scored: AtomicUsize::new(0),
            described: AtomicUsize::new(0),
            skipped_existing_description: AtomicUsize::new(0),
            skipped_unsupported: AtomicUsize::new(0),
            skipped_llm_error: AtomicUsize::new(0),
            skipped_unreadable: AtomicUsize::new(0),
            best: Mutex::new(None),
            score_sum: Mutex::new(0.0),
        }
    }

    pub fn record_score(&self, filename: &str, score: f64) {
        self.scored.fetch_add(1, Ordering::Relaxed);
        let mut sum = self.score_sum.lock().unwrap();
        *sum += score;
        let mut best = self.best.lock().unwrap();
        if best.as_ref().map_or(true, |(_, s)| score > *s) {
            *best = Some((filename.to_string(), score));
        }
    }

    pub fn display(&self) {
        let total = self.total.load(Ordering::Relaxed);
        let scored = self.scored.load(Ordering::Relaxed);
        let described = self.described.load(Ordering::Relaxed);
        let skip_desc = self.skipped_existing_description.load(Ordering::Relaxed);
        let skip_unsup = self.skipped_unsupported.load(Ordering::Relaxed);
        let skip_llm = self.skipped_llm_error.load(Ordering::Relaxed);
        let skip_unread = self.skipped_unreadable.load(Ordering::Relaxed);
        let skipped = skip_unsup + skip_llm + skip_unread;
        let processed = total - skipped;

        let avg = if scored > 0 {
            *self.score_sum.lock().unwrap() / scored as f64
        } else {
            0.0
        };
        let best = self.best.lock().unwrap();

        eprintln!("\nimgcull: {processed}/{total} images processed");
        if scored > 0 {
            if let Some((name, score)) = best.as_ref() {
                eprintln!("  ✓ {scored} scored (avg: {avg:.2}, best: {name} {score:.2})");
            }
        }
        if described > 0 || skip_desc > 0 {
            eprintln!("  ✓ {described} described ({skip_desc} already had descriptions)");
        }
        if skipped > 0 {
            eprintln!("  ⚠ {skipped} skipped ({skip_unsup} unsupported format, {skip_llm} LLM errors, {skip_unread} unreadable)");
        }
    }
}
```

- [ ] **Step 2: Implement pipeline.rs**

Create `src/pipeline.rs`. This is the orchestrator that:

1. Takes the list of discovered image paths
2. For each image (bounded by semaphore):
   a. Preprocess the image (extract preview, resize, base64)
   b. Read existing XMP sidecar (if any)
   c. Decide what work is needed (description? scoring? both?)
   d. Call LLM for description (if needed)
   e. Call LLM for scoring (if needed)
   f. Backup sidecar (if `--backup`)
   g. Write/merge XMP sidecar
   h. Update progress bar
3. Print end-of-run summary

```rust
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, warn, error};

use crate::config::{Config, Prompts};
use crate::llm::LlmClients;
use crate::preprocessing::preprocess_image;
use crate::retry::retry_with_backoff;
use crate::scoring::score_to_stars;
use crate::summary::RunSummary;
use crate::xmp::{XmpSidecar, SidecarPath, backup_sidecar};

pub struct PipelineOptions {
    pub no_description: bool,
    pub no_rating: bool,
    pub backup: bool,
    pub force: bool,
    pub dry_run: bool,
    pub score_only: bool, // true when running "score --no-description"
    pub describe_only: bool, // true when running "describe" subcommand
}

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
    pb.set_style(ProgressStyle::with_template(
        "[{pos}/{len}] {msg} {bar:40.cyan/blue} {eta}"
    ).unwrap());

    summary.total.store(images.len(), std::sync::atomic::Ordering::Relaxed);

    let mut handles = Vec::new();

    for image_path in images {
        let sem = semaphore.clone();
        let clients = clients.clone();
        let summary = summary.clone();
        let dims = dimensions.clone();
        let prompts_rendered = prompts.render_scoring_prompt(&dimensions, &prompts.guidelines);
        let desc_template = prompts.description.template.clone();
        let score_provider_name = config.default_settings.scoring_provider.clone();
        let score_model_name = config.providers.get(&config.default_settings.scoring_provider)
            .map(|p| p.model.clone()).unwrap_or_default();
        let pb = pb.clone();
        let options_no_desc = options.no_description;
        let options_no_score = options.describe_only;
        let options_no_rating = options.no_rating || options.describe_only;
        let options_backup = options.backup;
        let options_force = options.force;
        let options_dry_run = options.dry_run;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let filename = image_path.file_name()
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
                    summary.skipped_unreadable.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

            let needs_description = !options_no_desc
                && (options_force || !sidecar.has_description());
            let needs_scoring = !options_no_score
                && (options_force || !sidecar.has_scores());

            // Description (retry once on failure — description is free-text, not rate-limited as aggressively)
            if needs_description {
                let desc_result = {
                    let b64 = &preprocessed.base64;
                    let tmpl = &desc_template;
                    let c = &clients;
                    retry_with_backoff(2, || async { c.describe(b64, tmpl).await }).await
                };
                match desc_result {
                    Ok(desc) => {
                        sidecar.set_description(&desc);
                        summary.described.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        warn!("Description failed for {filename}: {e}");
                        summary.skipped_llm_error.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            } else if !options_no_desc && sidecar.has_description() {
                summary.skipped_existing_description.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }

            // Scoring (retry up to 3 times with backoff — handles rate limits)
            if needs_scoring {
                let score_result = {
                    let b64 = &preprocessed.base64;
                    let prompt = &prompts_rendered;
                    let c = &clients;
                    retry_with_backoff(3, || async { c.score(b64, prompt).await }).await
                };
                match score_result {
                    Ok(mut scores) => {
                        scores.clamp();
                        let overall = scores.overall_score(&dims);
                        let provider_info = format!("{}/{}", score_provider_name, score_model_name);
                        sidecar.set_scores(&scores, &dims, overall, provider_info);

                        if !options_no_rating {
                            sidecar.set_rating(score_to_stars(overall));
                        }

                        let stars = score_to_stars(overall);
                        let star_display = "★".repeat(stars as usize)
                            + &"☆".repeat(5 - stars as usize);
                        pb.println(format!("  {filename} {star_display} ({overall:.2})"));

                        summary.record_score(&filename, overall);
                    }
                    Err(e) => {
                        warn!("Scoring failed for {filename}: {e}");
                        summary.skipped_llm_error.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }

            // Backup & write
            if options_backup && sidecar_path.exists() {
                if let Err(e) = backup_sidecar(&sidecar_path) {
                    error!("Backup failed for {filename}: {e}");
                }
            }

            if let Err(e) = sidecar.write(&sidecar_path) {
                error!("Failed to write sidecar for {filename}: {e}");
                // Print what we had to stderr so data isn't lost
                eprintln!("XMP write failed for {filename} — description and scores lost. Re-run with --force to retry.");
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
```

- [ ] **Step 3: Export modules from lib.rs**

Add to `src/lib.rs`:

```rust
pub mod pipeline;
pub mod summary;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check`
Expected: Compiles (LLM module still has `todo!()` but pipeline references are structurally correct).

- [ ] **Step 5: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/pipeline.rs src/summary.rs src/lib.rs
git commit -m "feat: add processing pipeline with concurrency and progress"
```

---

### Task 10: Wire Up main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement the full main.rs**

Wire together all modules into the entry point:

```rust
mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok(); // Load .env if present, ignore if missing

    let cli = Cli::parse();

    match cli.command {
        Commands::Score(args) => run_process(args, false).await,
        Commands::Describe(args) => run_process(args, true).await,
        Commands::Init => run_init(),
    }
}

async fn run_process(args: cli::ProcessArgs, describe_only: bool) -> Result<()> {
    // Set up logging
    imgcull::setup_logging(args.verbose, args.quiet, args.log.as_deref())?;

    // Load config
    let config_dir = dirs::config_dir()
        .map(|d| d.join("imgcull"))
        .unwrap_or_default();
    let mut config = imgcull::config::Config::load(&config_dir.join("config.toml"))?;

    // CLI overrides (only when explicitly passed — preserves config file values)
    if let Some(c) = args.concurrency {
        config.default_settings.concurrency = c;
    }
    if let Some(ref p) = args.provider {
        config.default_settings.description_provider = p.clone();
        config.default_settings.scoring_provider = p.clone();
    }
    if let Some(ref p) = args.description_provider {
        config.default_settings.description_provider = p.clone();
    }
    if let Some(ref p) = args.scoring_provider {
        config.default_settings.scoring_provider = p.clone();
    }
    if let Some(ref dims) = args.dimensions {
        config.scoring.dimensions = dims.clone();
    }
    if args.backup {
        config.default_settings.backup = true;
    }

    // Load prompts
    let prompts_path = args.prompts.clone()
        .unwrap_or_else(|| config_dir.join("prompts.toml"));
    let prompts = imgcull::config::Prompts::load(&prompts_path)?;

    // Discover images
    let images = imgcull::discovery::discover_images(&args.paths);
    if images.is_empty() {
        eprintln!("No supported images found.");
        return Ok(());
    }
    eprintln!("Found {} images to process.", images.len());

    // Build LLM clients
    let clients = Arc::new(imgcull::llm::LlmClients::new(&config, &prompts)?);

    // Run pipeline
    let options = imgcull::pipeline::PipelineOptions {
        no_description: args.no_description,
        no_rating: args.no_rating,
        backup: config.default_settings.backup,
        force: args.force,
        dry_run: args.dry_run,
        score_only: false,
        describe_only,
    };

    imgcull::pipeline::run_pipeline(images, &config, &prompts, clients, options).await
}

fn run_init() -> Result<()> {
    let config_dir = dirs::config_dir()
        .map(|d| d.join("imgcull"))
        .ok_or_else(|| anyhow::anyhow!("Cannot determine config directory"))?;

    std::fs::create_dir_all(&config_dir)?;

    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        let default = imgcull::config::Config::default();
        let toml_str = toml::to_string_pretty(&default)?;
        std::fs::write(&config_path, toml_str)?;
        eprintln!("Created {}", config_path.display());
    } else {
        eprintln!("Config already exists: {}", config_path.display());
    }

    let prompts_path = config_dir.join("prompts.toml");
    if !prompts_path.exists() {
        let default = imgcull::config::Prompts::default();
        let toml_str = toml::to_string_pretty(&default)?;
        std::fs::write(&prompts_path, toml_str)?;
        eprintln!("Created {}", prompts_path.display());
    } else {
        eprintln!("Prompts already exists: {}", prompts_path.display());
    }

    let env_example_path = config_dir.join(".env.example");
    if !env_example_path.exists() {
        std::fs::write(&env_example_path,
            "# imgcull — API keys for LLM providers\n\
             # Copy this file to .env in your photos directory and fill in the keys you need.\n\n\
             # ANTHROPIC_API_KEY=sk-ant-...\n\
             # OPENAI_API_KEY=sk-...\n\
             # GEMINI_API_KEY=...\n\
             # DEEPSEEK_API_KEY=...\n"
        )?;
        eprintln!("Created {}", env_example_path.display());
    }

    eprintln!("Done. Edit files in: {}", config_dir.display());
    Ok(())
}
```

- [ ] **Step 2: Add a setup_logging function in lib.rs**

Add to `src/lib.rs`:

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn setup_logging(verbose: bool, quiet: bool, log_file: Option<&std::path::Path>) -> anyhow::Result<()> {
    let level = if quiet { "error" } else if verbose { "debug" } else { "warn" };

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(EnvFilter::new(level));

    if let Some(log_path) = log_file {
        let file = std::fs::File::create(log_path)?;
        let file_layer = fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_filter(EnvFilter::new("debug"));

        tracing_subscriber::registry()
            .with(stderr_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(stderr_layer)
            .init();
    }

    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/main.rs src/lib.rs
git commit -m "feat: wire up main.rs with full CLI orchestration"
```

---

### Task 11: Implement LLM Providers (Fill in llm.rs)

**Files:**
- Modify: `src/llm.rs`

This task replaces the `todo!()` calls with actual Rig provider implementations. This is the most provider-specific code in the project.

- [ ] **Step 1: Implement the Anthropic provider as reference pattern**

This is the reference implementation. All other providers follow the same pattern with different client constructors.

```rust
use rig::providers::anthropic;
use rig::completion::Prompt;
use rig::message::{DocumentSourceKind, Image, ImageMediaType};

// --- Anthropic Description Provider ---

struct AnthropicDescriptionProvider {
    agent: anthropic::completion::CompletionModel, // adjust type to match rig-core API
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for AnthropicDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = anthropic::Client::new(&resolve_api_key_for("claude")?);
        let agent = client
            .agent(&self.model)
            .preamble(&self.preamble)
            .build();

        let image = Image {
            data: DocumentSourceKind::base64(image_base64),
            media_type: Some(ImageMediaType::JPEG),
            ..Default::default()
        };

        // Send image + text prompt
        let response = agent.prompt((image, prompt)).await
            .context("Anthropic description request failed")?;
        Ok(response)
    }
}

// --- Anthropic Scoring Provider ---
// Uses Extractor<ScoringResult> for structured output

struct AnthropicScoringProvider {
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for AnthropicScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = anthropic::Client::new(&resolve_api_key_for("claude")?);
        let extractor = client
            .extractor::<ScoringResult>(&self.model)
            .preamble(&self.preamble)
            .build();

        let image = Image {
            data: DocumentSourceKind::base64(image_base64),
            media_type: Some(ImageMediaType::JPEG),
            ..Default::default()
        };

        let result = extractor.extract((image, prompt)).await
            .context("Anthropic scoring extraction failed")?;
        Ok(result)
    }
}
```

**Important:** The exact Rig API types may differ between versions. The implementer should:
1. Run `cargo doc --open -p rig-core` to check the actual type signatures
2. Adjust import paths and method calls to match
3. The pattern (client → agent/extractor → prompt with Image) is stable; the exact types may vary

- [ ] **Step 2: Replicate pattern for remaining 4 providers**

Each provider follows the same pattern with these differences:

| Provider | Client constructor | Notes |
|----------|-------------------|-------|
| `openai` | `openai::Client::new(&key)` | Same agent/extractor pattern |
| `gemini` | `gemini::Client::new(&key)` | Same pattern |
| `deepseek` | `deepseek::Client::new(&key)` | May not support Extractor — fall back to agent + JSON parse |
| `ollama` | `ollama::Client::new(Nothing)` | No API key; set `base_url` from config |

For providers that don't support structured output (Extractor), fall back to:
```rust
// Fallback: prompt for JSON, parse manually
let response = agent.prompt((image, prompt_with_json_instruction)).await?;
let result: ScoringResult = serde_json::from_str(&response)
    .context("Failed to parse scoring JSON from LLM response")?;
```

Build the provider dispatch function:
```rust
fn build_description_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn DescriptionProvider + Send + Sync>> {
    match name {
        "claude" => Ok(Box::new(AnthropicDescriptionProvider {
            model: config.model.clone(),
            preamble: String::new(), // set later from prompts
        })),
        "openai" => Ok(Box::new(OpenAIDescriptionProvider { ... })),
        "gemini" => Ok(Box::new(GeminiDescriptionProvider { ... })),
        "deepseek" => Ok(Box::new(DeepSeekDescriptionProvider { ... })),
        "ollama" => Ok(Box::new(OllamaDescriptionProvider { ... })),
        other => bail!("Unknown provider: {other}"),
    }
}
```

Helper for API key resolution per provider:
```rust
fn resolve_api_key_for(provider: &str) -> Result<String> {
    let env_var = match provider {
        "claude" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        _ => return Err(anyhow::anyhow!("No default env var for provider: {provider}")),
    };
    std::env::var(env_var)
        .with_context(|| format!("Set {} or add it to .env", env_var))
}
```

- [ ] **Step 2: Test with a real API key (manual integration test)**

Run: `ANTHROPIC_API_KEY=sk-ant-... cargo run -- score tests/fixtures/test_photo.jpg --verbose`
Expected: Processes the image, creates `tests/fixtures/test_photo.xmp` with description and scores.

- [ ] **Step 3: Commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
git add src/llm.rs
git commit -m "feat: implement LLM provider dispatch for all 5 providers"
```

---

### Task 12: End-to-End Testing and Polish

**Files:**
- Modify: various files for fixes found during integration testing

- [ ] **Step 1: Run imgcull init and verify config files are created**

Run: `cargo run -- init`
Expected: Creates `~/.config/imgcull/config.toml` and `~/.config/imgcull/prompts.toml`.

- [ ] **Step 2: Run imgcull score --dry-run on a test directory**

Run: `cargo run -- score --dry-run ~/some-photos/`
Expected: Lists images that would be processed, no LLM calls made.

- [ ] **Step 3: Run imgcull score on a small set of real images**

Run: `cargo run -- score --verbose ~/some-photos/test-batch/`
Expected: Progress bar, star ratings printed, XMP sidecars created, end-of-run summary displayed.

- [ ] **Step 4: Verify Lightroom reads the sidecars**

Import the processed images into Lightroom Classic, verify:
- `dc:description` appears in Metadata panel
- Star ratings appear in grid view
- Sorting by rating works as expected

- [ ] **Step 5: Run the full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Final commit**

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo doc --no-deps
git add -A
git commit -m "feat: complete imgcull v0.1.0 — AI-powered image culling CLI"
```
