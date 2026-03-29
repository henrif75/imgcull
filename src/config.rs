//! Configuration and prompts loading for imgcull.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level configuration for imgcull.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Default settings for processing.
    #[serde(rename = "default")]
    pub default_settings: DefaultSettings,

    /// Configured LLM providers.
    #[serde(default = "default_providers")]
    pub providers: HashMap<String, ProviderConfig>,

    /// Scoring configuration.
    #[serde(default)]
    pub scoring: ScoringConfig,
}

/// Default processing settings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DefaultSettings {
    /// Maximum number of parallel LLM requests.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,

    /// Provider used for image description.
    #[serde(default = "default_description_provider")]
    pub description_provider: String,

    /// Provider used for image scoring.
    #[serde(default = "default_scoring_provider")]
    pub scoring_provider: String,

    /// Whether to write star rating to XMP metadata.
    #[serde(default = "default_true")]
    pub set_rating: bool,

    /// Whether to backup existing .xmp sidecars before modifying.
    #[serde(default)]
    pub backup: bool,
}

/// Configuration for a single LLM provider.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    /// Model identifier for this provider.
    pub model: String,

    /// Environment variable name holding the API key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,

    /// Base URL for the provider API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Scoring configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScoringConfig {
    /// Dimensions to score images on.
    #[serde(default = "default_dimensions")]
    pub dimensions: Vec<String>,
}

/// Prompts configuration for description and scoring.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompts {
    /// Prompt entry for image description.
    #[serde(default = "default_description_prompt")]
    pub description: PromptEntry,

    /// Prompt entry for image scoring.
    #[serde(default = "default_scoring_prompt")]
    pub scoring: PromptEntry,

    /// Named scoring guidelines keyed by dimension.
    #[serde(default = "default_guidelines")]
    pub guidelines: HashMap<String, String>,
}

/// A prompt entry with system and template strings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptEntry {
    /// System message for the LLM.
    pub system: String,

    /// User-facing template with optional placeholders.
    pub template: String,
}

// --- Default value functions ---

fn default_concurrency() -> usize {
    4
}

fn default_description_provider() -> String {
    "claude".to_string()
}

fn default_scoring_provider() -> String {
    "claude".to_string()
}

fn default_true() -> bool {
    true
}

fn default_dimensions() -> Vec<String> {
    vec![
        "sharpness".to_string(),
        "exposure".to_string(),
        "composition".to_string(),
        "subject_clarity".to_string(),
        "aesthetics".to_string(),
    ]
}

fn default_providers() -> HashMap<String, ProviderConfig> {
    let mut m = HashMap::new();
    m.insert(
        "claude".to_string(),
        ProviderConfig {
            model: "claude-sonnet-4-6-20250514".to_string(),
            api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
            base_url: None,
        },
    );
    m.insert(
        "openai".to_string(),
        ProviderConfig {
            model: "gpt-5.4".to_string(),
            api_key_env: Some("OPENAI_API_KEY".to_string()),
            base_url: None,
        },
    );
    m.insert(
        "gemini".to_string(),
        ProviderConfig {
            model: "gemini-3.1-pro".to_string(),
            api_key_env: Some("GEMINI_API_KEY".to_string()),
            base_url: None,
        },
    );
    m.insert(
        "deepseek".to_string(),
        ProviderConfig {
            model: "deepseek-v3".to_string(),
            api_key_env: Some("DEEPSEEK_API_KEY".to_string()),
            base_url: None,
        },
    );
    m.insert(
        "ollama".to_string(),
        ProviderConfig {
            model: "llava".to_string(),
            api_key_env: None,
            base_url: Some("http://localhost:11434".to_string()),
        },
    );
    m
}

fn default_description_prompt() -> PromptEntry {
    PromptEntry {
        system: "You are a concise photography describer.".to_string(),
        template: "Describe this photograph in 1-3 sentences. Include the subject, setting, \
                   lighting conditions, and mood. Be concise and factual. \
                   Output the description directly — no preamble, labels, or introductory text."
            .to_string(),
    }
}

fn default_scoring_prompt() -> PromptEntry {
    PromptEntry {
        system: "You are an expert photography critic.".to_string(),
        template: "Analyze this image and score it on the following dimensions (each 0.0 to \
                   1.0):\n\n{{dimensions}}\n\nScoring guidelines:\n{{guidelines}}\n\n\
                   Respond with a JSON object only. Use the dimension names as keys with \
                   float scores as values, and include: a \"critique\" key with a concise \
                   narrative analysis, a \"keywords\" key with an array of 5-15 descriptive \
                   photography keywords (genre, subject, mood, lighting, technique, location \
                   type). Keywords are descriptive tags for image content, crucial for \
                   organizing and discoverability. Example: \
                   {\"sharpness\": 0.95, \"exposure\": 0.88, \"composition\": 0.75, \
                   \"critique\": \"Sharp focus on the subject...\", \
                   \"keywords\": [\"portrait\", \"natural light\", \"outdoors\"]}. \
                   No prose outside the JSON — raw JSON only."
            .to_string(),
    }
}

fn default_guidelines() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert(
        "sharpness".to_string(),
        "1.0 = tack sharp on subject, 0.0 = completely blurry".to_string(),
    );
    m.insert(
        "exposure".to_string(),
        "1.0 = perfectly exposed, 0.0 = severely over/underexposed".to_string(),
    );
    m.insert(
        "composition".to_string(),
        "1.0 = excellent framing and balance, 0.0 = poorly composed".to_string(),
    );
    m.insert(
        "subject_clarity".to_string(),
        "1.0 = subject immediately clear, 0.0 = no discernible subject".to_string(),
    );
    m.insert(
        "aesthetics".to_string(),
        "1.0 = visually stunning, 0.0 = no aesthetic appeal".to_string(),
    );
    m
}

// --- Default trait impls ---

impl Default for DefaultSettings {
    fn default() -> Self {
        Self {
            concurrency: default_concurrency(),
            description_provider: default_description_provider(),
            scoring_provider: default_scoring_provider(),
            set_rating: true,
            backup: false,
        }
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            dimensions: default_dimensions(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_settings: DefaultSettings::default(),
            providers: default_providers(),
            scoring: ScoringConfig::default(),
        }
    }
}

impl Default for Prompts {
    fn default() -> Self {
        Self {
            description: default_description_prompt(),
            scoring: default_scoring_prompt(),
            guidelines: default_guidelines(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file, falling back to defaults if the file is missing.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }
}

impl Prompts {
    /// Load prompts from a TOML file, falling back to defaults if the file is missing.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let prompts: Prompts = toml::from_str(&contents)?;
            Ok(prompts)
        } else {
            Ok(Prompts::default())
        }
    }

    /// Render the scoring prompt template, replacing `{{dimensions}}` and `{{guidelines}}`
    /// placeholders with the provided values.
    pub fn render_scoring_prompt(
        &self,
        dimensions: &[String],
        guidelines: &HashMap<String, String>,
    ) -> String {
        let dims_text = dimensions.join(", ");
        let guide_text: String = guidelines
            .iter()
            .map(|(k, v)| format!("- {k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.scoring
            .template
            .replace("{{dimensions}}", &dims_text)
            .replace("{{guidelines}}", &guide_text)
    }
}
