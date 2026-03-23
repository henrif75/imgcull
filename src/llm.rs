//! LLM provider abstraction layer for imgcull.
//!
//! This module defines the [`LlmClients`] struct and the [`DescriptionProvider`] /
//! [`ScoringProvider`] traits that decouple the pipeline from any specific Rig
//! provider implementation.  Each supported provider (Anthropic, OpenAI, Gemini,
//! DeepSeek, Ollama) has a concrete struct implementing both traits.

use anyhow::{Context, Result};
use rig::OneOrMany;
use rig::completion::message::{ContentFormat, ImageMediaType, UserContent};
use rig::completion::{Message, Prompt};

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

        let description_preamble = prompts.description.system.clone();
        let scoring_preamble = prompts.scoring.system.clone();

        let description_provider =
            build_description_provider(desc_provider_name, desc_config, &description_preamble)?;
        let scoring_provider =
            build_scoring_provider(score_provider_name, score_config, &scoring_preamble)?;

        Ok(Self {
            description_preamble,
            scoring_preamble,
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
        .ok_or_else(|| anyhow::anyhow!("Provider is missing api_key_env in configuration"))?;
    std::env::var(env_var).with_context(|| {
        format!(
            "API key not found. Set the {env_var} environment variable or add it to a .env file."
        )
    })
}

/// Build a user [`Message`] containing an image and a text prompt.
///
/// The image is passed as base64-encoded JPEG data.
fn build_image_message(image_base64: &str, prompt: &str) -> Message {
    let mut content = OneOrMany::one(UserContent::image(
        image_base64,
        Some(ContentFormat::Base64),
        Some(ImageMediaType::JPEG),
        None,
    ));
    content.push(UserContent::text(prompt));
    Message::User { content }
}

/// Extract a JSON object from `text` using brace-depth counting.
///
/// Finds the first `{` and then counts opening/closing braces to locate
/// the matching `}`, returning the full object slice.  This correctly
/// handles nested objects and prose that contains stray braces *after*
/// the JSON value.
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a [`ScoringResult`] from the LLM response text.
///
/// Attempts to extract JSON from the response, handling cases where the
/// model wraps the JSON in markdown code fences or prose.
fn parse_scoring_result(text: &str) -> Result<ScoringResult> {
    // Try direct parse first
    if let Ok(result) = serde_json::from_str::<ScoringResult>(text) {
        return Ok(result);
    }

    // Extract the first complete JSON object using depth-counting
    let json_str = extract_json_object(text)
        .with_context(|| format!("No JSON object found in LLM response: {text}"))?;

    serde_json::from_str::<ScoringResult>(json_str)
        .with_context(|| format!("Failed to parse scoring JSON: {json_str}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoring::ScoringResult;

    fn make_scoring_json() -> &'static str {
        r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5}"#
    }

    fn assert_scoring_result(result: &ScoringResult) {
        assert!((result.sharpness.unwrap() - 0.8).abs() < 1e-9);
        assert!((result.exposure.unwrap() - 0.7).abs() < 1e-9);
        assert!((result.composition.unwrap() - 0.6).abs() < 1e-9);
        assert!((result.subject_clarity.unwrap() - 0.9).abs() < 1e-9);
        assert!((result.aesthetics.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_raw_valid_json() {
        let result = parse_scoring_result(make_scoring_json()).unwrap();
        assert_scoring_result(&result);
    }

    #[test]
    fn test_parse_markdown_fenced_json() {
        let text = format!("```json\n{}\n```", make_scoring_json());
        let result = parse_scoring_result(&text).unwrap();
        assert_scoring_result(&result);
    }

    #[test]
    fn test_parse_json_embedded_in_prose() {
        let text = format!(
            "Here is my analysis of the image.\n\n{}\n\nI hope this helps!",
            make_scoring_json()
        );
        let result = parse_scoring_result(&text).unwrap();
        assert_scoring_result(&result);
    }

    #[test]
    fn test_parse_no_json_returns_error() {
        let err = parse_scoring_result("No JSON here at all.").unwrap_err();
        assert!(err.to_string().contains("No JSON object found"));
    }

    #[test]
    fn test_parse_json_with_nested_braces() {
        // JSON with a nested object; prose after it contains a stray `}`
        let text = r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5, "meta": {"note": "test"}}"#;
        // ScoringResult uses #[serde(default)] so unknown fields need deny_unknown_fields
        // to fail — without it serde ignores extra fields, so this should parse fine.
        let result = parse_scoring_result(text).unwrap();
        assert!((result.sharpness.unwrap() - 0.8).abs() < 1e-9);
    }
}

// ----------------------------------------------------------------
// Anthropic (Claude) provider
// ----------------------------------------------------------------

struct ClaudeDescriptionProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for ClaudeDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = rig::providers::anthropic::ClientBuilder::new(&self.api_key).build();
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Claude description request failed")?;
        Ok(response)
    }
}

struct ClaudeScoringProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for ClaudeScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = rig::providers::anthropic::ClientBuilder::new(&self.api_key).build();
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Claude scoring request failed")?;
        parse_scoring_result(&response)
    }
}

