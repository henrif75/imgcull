//! LLM provider abstraction layer for imgcull.
//!
//! This module defines the [`LlmClients`] struct and the [`DescriptionProvider`] /
//! [`ScoringProvider`] traits that decouple the pipeline from any specific Rig
//! provider implementation.  Each supported provider (Anthropic, OpenAI, Gemini,
//! DeepSeek, Ollama) has a concrete struct implementing both traits.

use anyhow::{Context, Result};
use rig::client::{CompletionClient, Nothing};
use rig::completion::message::{AssistantContent, ImageMediaType, UserContent};
use rig::completion::{CompletionModel, Message};

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
    ///
    /// The result may include a `critique` field with a narrative analysis.
    async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult>;
}

// ----------------------------------------------------------------
// LlmClients
// ----------------------------------------------------------------

/// Holds pre-built description and scoring provider instances.
pub struct LlmClients {
    description_provider: Box<dyn DualProvider>,
    scoring_provider: Box<dyn DualProvider>,
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

        let description_provider =
            build_provider(desc_provider_name, desc_config, &prompts.description.system)?;
        let scoring_provider =
            build_provider(score_provider_name, score_config, &prompts.scoring.system)?;

        Ok(Self {
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
    ///
    /// The result may include a `critique` field with a narrative analysis.
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
    Message::User {
        content: vec![
            UserContent::image_base64(image_base64, Some(ImageMediaType::JPEG), None),
            UserContent::text(prompt),
        ],
    }
}

/// Extract a JSON object from `text` using brace-depth counting.
///
/// Finds the first `{` and then counts opening/closing braces to locate
/// the matching `}`, returning the full object slice.  Correctly handles:
/// - nested objects,
/// - prose that contains stray braces after the JSON value,
/// - `{` / `}` appearing inside JSON string values (tracked with an
///   in-string state machine that honours backslash escapes).
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in text[start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match ch {
                '\\' => escape = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
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
        r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5, "keywords": ["portrait", "natural light", "outdoors"]}"#
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
    fn test_parse_json_without_keywords_backward_compat() {
        let json = r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5}"#;
        let result = parse_scoring_result(json).unwrap();
        assert!((result.sharpness.unwrap() - 0.8).abs() < 1e-9);
        assert!(result.keywords.is_none());
    }

    #[test]
    fn test_parse_json_with_keywords() {
        let result = parse_scoring_result(make_scoring_json()).unwrap();
        let keywords = result.keywords.unwrap();
        assert_eq!(keywords, vec!["portrait", "natural light", "outdoors"]);
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

    #[test]
    fn test_parse_json_with_braces_in_string_value() {
        // `critique` contains literal `{` and `}` characters.  The naive depth
        // counter (pre-hardening) would have treated the `{` as opening a
        // nested object and truncated the slice at the first unmatched `}`.
        let text = r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5, "critique": "braces { and } inside prose"}"#;
        let result = parse_scoring_result(text).unwrap();
        assert!((result.sharpness.unwrap() - 0.8).abs() < 1e-9);
        assert_eq!(
            result.critique.as_deref(),
            Some("braces { and } inside prose")
        );
    }

    #[test]
    fn test_parse_json_with_escaped_quote_in_string() {
        // Backslash-escaped quotes inside a string must not flip the
        // in-string state prematurely.
        let text = r#"{"sharpness": 0.8, "exposure": 0.7, "composition": 0.6, "subject_clarity": 0.9, "aesthetics": 0.5, "critique": "say \"hi\" and then }"}"#;
        let result = parse_scoring_result(text).unwrap();
        assert_eq!(result.critique.as_deref(), Some("say \"hi\" and then }"));
    }
}

// ----------------------------------------------------------------
// Shared prompt helper
// ----------------------------------------------------------------

/// Send a multimodal image + text message to any Rig completion model and
/// return the response text.  All provider impls delegate here to avoid
/// repeating the `build_image_message` / request-builder boilerplate.
///
/// Rig 0.41 removed the `Agent` abstraction from `rig-core`; a one-shot
/// preamble + prompt request is now expressed directly against
/// [`CompletionModel`] via `completion_request()`.
async fn run_model_prompt<M: CompletionModel + Clone>(
    model: &M,
    preamble: &str,
    image_base64: &str,
    user_prompt: &str,
    error_context: &'static str,
) -> Result<String> {
    let msg = build_image_message(image_base64, user_prompt);
    let response = model
        .completion_request(msg)
        .preamble(preamble.to_owned())
        .send()
        .await
        .context(error_context)?;
    extract_response_text(response.choice).context(error_context)
}

/// Pull the assistant's text out of a completion response.
///
/// Joins every `Text` block with newlines (reasoning models may emit
/// reasoning blocks before the text; those are skipped).  Errors if the
/// response contains no text at all.
fn extract_response_text(choice: Vec<AssistantContent>) -> Result<String> {
    let texts: Vec<String> = choice
        .into_iter()
        .filter_map(|content| match content {
            AssistantContent::Text(t) => Some(t.text),
            _ => None,
        })
        .collect();
    if texts.is_empty() {
        anyhow::bail!("LLM response contained no text content");
    }
    Ok(texts.join("\n"))
}

// ----------------------------------------------------------------
// Provider structs — macro-generated for API-key providers,
// manual for Ollama (different client construction).
// ----------------------------------------------------------------

/// Generate `DescriptionProvider` and `ScoringProvider` impls for a provider
/// struct that stores its pre-built Rig completion model in a `model` field
/// and its system preamble in a `preamble` field.
macro_rules! provider_impls {
    ($struct_name:ident, $label:expr) => {
        #[async_trait::async_trait]
        impl DescriptionProvider for $struct_name {
            async fn describe(&self, image_base64: &str, prompt: &str) -> Result<String> {
                run_model_prompt(
                    &self.model,
                    &self.preamble,
                    image_base64,
                    prompt,
                    concat!($label, " description request failed"),
                )
                .await
            }
        }

        #[async_trait::async_trait]
        impl ScoringProvider for $struct_name {
            async fn score(&self, image_base64: &str, prompt: &str) -> Result<ScoringResult> {
                run_model_prompt(
                    &self.model,
                    &self.preamble,
                    image_base64,
                    prompt,
                    concat!($label, " scoring request failed"),
                )
                .await
                .and_then(|r: String| parse_scoring_result(&r))
            }
        }
    };
}

/// Generate a provider struct with both `DescriptionProvider` and `ScoringProvider`
/// impls for an API-key-based Rig provider.
///
/// The generated struct stores a pre-built [`CompletionModel`] (which holds the
/// provider client), so the underlying `reqwest` client and connection pool are
/// constructed once per `LlmClients` rather than rebuilt on every LLM call.
macro_rules! api_key_provider {
    ($struct_name:ident, $client_type:ty, $label:expr) => {
        struct $struct_name {
            model: <$client_type as CompletionClient>::CompletionModel,
            preamble: String,
        }

        impl $struct_name {
            fn new(api_key: &str, model: &str, preamble: &str) -> Result<Self> {
                let client = <$client_type>::new(api_key)?;
                Ok(Self {
                    model: client.completion_model(model),
                    preamble: preamble.to_owned(),
                })
            }
        }

        provider_impls!($struct_name, $label);
    };
}

api_key_provider!(ClaudeProvider, rig::providers::anthropic::Client, "Claude");
api_key_provider!(OpenAiProvider, rig::providers::openai::Client, "OpenAI");
api_key_provider!(GeminiProvider, rig::providers::gemini::Client, "Gemini");
api_key_provider!(
    DeepSeekProvider,
    rig::providers::deepseek::Client,
    "DeepSeek"
);

/// Ollama uses a builder pattern with `Nothing` instead of an API key.
struct OllamaProvider {
    model: <rig::providers::ollama::Client as CompletionClient>::CompletionModel,
    preamble: String,
}

impl OllamaProvider {
    fn new(base_url: &str, model: &str, preamble: &str) -> Result<Self> {
        let client = rig::providers::ollama::Client::builder()
            .api_key(Nothing)
            .base_url(base_url)
            .build()?;
        Ok(Self {
            model: client.completion_model(model),
            preamble: preamble.to_owned(),
        })
    }
}

provider_impls!(OllamaProvider, "Ollama");

// ----------------------------------------------------------------
// Builder function
// ----------------------------------------------------------------

/// Build a provider instance for the named backend.
///
/// Returns a boxed trait object that implements both [`DescriptionProvider`]
/// and [`ScoringProvider`].  The caller picks which trait to use.
fn build_provider(
    name: &str,
    config: &ProviderConfig,
    preamble: &str,
) -> Result<Box<dyn DualProvider>> {
    match name {
        "claude" => Ok(Box::new(ClaudeProvider::new(
            &resolve_api_key(config)?,
            &config.model,
            preamble,
        )?)),
        "openai" => Ok(Box::new(OpenAiProvider::new(
            &resolve_api_key(config)?,
            &config.model,
            preamble,
        )?)),
        "gemini" => Ok(Box::new(GeminiProvider::new(
            &resolve_api_key(config)?,
            &config.model,
            preamble,
        )?)),
        "deepseek" => Ok(Box::new(DeepSeekProvider::new(
            &resolve_api_key(config)?,
            &config.model,
            preamble,
        )?)),
        "ollama" => {
            let base_url = config
                .base_url
                .as_deref()
                .unwrap_or("http://localhost:11434");
            Ok(Box::new(OllamaProvider::new(
                base_url,
                &config.model,
                preamble,
            )?))
        }
        other => anyhow::bail!("Unsupported provider: {other}"),
    }
}

/// Convenience super-trait so a single boxed object can serve as both
/// description and scoring provider.
trait DualProvider: DescriptionProvider + ScoringProvider {}
impl<T: DescriptionProvider + ScoringProvider> DualProvider for T {}
