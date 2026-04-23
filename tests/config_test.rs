use imgcull::config::{Config, Prompts};
use std::path::Path;

#[test]
fn default_config_has_expected_fields() {
    let config = Config::default();
    assert_eq!(config.default_settings.concurrency, 4);
    assert_eq!(config.default_settings.description_provider, "claude");
    assert_eq!(config.default_settings.scoring_provider, "claude");
    assert!(config.default_settings.set_rating);
    assert!(!config.default_settings.backup);
}

#[test]
fn config_parses_from_toml() {
    let toml_str = r#"
[default]
concurrency = 8
description_provider = "openai"
scoring_provider = "gemini"
set_rating = false
backup = true

[providers.custom]
model = "my-model"
api_key_env = "MY_KEY"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.default_settings.concurrency, 8);
    assert_eq!(config.default_settings.description_provider, "openai");
    assert_eq!(config.default_settings.scoring_provider, "gemini");
    assert!(!config.default_settings.set_rating);
    assert!(config.default_settings.backup);
    assert!(config.providers.contains_key("custom"));
}

/// Legacy 0.2.x configs included a `[scoring]` section. It is now ignored
/// (dimensions are fixed) but must not cause a parse error for users upgrading.
#[test]
fn config_with_legacy_scoring_section_parses() {
    let toml_str = r#"
[default]
concurrency = 4
description_provider = "claude"
scoring_provider = "claude"

[scoring]
dimensions = ["sharpness", "exposure"]
"#;
    let config: Config = toml::from_str(toml_str).expect("legacy [scoring] must be ignored");
    assert_eq!(config.default_settings.concurrency, 4);
}

#[test]
fn config_load_falls_back_to_defaults() {
    let config = Config::load(Path::new("/nonexistent/config.toml")).unwrap();
    assert_eq!(config.default_settings.concurrency, 4);
    assert_eq!(config.providers.len(), 5);
}

#[test]
fn default_prompts_have_entries() {
    let prompts = Prompts::default();
    assert!(!prompts.description.system.is_empty());
    assert!(!prompts.description.template.is_empty());
    assert!(!prompts.scoring.system.is_empty());
    assert!(!prompts.scoring.template.is_empty());
}

#[test]
fn provider_configs_have_correct_defaults() {
    let config = Config::default();
    let claude = config.providers.get("claude").unwrap();
    assert_eq!(claude.model, "claude-sonnet-4-6-20250514");
    assert_eq!(claude.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
    assert!(claude.base_url.is_none());

    let ollama = config.providers.get("ollama").unwrap();
    assert_eq!(ollama.model, "llava");
    assert!(ollama.api_key_env.is_none());
    assert_eq!(ollama.base_url.as_deref(), Some("http://localhost:11434"));
}

#[test]
fn render_scoring_prompt_replaces_placeholders() {
    let prompts = Prompts::default();
    let mut guidelines = std::collections::HashMap::new();
    guidelines.insert("sharpness".to_string(), "1.0 = tack sharp".to_string());
    let rendered = prompts.render_scoring_prompt(&guidelines);
    // The canonical dimensions list from `scoring::DIMENSIONS` must appear.
    assert!(rendered.contains("sharpness, exposure, composition, subject_clarity, aesthetics"));
    assert!(rendered.contains("- sharpness: 1.0 = tack sharp"));
    assert!(!rendered.contains("{{dimensions}}"));
    assert!(!rendered.contains("{{guidelines}}"));
}

#[test]
fn default_guidelines_has_five_entries() {
    let prompts = Prompts::default();
    assert_eq!(prompts.guidelines.len(), 5);
}
