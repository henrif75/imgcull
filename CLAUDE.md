# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                          # Build the project
cargo test                           # Run all 49 tests (unit + integration)
cargo test --test xmp_test           # Run a single test file
cargo test test_preprocess_small     # Run a test by name substring
cargo fmt                            # Format all code
cargo clippy -- -D warnings          # Lint with zero-warning policy
cargo doc --no-deps                  # Build docs (must produce zero warnings)
```

**Before every commit**, run: `cargo fmt && cargo clippy -- -D warnings && cargo test`

## Architecture

imgcull is an async Rust CLI tool that sends images to vision LLMs for description and quality scoring, then writes results into XMP sidecar files compatible with Adobe Lightroom Classic.

### Pipeline flow

```
CLI (clap) → File Discovery → Image Preprocessing → [Semaphore gate]
  → LLM Description (2 total attempts) → LLM Scoring (3 total attempts)
  → XMP Sidecar Merge/Write → Progress bar + Summary
```

### Key design decisions

- **Two LLM calls per image**: description (free-text via `DescriptionProvider` trait) and scoring (structured via `ScoringProvider` trait + JSON parse). Always separate — never combined.
- **Rig's `Extractor<T>` cannot handle multimodal input** (images + text). All scoring uses `Agent` prompt → parse JSON response. The `parse_scoring_result()` function in `llm.rs` handles both raw JSON and markdown-fenced JSON from LLMs.
- **XMP merge, not overwrite**: When a sidecar already exists (e.g., from Lightroom), `write()` injects/replaces only imgcull-managed fields (`dc:description`, `xmp:Rating`, `imgcull:*`) and preserves all other XML content. The `raw_content` field on `XmpSidecar` stores the original file for merging.
- **Dirty tracking**: `XmpSidecar` tracks whether any modifications were made. The pipeline skips backup+write for unchanged sidecars.
- **Dry-run skips LLM client construction** entirely — handled in `main.rs` before `LlmClients::new()`, so no API keys are needed.

### Module responsibilities

- `main.rs` — Entry point. Uses `mod cli` (private). Orchestrates config loading, CLI overrides, and dispatches to pipeline or init.
- `lib.rs` — Crate root. Re-exports all modules as `pub mod`. Contains `setup_logging()`.
- `llm.rs` — `DescriptionProvider` / `ScoringProvider` traits + 10 concrete provider structs (5 providers × 2 traits). Providers create a fresh Rig client per call.
- `pipeline.rs` — `run_pipeline()` spawns a `tokio::spawn` per image, bounded by `Arc<Semaphore>`. Each task preprocesses, calls LLMs with retry, writes XMP.
- `xmp.rs` — XMP sidecar read/merge/write using string manipulation (not a full XML DOM). Uses `quick-xml` only for validation.
- `preprocessing.rs` — JPEG passthrough, RAW preview extraction (SOI/EOI marker scan), resize to 2048px max, base64 encode.
- `config.rs` — TOML config + prompts loading with defaults. `Config::default()` and `Prompts::default()` provide built-in fallbacks.
- `scoring.rs` — `ScoringResult` struct with 5 hardcoded dimension fields (`Option<f64>`). Derives `JsonSchema` for Rig compatibility.

### Provider abstraction

`LlmClients` holds two boxed trait objects. The `build_*_provider()` functions in `llm.rs` match on provider name strings ("claude", "openai", "gemini", "deepseek", "ollama") and construct the appropriate Rig client. Adding a new provider means: add a struct pair, implement both traits, add match arms.

### Rig crate notes (rig-core 0.33)

- All providers: `Client::new(key)?` returns `Result<Client>` — must propagate with `?`
- Anthropic: `rig::providers::anthropic::Client::new(&key)?`
- OpenAI/Gemini/DeepSeek: `Client::new(&key)?`
- Ollama: `Client::builder().api_key(Nothing).base_url(&url).build()?` (no API key)
- `agent()` is on the `CompletionClient` trait — must be imported: `use rig::client::CompletionClient`
- Image messages: `UserContent::image_base64(data, Some(ImageMediaType::JPEG), None)` (3 args)
- Import paths: `rig::client::{CompletionClient, Nothing}`, `rig::completion::message::{ImageMediaType, UserContent}`, `rig::completion::{Message, Prompt}`, `rig::OneOrMany`

## Project conventions

- **Rust 2024 edition** — uses `let chains` in `if` expressions, `is_none_or()`, etc.
- **All public items require `///` doc comments** — verified by `cargo doc --no-deps` producing zero warnings.
- **Integration tests live in `tests/`**, unit tests in `#[cfg(test)] mod tests` blocks within source files.
- **API keys are never stored in config** — resolved from environment variables via `dotenvy`. The `api_key_env` field in provider config names the env var.
- **Config precedence**: CLI flag > config file (`~/Library/Application Support/imgcull/config.toml`) > built-in default.

## Git Workflow

When asked to make changes and commit, always create a feature branch and open a PR unless explicitly told to commit directly to main.

## Testing

Always run `cargo build` and `cargo test` after making Rust code changes before committing.

## Code Standards

When implementing dry-run or preview modes, ensure no side effects occur — no network calls requiring credentials, no file writes that could destroy existing metadata.
