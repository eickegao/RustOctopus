//! Configuration schema structs compatible with the Python config.json format.
//!
//! All structs use `#[serde(rename_all = "camelCase")]` to match the
//! camelCase JSON keys produced by the Python Pydantic models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AgentDefaults
// ---------------------------------------------------------------------------

/// Default agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentDefaults {
    pub workspace: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f64,
    pub max_tool_iterations: u32,
    pub memory_window: u32,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: "~/.rustoctopus/workspace".to_string(),
            model: "anthropic/claude-opus-4-5".to_string(),
            max_tokens: 8192,
            temperature: 0.1,
            max_tool_iterations: 40,
            memory_window: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentsConfig
// ---------------------------------------------------------------------------

/// Agent configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentsConfig {
    pub defaults: AgentDefaults,
}

// ---------------------------------------------------------------------------
// ProviderConfig
// ---------------------------------------------------------------------------

/// LLM provider configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub api_key: String,
    pub api_base: Option<String>,
    pub extra_headers: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// ProvidersConfig
// ---------------------------------------------------------------------------

/// Configuration for LLM providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProvidersConfig {
    pub custom: ProviderConfig,
    pub anthropic: ProviderConfig,
    pub openai: ProviderConfig,
    pub openrouter: ProviderConfig,
    pub deepseek: ProviderConfig,
    pub groq: ProviderConfig,
    pub zhipu: ProviderConfig,
    pub dashscope: ProviderConfig,
    pub vllm: ProviderConfig,
    pub gemini: ProviderConfig,
    pub moonshot: ProviderConfig,
    pub minimax: ProviderConfig,
    pub aihubmix: ProviderConfig,
    pub siliconflow: ProviderConfig,
    pub volcengine: ProviderConfig,
}

// ---------------------------------------------------------------------------
// TelegramConfig
// ---------------------------------------------------------------------------

/// Telegram channel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>,
    pub proxy: Option<String>,
    pub reply_to_message: bool,
}

// ---------------------------------------------------------------------------
// FeishuConfig
// ---------------------------------------------------------------------------

/// Feishu/Lark channel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FeishuConfig {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub encrypt_key: String,
    pub verification_token: String,
    pub allow_from: Vec<String>,
}

// ---------------------------------------------------------------------------
// WhatsAppConfig
// ---------------------------------------------------------------------------

/// WhatsApp channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    pub allow_from: Vec<String>,
    pub bridge_port: u16,
    pub bridge_token: Option<String>,
    pub auto_start_bridge: bool,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_from: Vec::new(),
            bridge_port: 3001,
            bridge_token: None,
            auto_start_bridge: true,
        }
    }
}

// ---------------------------------------------------------------------------
// ChannelsConfig
// ---------------------------------------------------------------------------

/// Configuration for chat channels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelsConfig {
    pub send_progress: bool,
    pub send_tool_hints: bool,
    pub telegram: TelegramConfig,
    pub feishu: FeishuConfig,
    pub whatsapp: WhatsAppConfig,
}

impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            send_progress: true,
            send_tool_hints: false,
            telegram: TelegramConfig::default(),
            feishu: FeishuConfig::default(),
            whatsapp: WhatsAppConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// GatewayConfig
// ---------------------------------------------------------------------------

/// Gateway/server configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 18790,
        }
    }
}

// ---------------------------------------------------------------------------
// WebSearchConfig
// ---------------------------------------------------------------------------

/// Web search tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WebSearchConfig {
    pub api_key: String,
    pub max_results: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            max_results: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// WebToolsConfig
// ---------------------------------------------------------------------------

/// Web tools configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WebToolsConfig {
    pub search: WebSearchConfig,
}

// ---------------------------------------------------------------------------
// ExecToolConfig
// ---------------------------------------------------------------------------

/// Shell exec tool configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExecToolConfig {
    pub timeout: u64,
}

impl Default for ExecToolConfig {
    fn default() -> Self {
        Self { timeout: 60 }
    }
}

// ---------------------------------------------------------------------------
// ToolsConfig
// ---------------------------------------------------------------------------

/// Tools configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolsConfig {
    pub web: WebToolsConfig,
    pub exec: ExecToolConfig,
    pub restrict_to_workspace: bool,
}

// ---------------------------------------------------------------------------
// McpConfig
// ---------------------------------------------------------------------------

/// MCP (Model Context Protocol) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpConfig {
    pub enabled: bool,
    pub servers: HashMap<String, McpServerConfig>,
}

// ---------------------------------------------------------------------------
// McpServerConfig
// ---------------------------------------------------------------------------

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
    pub auto_approve: Vec<String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            enabled: true,
            auto_approve: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP tool name helpers
// ---------------------------------------------------------------------------

/// Parse "mcp_filesystem_read_file" -> ("filesystem", "read_file")
pub fn parse_mcp_tool_name(namespaced: &str) -> Option<(&str, &str)> {
    let rest = namespaced.strip_prefix("mcp_")?;
    let idx = rest.find('_')?;
    if idx == 0 || idx == rest.len() - 1 {
        return None;
    }
    Some((&rest[..idx], &rest[idx + 1..]))
}

/// Build "mcp_filesystem_read_file" from ("filesystem", "read_file")
pub fn build_mcp_tool_name(server: &str, tool: &str) -> String {
    format!("mcp_{}_{}", server, tool)
}

// ---------------------------------------------------------------------------
// Config (root)
// ---------------------------------------------------------------------------

/// Root configuration for RustOctopus.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub agents: AgentsConfig,
    pub channels: ChannelsConfig,
    pub providers: ProvidersConfig,
    pub gateway: GatewayConfig,
    pub tools: ToolsConfig,
    pub mcp: McpConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whatsapp_config_defaults() {
        let config: WhatsAppConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.enabled);
        assert!(config.allow_from.is_empty());
        assert_eq!(config.bridge_port, 3001);
        assert!(config.bridge_token.is_none());
        assert!(config.auto_start_bridge);
    }

    #[test]
    fn test_whatsapp_config_camel_case() {
        let json = r#"{
            "enabled": true,
            "allowFrom": ["+1234567890"],
            "bridgePort": 4000,
            "bridgeToken": "secret",
            "autoStartBridge": false
        }"#;
        let config: WhatsAppConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.allow_from, vec!["+1234567890"]);
        assert_eq!(config.bridge_port, 4000);
        assert_eq!(config.bridge_token, Some("secret".to_string()));
        assert!(!config.auto_start_bridge);
    }

    #[test]
    fn test_channels_config_includes_whatsapp() {
        let json = r#"{"whatsapp": {"enabled": true}}"#;
        let config: ChannelsConfig = serde_json::from_str(json).unwrap();
        assert!(config.whatsapp.enabled);
    }
}