// ----------------------------------------------------------------
// OpenAI provider
// ----------------------------------------------------------------

struct OpenAiDescriptionProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for OpenAiDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = rig::providers::openai::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("OpenAI description request failed")?;
        Ok(response)
    }
}

struct OpenAiScoringProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for OpenAiScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = rig::providers::openai::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("OpenAI scoring request failed")?;
        parse_scoring_result(&response)
    }
}

// ----------------------------------------------------------------
// Gemini provider
// ----------------------------------------------------------------

struct GeminiDescriptionProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for GeminiDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = rig::providers::gemini::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Gemini description request failed")?;
        Ok(response)
    }
}

struct GeminiScoringProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for GeminiScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = rig::providers::gemini::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Gemini scoring request failed")?;
        parse_scoring_result(&response)
    }
}

// ----------------------------------------------------------------
// DeepSeek provider
// ----------------------------------------------------------------

struct DeepSeekDescriptionProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for DeepSeekDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = rig::providers::deepseek::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("DeepSeek description request failed")?;
        Ok(response)
    }
}

struct DeepSeekScoringProvider {
    api_key: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for DeepSeekScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = rig::providers::deepseek::Client::new(&self.api_key);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("DeepSeek scoring request failed")?;
        parse_scoring_result(&response)
    }
}

// ----------------------------------------------------------------
// Ollama provider
// ----------------------------------------------------------------

struct OllamaDescriptionProvider {
    base_url: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl DescriptionProvider for OllamaDescriptionProvider {
    async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
        let client = rig::providers::ollama::Client::from_url(&self.base_url);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Ollama description request failed")?;
        Ok(response)
    }
}

struct OllamaScoringProvider {
    base_url: String,
    model: String,
    preamble: String,
}

#[async_trait::async_trait]
impl ScoringProvider for OllamaScoringProvider {
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
        let client = rig::providers::ollama::Client::from_url(&self.base_url);
        let agent = client.agent(&self.model).preamble(&self.preamble).build();

        let msg = build_image_message(image_base64, prompt);
        let response = agent
            .prompt(msg)
            .await
            .context("Ollama scoring request failed")?;
        parse_scoring_result(&response)
    }
}

// ----------------------------------------------------------------
// Builder functions
// ----------------------------------------------------------------

/// Build a [`DescriptionProvider`] for the named provider.
fn build_description_provider(
    name: &str,
    config: &ProviderConfig,
    preamble: &str,
) -> Result<Box<dyn DescriptionProvider + Send + Sync>> {
    match name {
        "claude" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(ClaudeDescriptionProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "openai" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(OpenAiDescriptionProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "gemini" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(GeminiDescriptionProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "deepseek" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(DeepSeekDescriptionProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "ollama" => {
            let base_url = config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let model = config.model.clone();
            Ok(Box::new(OllamaDescriptionProvider {
                base_url,
                model,
                preamble: preamble.to_string(),
            }))
        }
        other => {
            anyhow::bail!("Unsupported description provider: {other}")
        }
    }
}

/// Build a [`ScoringProvider`] for the named provider.
///
/// All providers use the agent approach with JSON response parsing,
/// since Rig's `Extractor` only accepts text prompts and cannot
/// handle multimodal (image + text) input.
fn build_scoring_provider(
    name: &str,
    config: &ProviderConfig,
    preamble: &str,
) -> Result<Box<dyn ScoringProvider + Send + Sync>> {
    match name {
        "claude" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(ClaudeScoringProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "openai" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(OpenAiScoringProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "gemini" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(GeminiScoringProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "deepseek" => {
            let api_key = resolve_api_key(config)?;
            let model = config.model.clone();
            Ok(Box::new(DeepSeekScoringProvider {
                api_key,
                model,
                preamble: preamble.to_string(),
            }))
        }
        "ollama" => {
            let base_url = config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let model = config.model.clone();
            Ok(Box::new(OllamaScoringProvider {
                base_url,
                model,
                preamble: preamble.to_string(),
            }))
        }
        other => {
            anyhow::bail!("Unsupported scoring provider: {other}")
        }
    }
}
