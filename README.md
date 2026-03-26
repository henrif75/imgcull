# imgcull

> AI-powered image culling — score and describe your photos using vision LLMs, with results written directly to XMP sidecar files for Adobe Lightroom Classic.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/henrif75/imgcull#license)
[![Build Status](https://img.shields.io/github/actions/workflow/status/henrif75/imgcull/ci.yml?branch=main)](https://github.com/henrif75/imgcull/actions)
[![Version](https://img.shields.io/badge/version-0.1.0-orange.svg)](https://github.com/henrif75/imgcull/releases)

![imgcull — AI-powered image culling](docs/banner.svg)

---

## 🔍 Why imgcull?

After a shoot, photographers face hundreds (or thousands) of raw files to review. Manual culling is slow and subjective. imgcull uses vision-capable LLMs to evaluate each image across technical and aesthetic dimensions — sharpness, exposure, composition, subject clarity, aesthetics — and writes scores and descriptions into the `.xmp` sidecar files that Lightroom already reads. Your star ratings and captions appear automatically in your library, without touching the original files.

## ✨ Features

- **Multi-provider support** — works with Claude (Anthropic), GPT-4o (OpenAI), Gemini, DeepSeek, and local models via Ollama
- **XMP-native output** — scores and descriptions are written to `.xmp` sidecar files; existing Lightroom metadata (collections, color labels, crops) is preserved
- **Non-destructive** — never modifies original image files; optional `.xmp.bak` backup before any update
- **Configurable dimensions** — score on any subset of: sharpness, exposure, composition, subject clarity, aesthetics
- **Parallel processing** — bounded concurrency with configurable worker count for fast batch runs
- **Dry-run mode** — preview which files would be processed without making any LLM calls or file writes
- **Separate describe / score commands** — run description-only, scoring-only, or both in one pass

## 🛠 Tech Stack

Built with **Rust** (edition 2024), **Tokio** (async runtime), **Rig** (LLM provider abstraction), and **clap** (CLI).

---

## 📦 Installation

### Prerequisites

- Rust `>=` 1.85 (edition 2024)
- An API key for at least one supported provider, **or** a running [Ollama](https://ollama.ai) instance for local inference

### Build from source

```bash
git clone https://github.com/henrif75/imgcull.git
cd imgcull
cargo build --release
# Binary is at ./target/release/imgcull
```

### First-time setup

```bash
imgcull init
```

This creates `~/Library/Application Support/imgcull/config.toml`, `prompts.toml`, and `.env.example`. Copy `.env.example` to `.env` and fill in your API key:

```bash
cp ~/Library/"Application Support"/imgcull/.env.example ~/Library/"Application Support"/imgcull/.env
# Edit ~/Library/"Application Support"/imgcull/.env and set e.g. ANTHROPIC_API_KEY=sk-...
```

---

## 🚀 Usage

### Score and describe a folder of photos

```bash
imgcull score ~/Photos/2026-03-shoot/
```

This processes every supported image (JPEG, CR2, NEF, ARW, DNG, ORF), writes a scene description to `dc:description` and a 1–5 star rating to `xmp:Rating` in each `.xmp` sidecar.

### Generate descriptions only

```bash
imgcull describe ~/Photos/2026-03-shoot/
```

### Score only (no description)

```bash
imgcull score --no-description ~/Photos/2026-03-shoot/
```

### Use a specific provider

```bash
imgcull score --provider openai ~/Photos/2026-03-shoot/
imgcull score --description-provider claude --scoring-provider ollama ~/Photos/
```

### Preview without making any LLM calls

```bash
imgcull score --dry-run ~/Photos/2026-03-shoot/
```

### Advanced options

| Flag | Description |
|------|-------------|
| `--provider NAME` | Override both description and scoring provider |
| `--description-provider NAME` | Override description provider only |
| `--scoring-provider NAME` | Override scoring provider only |
| `--dimensions a,b,c` | Score on a custom subset of dimensions |
| `--concurrency N` | Max parallel LLM requests (default: 4) |
| `--backup` | Back up existing `.xmp` to `.xmp.bak` before modifying |
| `--force` | Re-process images that already have scores/descriptions |
| `--no-rating` | Skip writing star rating to `xmp:Rating` |
| `--dry-run` | Show what would be processed; no LLM calls, no writes |
| `--log PATH` | Write detailed log to file |
| `--verbose` / `-v` | Verbose terminal output |
| `--quiet` / `-q` | Only show errors |

### Configuration

imgcull reads `~/Library/Application Support/imgcull/config.toml`. Example:

```toml
[default]
description_provider = "claude"
scoring_provider     = "claude"
concurrency          = 8

[providers.claude]
model       = "claude-opus-4-5"
api_key_env = "ANTHROPIC_API_KEY"

[providers.ollama]
model    = "llava"
base_url = "http://localhost:11434"

[scoring]
dimensions = ["sharpness", "exposure", "composition", "subject_clarity", "aesthetics"]
```

Custom prompts can be edited in `~/Library/Application Support/imgcull/prompts.toml`.

---

## 🤝 Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

- Bug reports and feature requests → [GitHub Issues](https://github.com/henrif75/imgcull/issues)
- Questions → [GitHub Discussions](https://github.com/henrif75/imgcull/discussions)

When contributing code, run the full check suite before submitting a PR:

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

---

## 📄 License

Distributed under the **MIT OR Apache-2.0** license. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE) for details.
