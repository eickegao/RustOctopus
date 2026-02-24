use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::schema::Config;
use crate::providers::openai_compat::OpenAiCompatClient;
use crate::providers::registry::find_by_model;
use crate::providers::traits::LlmProvider;

/// Expand `~` to the user's home directory.
pub fn resolve_workspace_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

/// Create an LLM provider from the global config.
///
/// Uses `config.agents.defaults.model` to look up the provider spec,
/// then matches the provider name to the corresponding API key in
/// `config.providers`.
pub fn create_provider(config: &Config) -> Result<Box<dyn LlmProvider>> {
    let model = &config.agents.defaults.model;
    let spec = find_by_model(model)
        .with_context(|| format!("Unknown model: {model}"))?;

    let provider_cfg = get_provider_config(config, spec.name);
    let api_base = provider_cfg
        .api_base
        .clone()
        .or_else(|| {
            if spec.default_api_base.is_empty() {
                None
            } else {
                Some(spec.default_api_base.to_string())
            }
        });

    Ok(Box::new(OpenAiCompatClient::new(
        provider_cfg.api_key.clone(),
        api_base,
        model.clone(),
        provider_cfg.extra_headers.clone().unwrap_or_default(),
        Some(spec.name),
    )))
}

fn get_provider_config<'a>(
    config: &'a Config,
    provider_name: &str,
) -> &'a crate::config::schema::ProviderConfig {
    match provider_name {
        "anthropic" => &config.providers.anthropic,
        "openai" => &config.providers.openai,
        "openrouter" => &config.providers.openrouter,
        "deepseek" => &config.providers.deepseek,
        "groq" => &config.providers.groq,
        "gemini" => &config.providers.gemini,
        "moonshot" => &config.providers.moonshot,
        "zhipu" => &config.providers.zhipu,
        "dashscope" => &config.providers.dashscope,
        "vllm" => &config.providers.vllm,
        "minimax" => &config.providers.minimax,
        "aihubmix" => &config.providers.aihubmix,
        "siliconflow" => &config.providers.siliconflow,
        "volcengine" => &config.providers.volcengine,
        _ => &config.providers.custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_workspace_expands_tilde() {
        let path = resolve_workspace_path("~/.nanobot/workspace");
        assert!(!path.to_string_lossy().contains('~'));
        assert!(path.to_string_lossy().contains("nanobot"));
    }

    #[test]
    fn test_resolve_workspace_absolute_unchanged() {
        let path = resolve_workspace_path("/tmp/workspace");
        assert_eq!(path, PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn test_resolve_workspace_relative_unchanged() {
        let path = resolve_workspace_path("relative/path");
        assert_eq!(path, PathBuf::from("relative/path"));
    }

    #[test]
    fn test_create_provider_anthropic() {
        let mut config = Config::default();
        config.providers.anthropic.api_key = "test-key".to_string();
        config.agents.defaults.model = "anthropic/claude-sonnet-4-20250514".to_string();
        let result = create_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_deepseek() {
        let mut config = Config::default();
        config.providers.deepseek.api_key = "test-key".to_string();
        config.agents.defaults.model = "deepseek/deepseek-chat".to_string();
        let result = create_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_missing_key() {
        let mut config = Config::default();
        config.agents.defaults.model = "anthropic/claude-sonnet-4-20250514".to_string();
        // Empty key — should still create provider (fails at runtime)
        let result = create_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_unknown_model() {
        let mut config = Config::default();
        config.agents.defaults.model = "totally-unknown-provider/model".to_string();
        let result = create_provider(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_provider_config_returns_correct_provider() {
        let mut config = Config::default();
        config.providers.openai.api_key = "openai-key".to_string();
        config.providers.anthropic.api_key = "anthropic-key".to_string();
        assert_eq!(get_provider_config(&config, "openai").api_key, "openai-key");
        assert_eq!(
            get_provider_config(&config, "anthropic").api_key,
            "anthropic-key"
        );
        // Unknown falls back to custom
        assert_eq!(get_provider_config(&config, "unknown").api_key, config.providers.custom.api_key);
    }
}
