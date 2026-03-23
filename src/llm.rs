//! LLM provider abstraction layer for imgcull.
//!
//! This module defines the [`LlmClients`] struct and the [`DescriptionProvider`] /
//! [`ScoringProvider`] traits that decouple the pipeline from any specific Rig
//! provider implementation.  Concrete provider wiring is filled in by Task 11.

use anyhow::{Context, Result};

use crate::config::{Config, Prompts, ProviderConfig};
use crate::scoring::ScoringResult;

// ----------------------------------------------------------------
// Public traits
// ----------------------------------------------------------------

/// Describes a single image using a vision-capable LLM.
#[async_trait::async_trait]
pub trait DescriptionProvider: Send + Sync {
    /// Send `image_base64` together with `prompt` to the LLM and return the
    /// model's textual description.
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String>;
}

/// Scores a single image across the configured quality dimensions.
#[async_trait::async_trait]
pub trait ScoringProvider: Send + Sync {
    /// Send `image_base64` together with `prompt` to the LLM and return a
    /// structured [`ScoringResult`].
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult>;
}

// ----------------------------------------------------------------
// LlmClients
// ----------------------------------------------------------------

/// Holds pre-built description and scoring provider instances along with the
/// system-prompt preambles read from the project's prompts configuration.
pub struct LlmClients {
    /// System preamble injected into every description request.
    pub description_preamble: String,

    /// System preamble injected into every scoring request.
    pub scoring_preamble: String,

    description_provider: Box<dyn DescriptionProvider + Send + Sync>,
    scoring_provider: Box<dyn ScoringProvider + Send + Sync>,
}

impl LlmClients {
    /// Construct `LlmClients` from the project [`Config`] and [`Prompts`].
    ///
    /// Looks up the configured description and scoring provider names in
    /// `config.providers`, resolves API keys from the environment, and builds
    /// the provider instances.
    ///
    /// # Errors
    /// Returns an error if a named provider is not present in `config.providers`,
    /// if a required environment variable is missing, or if provider construction
    /// fails.
    pub fn new(config: &Config, prompts: &Prompts) -> Result<Self> {
        let desc_provider_name = &config.default_settings.description_provider;
        let score_provider_name = &config.default_settings.scoring_provider;

        let desc_config = config
            .providers
            .get(desc_provider_name)
            .with_context(|| format!("Unknown description provider: {desc_provider_name}"))?;
        let score_config = config
            .providers
            .get(score_provider_name)
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

    /// Ask the description provider to describe an image.
    ///
    /// `image_base64` must be a standard base64-encoded JPEG or PNG.
    /// `prompt` is the user-facing prompt text (not the system preamble).
    pub async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        self.description_provider
            .describe(image_base64, prompt)
            .await
    }

    /// Ask the scoring provider to score an image.
    ///
    /// `image_base64` must be a standard base64-encoded JPEG or PNG.
    /// `prompt` is the fully-rendered scoring prompt (dimensions + guidelines).
    pub async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        self.scoring_provider.score(image_base64, prompt).await
    }
}

// ----------------------------------------------------------------
// Internal helpers
// ----------------------------------------------------------------

/// Resolve the API key for a provider from the environment.
///
/// Reads the variable named by `provider_config.api_key_env`.  Returns an
/// error with a human-readable message if the variable is not set.
fn resolve_api_key(provider_config: &ProviderConfig) -> Result<String> {
    let env_var = provider_config
        .api_key_env
        .as_deref()
        .unwrap_or("MISSING_API_KEY_ENV");

    std::env::var(env_var).with_context(|| {
        format!(
            "API key not found. Set the {env_var} environment variable or add it to a .env file."
        )
    })
}

/// Build a [`DescriptionProvider`] for the named provider.
///
/// Dispatches on `name` to select the correct Rig client.  The bodies are
/// intentionally left as `todo!()` — Task 11 provides the full implementation.
fn build_description_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn DescriptionProvider + Send + Sync>> {
    match name {
        "claude" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement Anthropic description provider")
        }
        "openai" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement OpenAI description provider")
        }
        "gemini" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement Gemini description provider")
        }
        "deepseek" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement DeepSeek description provider")
        }
        "ollama" => {
            let _base_url = config.base_url.clone();
            let _model = config.model.clone();
            todo!("Implement Ollama description provider")
        }
        other => {
            anyhow::bail!("Unsupported description provider: {other}")
        }
    }
}

/// Build a [`ScoringProvider`] for the named provider.
///
/// Uses Rig's structured-extraction capabilities where available, falling back
/// to raw JSON parsing.  The bodies are intentionally left as `todo!()` —
/// Task 11 provides the full implementation.
fn build_scoring_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Box<dyn ScoringProvider + Send + Sync>> {
    match name {
        "claude" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement Anthropic scoring provider")
        }
        "openai" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement OpenAI scoring provider")
        }
        "gemini" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement Gemini scoring provider")
        }
        "deepseek" => {
            let _api_key = resolve_api_key(config)?;
            let _model = config.model.clone();
            todo!("Implement DeepSeek scoring provider")
        }
        "ollama" => {
            let _base_url = config.base_url.clone();
            let _model = config.model.clone();
            todo!("Implement Ollama scoring provider")
        }
        other => {
            anyhow::bail!("Unsupported scoring provider: {other}")
        }
    }
}
