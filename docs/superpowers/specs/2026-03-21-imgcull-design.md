# imgcull — AI-Powered Image Culling CLI Tool

## Overview

`imgcull` is a Rust CLI tool that processes a batch of image files, using a vision-capable LLM to:

1. **Generate scene descriptions** — written to the standard `dc:description` XMP field
2. **Score image quality** (0.0–1.0) across configurable dimensions — written to a custom `imgcull:*` XMP namespace
3. **Map the overall score to a 1–5 star rating** — written to `xmp:Rating` for instant Lightroom Classic sorting

The tool is designed for photographers who need to cull large sets of images efficiently. Scores and descriptions are stored in XMP sidecar files alongside the originals, making them non-destructive and compatible with Lightroom Classic's Library module.

## Architecture

Monolithic pipeline with a single async binary:

```
CLI (clap)
  → File Discovery (walk paths, filter by extension)
  → Metadata Reader (check existing XMP sidecars, decide what work is needed)
  → Concurrency Controller (tokio semaphore, bounded to --concurrency N)
  → LLM Provider via Rig (Claude / OpenAI / Ollama)
  → XMP Sidecar Writer (write/merge .xmp files)
```

### LLM Provider Layer (Rig)

The tool uses the `rig-core` crate to abstract over LLM providers. Two Rig agents are built at startup:

- **Description agent**: configured from `description_provider` in config, with a preamble tuned for scene description
- **Scoring agent**: configured from `scoring_provider` in config, with a preamble tuned for quality evaluation and structured JSON output

**Same-provider optimization**: if both agents use the same provider and model, a single combined agent is built instead, making one LLM request per image (saving tokens and latency). When providers differ, two separate requests are made per image.

### Concurrency and Resilience

- Configurable concurrency via `--concurrency N` (default: 4), implemented with a `tokio::sync::Semaphore`
- Automatic retry with exponential backoff on rate limits (429): max 3 retries, 1s → 2s → 4s
- Other API errors: retry once, then warn and skip
- Invalid JSON from LLM: retry once with stricter prompt, then warn and skip
- Batch-resilient: a single image failure never stops the run

## Supported Formats

- **JPEG**: `.jpg`, `.jpeg`
- **RAW**: `.cr2` (Canon), `.nef` (Nikon), `.arw` (Sony), `.dng` (Adobe), `.orf` (Olympus)

Unsupported extensions are warned and skipped. Expandable to PNG/WebP/HEIC in the future.

## XMP Sidecar Format

Each processed image gets a sidecar file alongside it. For `IMG_1234.jpg`, the sidecar is `IMG_1234.jpg.xmp`.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmlns:imgcull="http://imgcull.dev/ns/1.0/">

      <!-- Scene description (standard Dublin Core — Lightroom reads this) -->
      <dc:description>
        <rdf:Alt>
          <rdf:li xml:lang="x-default">
            A golden retriever running through shallow surf on a beach at sunset,
            with warm orange light reflecting off the water.
          </rdf:li>
        </rdf:Alt>
      </dc:description>

      <!-- Star rating mapped from overall score (standard XMP — Lightroom reads this) -->
      <xmp:Rating>4</xmp:Rating>

      <!-- Overall quality score -->
      <imgcull:score>0.82</imgcull:score>

      <!-- Individual dimension scores -->
      <imgcull:sharpness>0.90</imgcull:sharpness>
      <imgcull:exposure>0.85</imgcull:exposure>
      <imgcull:composition>0.75</imgcull:composition>
      <imgcull:subject_clarity>0.80</imgcull:subject_clarity>
      <imgcull:aesthetics>0.78</imgcull:aesthetics>

      <!-- Metadata about the scoring run -->
      <imgcull:scored_at>2026-03-21T16:30:00Z</imgcull:scored_at>
      <imgcull:scored_by>claude/claude-sonnet-4-20250514</imgcull:scored_by>
      <imgcull:dimensions>sharpness,exposure,composition,subject_clarity,aesthetics</imgcull:dimensions>

    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

### Field Details

