pub mod factory;
pub mod loader;
pub mod schema;

pub use factory::{create_provider, resolve_workspace_path};
pub use schema::Config;

#[cfg(test)]
mod tests {
    use super::loader::{load_config, save_config};
    use super::schema::*;
    use std::path::Path;

    #[test]
    fn test_default_config() {
        let config = Config::default();

        // AgentDefaults
        let defaults = &config.agents.defaults;
        assert_eq!(defaults.workspace, "~/.rustoctopus/workspace");
        assert_eq!(defaults.model, "anthropic/claude-opus-4-5");
        assert_eq!(defaults.max_tokens, 8192);
        assert!((defaults.temperature - 0.1).abs() < f64::EPSILON);
        assert_eq!(defaults.max_tool_iterations, 40);
        assert_eq!(defaults.memory_window, 100);

        // ChannelsConfig
        assert!(config.channels.send_progress);
        assert!(!config.channels.send_tool_hints);

        // GatewayConfig
        assert_eq!(config.gateway.host, "0.0.0.0");
        assert_eq!(config.gateway.port, 18790);

        // WebSearchConfig
        assert_eq!(config.tools.web.search.max_results, 5);

        // ExecToolConfig
        assert_eq!(config.tools.exec.timeout, 60);

        // ToolsConfig
        assert!(!config.tools.restrict_to_workspace);
    }

    #[test]
    fn test_deserialize_camel_case() {
        let json = r#"{
            "agents": {
                "defaults": {
                    "workspace": "/tmp/test",
                    "model": "openai/gpt-4",
                    "maxTokens": 4096,
                    "temperature": 0.5,
                    "maxToolIterations": 20,
                    "memoryWindow": 50
                }
            },
            "channels": {
                "sendProgress": false,
                "sendToolHints": true
            },
            "gateway": {
                "host": "127.0.0.1",
                "port": 9090
            },
            "tools": {
                "web": {
                    "search": {
                        "apiKey": "test-key",
                        "maxResults": 10
                    }
                },
                "exec": {
                    "timeout": 120
                },
                "restrictToWorkspace": true
            }
        }"#;

        let config: Config = serde_json::from_str(json).expect("Failed to deserialize");

        assert_eq!(config.agents.defaults.workspace, "/tmp/test");
        assert_eq!(config.agents.defaults.model, "openai/gpt-4");
        assert_eq!(config.agents.defaults.max_tokens, 4096);
        assert!((config.agents.defaults.temperature - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.agents.defaults.max_tool_iterations, 20);
        assert_eq!(config.agents.defaults.memory_window, 50);

        assert!(!config.channels.send_progress);
        assert!(config.channels.send_tool_hints);

        assert_eq!(config.gateway.host, "127.0.0.1");
        assert_eq!(config.gateway.port, 9090);

        assert_eq!(config.tools.web.search.api_key, "test-key");
        assert_eq!(config.tools.web.search.max_results, 10);
        assert_eq!(config.tools.exec.timeout, 120);
        assert!(config.tools.restrict_to_workspace);
    }

    #[test]
    fn test_config_round_trip() {
        let mut config = Config::default();
        config.agents.defaults.max_tokens = 16384;
        config.agents.defaults.model = "deepseek/deepseek-chat".to_string();
        config.gateway.port = 3000;
        config.channels.send_progress = false;
        config.tools.exec.timeout = 300;
        config.providers.anthropic.api_key = "sk-ant-test".to_string();
        config.providers.openai.api_base = Some("https://custom.api.com/v1".to_string());

        let json = serde_json::to_string_pretty(&config).expect("Failed to serialize");
        let deserialized: Config = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let path = Path::new("/tmp/nanobot_test_nonexistent_config_12345.json");
        // Make sure it doesn't exist
        let _ = std::fs::remove_file(path);

        let config = load_config(Some(path)).expect("Should return default config");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = dir.path().join("config.json");

        let mut config = Config::default();
        config.agents.defaults.max_tokens = 2048;
        config.agents.defaults.workspace = "/custom/workspace".to_string();
        config.gateway.host = "localhost".to_string();
        config.gateway.port = 8080;
        config.providers.deepseek.api_key = "sk-deep-test".to_string();
        config.channels.send_tool_hints = true;
        config.tools.restrict_to_workspace = true;

        save_config(&config, Some(&path)).expect("Failed to save config");

        assert!(path.exists(), "Config file should exist after save");

        let loaded = load_config(Some(&path)).expect("Failed to load config");
        assert_eq!(config, loaded);
    }
}
