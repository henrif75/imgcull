---
name: add-llm-provider
description: Add a new LLM backend to imgcull. Scaffolds the provider struct, wires up trait impls, registers it in `build_provider()`, and updates the default config.
disable-model-invocation: true
---

# Adding a new LLM provider

Use this skill to add a new vision-capable LLM backend. The pipeline will auto-route to it once it's registered.

## Step 1 — Decide provider type

**API-key provider** (Claude, OpenAI, Gemini, DeepSeek pattern): uses `Client::new(&api_key)?` then `.agent(&model).preamble(&preamble).build()`. Use the `api_key_provider!` macro.

**Custom builder provider** (Ollama pattern): uses `Client::builder()...build()` or needs `base_url` instead of a key. Write a manual struct with trait impls.

Ask the user which pattern the new backend follows, then proceed.

## Step 2 — Verify Rig support

Check whether `rig-core 0.33` actually exposes a provider module for this backend.

```bash
cargo doc --open -p rig-core 2>/dev/null || true
```

Or grep the installed crate:

```bash
find ~/.cargo/registry/src -type d -name providers -path '*rig-core*' 2>/dev/null | head -1 | xargs ls
```

If the provider isn't in Rig, stop and tell the user — they'll need to upgrade Rig or pick a different backend.

## Step 3 — Add the provider

### For API-key providers

In [src/llm.rs](src/llm.rs), find the block of `api_key_provider!` invocations and add one line:

```rust
api_key_provider!(NewProvider, rig::providers::newprovider::Client, "NewProvider");
```

The macro generates a struct that stores a pre-built `Agent<CompletionModel>` bound to the provider's preamble, along with a `new(api_key, model, preamble) -> Result<Self>` constructor. That constructor is what `build_provider()` calls — the client and its connection pool are built once and reused for every LLM call, instead of being rebuilt per image.

Add the match arm in `build_provider()`:

```rust
"newprovider" => Ok(Box::new(NewProvider::new(
    &resolve_api_key(config)?,
    &config.model,
    preamble,
)?)),
```

### For custom builder providers

Write a manual struct with impls for both `DescriptionProvider` and `ScoringProvider`. Use `OllamaProvider` as the reference — it also stores a pre-built `Agent` and exposes a `new(base_url, model, preamble) -> Result<Self>` constructor. Add its match arm in `build_provider()`.

## Step 4 — Register default config

In [src/config.rs](src/config.rs), find the `default_providers()` function where the built-in providers are registered. Add an entry only if the new backend is a vision-capable provider that every user should get by default:

```rust
m.insert(
    "newprovider".to_string(),
    ProviderConfig {
        model: "model-id-here".to_string(),
        api_key_env: Some("NEWPROVIDER_API_KEY".to_string()),
        base_url: None,
    },
);
```

For non-API-key providers, set `api_key_env: None` and provide a `base_url`.

If the backend lacks vision support (see Step 5 below) **do not** add it to `default_providers()` — leave the `build_provider()` match arm in place so users who want it can opt in via a manual `[providers.newprovider]` section in their config, but don't ship it as a default. DeepSeek is the existing example of this pattern.

## Step 5 — Sanity checks

Multimodal (image + text) support:
- Scoring uses `Agent` + manual JSON parse — NOT `Extractor<T>` (Rig 0.33 can't do multimodal extraction).
- Description uses `Agent` with an image in `UserContent::image_base64(data, Some(ImageMediaType::JPEG), None)`.
- If the backend doesn't support vision, it won't work for either call — confirm with the user before continuing.

Imports required (already present in `llm.rs`, verify):
```rust
use rig::client::{CompletionClient, Nothing};
use rig::completion::message::{ImageMediaType, UserContent};
use rig::completion::{Message, Prompt};
use rig::OneOrMany;
```

## Step 6 — Verify

Run the full gate:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps
cargo test
```

Every new `pub` item needs a `///` doc comment — the macro handles this for API-key providers, but manual structs need doc comments added.

## Step 7 — Smoke test

Without making real API calls, verify registration:

```bash
cargo run -- --dry-run --provider newprovider <any-image-dir>
```

Dry-run skips `LlmClients::new()`, so no API key is needed. If the CLI errors on "Unsupported provider", the match arm in `build_provider()` is missing or misspelled.

For a real call, set the env var and run without `--dry-run` on one test image.

## Step 8 — Update docs

- [README.md](README.md) — add to the supported providers list if there is one
- [CLAUDE.md](CLAUDE.md) — update the "Provider abstraction" section if the new provider introduces a non-standard pattern

## Files touched (checklist)

- [ ] `src/llm.rs` — macro invocation (or manual struct) + match arm in `build_provider()`
- [ ] `src/config.rs` — default `ProviderConfig` entry
- [ ] `README.md` — user-facing docs
- [ ] `CLAUDE.md` — only if pattern is non-standard
- [ ] Gate passes: `cargo fmt && cargo clippy -- -D warnings && cargo test`