| Field | Namespace | Purpose |
|-------|-----------|---------|
| `dc:description` | Dublin Core (standard) | Scene description. Lightroom displays this in the Metadata panel. Skipped if already present (unless `--force`). |
| `xmp:Rating` | XMP Core (standard) | 1–5 star rating mapped from overall score. Enabled by default, disable with `--no-rating`. |
| `imgcull:score` | Custom | Overall quality score 0.0–1.0 (weighted average of dimensions). |
| `imgcull:sharpness` | Custom | Focus quality, motion blur, camera shake. |
| `imgcull:exposure` | Custom | Exposure accuracy, blown highlights, crushed shadows. |
| `imgcull:composition` | Custom | Framing, rule of thirds, leading lines, balance. |
| `imgcull:subject_clarity` | Custom | Subject identification and background separation. |
| `imgcull:aesthetics` | Custom | Emotional impact, mood, storytelling. |
| `imgcull:scored_at` | Custom | ISO 8601 timestamp of when scoring was performed. |
| `imgcull:scored_by` | Custom | Provider/model that produced the scores. |
| `imgcull:dimensions` | Custom | Which dimensions were evaluated in this run. |

### Star Rating Mapping

| Score Range | Stars |
|-------------|-------|
| 0.00 – 0.20 | 1 |
| 0.21 – 0.40 | 2 |
| 0.41 – 0.60 | 3 |
| 0.61 – 0.80 | 4 |
| 0.81 – 1.00 | 5 |

### Sidecar Merge Behavior

- If a sidecar already exists (e.g., created by Lightroom), `imgcull` merges its fields into the existing file rather than overwriting.
- If an existing sidecar is malformed XML, a warning is logged and a new sidecar is written (no merge attempted).

## Configuration

### Config File

Location: `~/.config/imgcull/config.toml`

```toml
[default]
concurrency = 4
description_provider = "claude"
scoring_provider = "claude"
set_rating = true

[providers.claude]
api_key = "sk-ant-..."
model = "claude-sonnet-4-20250514"

[providers.openai]
api_key = "sk-..."
model = "gpt-4o"

[providers.ollama]
base_url = "http://localhost:11434"
model = "llava"

[scoring]
dimensions = ["sharpness", "exposure", "composition", "subject_clarity", "aesthetics"]
```

### Prompts File

Location: `~/.config/imgcull/prompts.toml`

```toml
[combined]
system = "You are an expert photography critic."
template = """
Analyze this image and return a JSON object with the following structure:

{
  "description": "A concise scene description (1-3 sentences). Describe the subject, setting, lighting, and mood.",
  "scores": {
    {{dimensions}}
  }
}

Scoring guidelines:
{{guidelines}}

Return ONLY valid JSON, no markdown fencing, no explanation.
"""

[description_only]
system = "You are a concise photography describer."
template = """
Describe this photograph in 1-3 sentences. Include the subject, setting,
lighting conditions, and mood. Be concise and factual.
"""

[scoring_only]
system = "You are an expert photography critic."
template = """
Analyze this image and return quality scores as JSON.
{{scoring_section}}
"""

[guidelines]
sharpness = "Is the subject in focus? Is there unwanted motion blur or camera shake?"
exposure = "Is the image well-exposed? Are highlights blown or shadows crushed?"
composition = "Does the framing guide the eye? Balance, rule of thirds, leading lines."
subject_clarity = "Is the main subject obvious and well-separated from the background?"
aesthetics = "Overall emotional impact, mood, storytelling, wow factor."
```

`{{dimensions}}` and `{{guidelines}}` placeholders are filled at runtime based on which dimensions are enabled. Users can fully customize prompts. A `--prompts <PATH>` CLI flag allows pointing to an alternative prompts file.

### Precedence

CLI flag > config file > built-in default.

## CLI Interface

```
imgcull score [OPTIONS] <PATHS>...

Arguments:
  <PATHS>...                     Image files or directories to process

Options:
  --provider <NAME>              Override both description and scoring provider
  --description-provider <NAME>  Override description provider only
  --scoring-provider <NAME>      Override scoring provider only
  --concurrency <N>              Max parallel LLM requests [default: 4]
  --dimensions <LIST>            Comma-separated dimensions to score
  --no-description               Skip description generation
  --no-rating                    Don't write star rating to xmp:Rating
  --force                        Re-process even if already scored/described
  --dry-run                      Show what would be processed without calling LLMs
  --log <PATH>                   Write detailed log to file
  --prompts <PATH>               Use alternative prompts file
  -v, --verbose                  Verbose terminal output
  -q, --quiet                    Only show errors
  -h, --help                     Print help
  -V, --version                  Print version

imgcull describe [OPTIONS] <PATHS>...
  (Same options as score, but only generates descriptions — no scoring)

imgcull init
  (Creates default config.toml and prompts.toml at ~/.config/imgcull/)
```

## Logging

| Flag combination | Terminal (stderr) | Log file |
|------------------|-------------------|----------|
| (default) | Progress bar + warnings | No file |
| `--log run.log` | Progress bar + warnings | DEBUG-level structured log |
| `--verbose` | Progress bar + debug output | No file |
| `--verbose --log run.log` | Progress bar + debug output | DEBUG-level structured log |
| `--quiet` | Errors only | No file |
| `--quiet --log run.log` | Errors only | DEBUG-level structured log |

The log file captures full LLM request/response payloads at DEBUG level, useful for debugging prompt effectiveness.

## Error Handling

| Error | Behavior |
|-------|----------|
| Unsupported file extension | Warn and skip |
| File not readable (permissions, corrupt) | Warn and skip |
| LLM rate limit (429) | Retry with exponential backoff (max 3 retries, 1s → 2s → 4s) |
| LLM API error (500, timeout) | Retry once, then warn and skip |
| LLM returns invalid JSON | Retry once with stricter prompt, then warn and skip |
| Score out of range | Clamp to 0.0–1.0, log warning |
| XMP sidecar write failure | Error and skip (print LLM JSON to stderr to avoid data loss) |
| Existing sidecar parse failure | Warn, write new sidecar (no merge) |
| Config file missing | Use built-in defaults |
| Config file malformed | Hard error, exit with message |
| Prompts file missing | Use built-in default prompts |
| Prompts file malformed | Hard error, exit with message |
| No API key for selected provider | Hard error before processing starts |

### End-of-Run Summary

```
imgcull: 187/200 images processed
  ✓ 185 scored (avg: 0.64, best: IMG_4521.jpg 0.97)
  ✓ 170 described (17 already had descriptions)
  ⚠ 13 skipped (8 unsupported format, 3 LLM errors, 2 unreadable)
```

## Crate Dependencies

| Crate | Purpose |
|-------|---------|
| `rig-core` | LLM provider abstraction (Claude, OpenAI, Ollama) |
| `clap` (derive) | CLI argument parsing |
| `tokio` | Async runtime for concurrent processing |
| `serde` / `serde_json` | JSON serialization for LLM responses |
| `toml` | Config and prompts file parsing |
| `xmp_toolkit` or `quick-xml` | XMP sidecar read/write/merge |
| `indicatif` | Progress bars and spinners |
| `base64` | Image encoding for LLM vision APIs |
| `anyhow` | Application-level error handling |
| `tracing` / `tracing-subscriber` / `tracing-appender` | Structured logging with multi-output support |
| `dirs` | Platform-appropriate config directory resolution |

### XMP Library Decision

- **`xmp_toolkit`**: Bindings to Adobe's XMP SDK. Understands XMP semantics natively (namespaces, alt-text, merging). Has a C dependency.
- **`quick-xml`**: Pure Rust XML parser. We'd handle XMP semantics manually.
- **Recommendation**: Start with `xmp_toolkit`. Fall back to `quick-xml` if the C dependency causes build issues.

## Future Considerations (Out of Scope)

- `imgcull rank` subcommand for querying/sorting scored images
- PNG / WebP / HEIC support
- Configurable dimension weights for overall score
- Lightroom Classic plugin to display custom `imgcull:*` fields
