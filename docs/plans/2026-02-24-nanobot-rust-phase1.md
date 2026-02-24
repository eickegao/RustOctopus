# RustOctopus Rust Port — Phase 1: Core Engine

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build `rustoctopus-core` Rust library with agent loop, tools, providers, session, memory, and cron — testable without any UI.

**Architecture:** Cargo workspace with `rustoctopus-core` lib crate. Async runtime via tokio. All LLM providers accessed through a unified OpenAI-compatible HTTP client. Tools registered dynamically via trait objects.

**Tech Stack:** Rust 1.75+, tokio, reqwest, serde/serde_json, chrono, regex, async-trait, tracing

---

### Task 1: Workspace Scaffolding

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/rustoctopus-core/Cargo.toml`
- Create: `crates/rustoctopus-core/src/lib.rs`

**Step 1: Create workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["crates/rustoctopus-core"]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.12", features = ["json"] }
regex = "1"
uuid = { version = "1", features = ["v4"] }
glob = "0.3"
thiserror = "2"
anyhow = "1"
```

**Step 2: Create rustoctopus-core Cargo.toml**

```toml
[package]
name = "rustoctopus-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
async-trait = { workspace = true }
tracing = { workspace = true }
reqwest = { workspace = true }
regex = { workspace = true }
uuid = { workspace = true }
glob = { workspace = true }
thiserror = { workspace = true }
anyhow = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

**Step 3: Create lib.rs with module declarations**

```rust
pub mod bus;
pub mod config;
pub mod providers;
pub mod tools;
pub mod agent;
pub mod session;
pub mod cron;
```

Create stub `mod.rs` for each module so it compiles.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles with warnings about empty modules

**Step 5: Commit**

```bash
git add crates/ Cargo.toml
git commit -m "feat: scaffold Rust workspace with rustoctopus-core crate"
```

---

### Task 2: Config Schema

**Files:**
- Create: `crates/rustoctopus-core/src/config/mod.rs`
- Create: `crates/rustoctopus-core/src/config/schema.rs`
- Create: `crates/rustoctopus-core/src/config/loader.rs`
- Test: `crates/rustoctopus-core/src/config/tests.rs`

**Step 1: Write test for config deserialization**

```rust
// config/tests.rs
#[cfg(test)]
mod tests {
    use super::schema::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.agents.defaults.model, "anthropic/claude-opus-4-5");
        assert_eq!(config.agents.defaults.max_tokens, 8192);
        assert_eq!(config.agents.defaults.temperature, 0.1);
        assert_eq!(config.agents.defaults.memory_window, 100);
    }

    #[test]
    fn test_deserialize_camel_case() {
        let json = r#"{
            "agents": {
                "defaults": {
                    "workspace": "~/test",
                    "model": "deepseek/deepseek-chat",
                    "maxTokens": 4096,
                    "temperature": 0.5,
                    "maxToolIterations": 20,
                    "memoryWindow": 50
                }
            }
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.agents.defaults.model, "deepseek/deepseek-chat");
        assert_eq!(config.agents.defaults.max_tokens, 4096);
        assert_eq!(config.agents.defaults.memory_window, 50);
    }

    #[test]
    fn test_config_round_trip() {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agents.defaults.model, config.agents.defaults.model);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core config`
Expected: FAIL — module not found

**Step 3: Implement config schema**

```rust
// config/schema.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub agents: AgentsConfig,
    pub channels: ChannelsConfig,
    pub providers: ProvidersConfig,
    pub gateway: GatewayConfig,
    pub tools: ToolsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentsConfig {
    pub defaults: AgentDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            workspace: "~/.rustoctopus/workspace".into(),
            model: "anthropic/claude-opus-4-5".into(),
            max_tokens: 8192,
            temperature: 0.1,
            max_tool_iterations: 40,
            memory_window: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderConfig {
    pub api_key: String,
    pub api_base: Option<String>,
    pub extra_headers: Option<std::collections::HashMap<String, String>>,
}

// ProvidersConfig with all provider fields
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

// Channel configs — Telegram + Feishu for Phase 1, others as stubs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>,
    pub proxy: Option<String>,
    pub reply_to_message: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FeishuConfig {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub encrypt_key: String,
    pub verification_token: String,
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelsConfig {
    pub send_progress: bool,
    pub send_tool_hints: bool,
    pub telegram: TelegramConfig,
    pub feishu: FeishuConfig,
    // Other channels as serde_json::Value for forward compat
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self { host: "0.0.0.0".into(), port: 18790 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WebSearchConfig {
    pub api_key: String,
    pub max_results: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExecToolConfig {
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolsConfig {
    pub web: WebToolsConfig,
    pub exec: ExecToolConfig,
    pub restrict_to_workspace: bool,
}

// Implement Default for all remaining structs...
// (all #[serde(default)] structs that don't derive Default need manual impl)
```

**Step 4: Implement config loader**

```rust
// config/loader.rs
use std::path::{Path, PathBuf};
use anyhow::Result;
use super::schema::Config;

pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rustoctopus")
        .join("config.json")
}

pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let path = path.map(|p| p.to_owned()).unwrap_or_else(default_config_path);
    if path.exists() {
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        Ok(Config::default())
    }
}

pub fn save_config(config: &Config, path: Option<&Path>) -> Result<()> {
    let path = path.map(|p| p.to_owned()).unwrap_or_else(default_config_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, data)?;
    Ok(())
}
```

Note: add `dirs = "6"` to Cargo.toml dependencies.

**Step 5: Run tests**

Run: `cargo test -p rustoctopus-core config`
Expected: all 3 tests PASS

**Step 6: Commit**

```bash
git commit -am "feat: config schema with camelCase serde compat"
```

---

### Task 3: Message Bus

**Files:**
- Create: `crates/rustoctopus-core/src/bus/mod.rs`
- Create: `crates/rustoctopus-core/src/bus/events.rs`
- Create: `crates/rustoctopus-core/src/bus/queue.rs`

**Step 1: Write tests**

```rust
// bus/mod.rs tests
#[cfg(test)]
mod tests {
    use super::{events::*, queue::*};

    #[tokio::test]
    async fn test_bus_inbound_send_recv() {
        let (bus, mut inbound_rx, _outbound_rx) = MessageBus::new();
        bus.publish_inbound(InboundMessage::new("telegram", "user1", "chat1", "hello")).await;
        let msg = inbound_rx.recv().await.unwrap();
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.channel, "telegram");
    }

    #[tokio::test]
    async fn test_bus_outbound_send_recv() {
        let (bus, _inbound_rx, mut outbound_rx) = MessageBus::new();
        bus.publish_outbound(OutboundMessage::new("telegram", "chat1", "hi")).await;
        let msg = outbound_rx.recv().await.unwrap();
        assert_eq!(msg.content, "hi");
    }

    #[test]
    fn test_session_key() {
        let msg = InboundMessage::new("telegram", "user1", "chat1", "hello");
        assert_eq!(msg.session_key(), "telegram:chat1");
    }

    #[test]
    fn test_session_key_override() {
        let mut msg = InboundMessage::new("telegram", "user1", "chat1", "hello");
        msg.session_key_override = Some("custom:key".into());
        assert_eq!(msg.session_key(), "custom:key");
    }
}
```

**Step 2: Run tests — expect FAIL**

**Step 3: Implement events**

```rust
// bus/events.rs
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub media: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub session_key_override: Option<String>,
}

impl InboundMessage {
    pub fn new(channel: &str, sender_id: &str, chat_id: &str, content: &str) -> Self {
        Self {
            channel: channel.into(),
            sender_id: sender_id.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: HashMap::new(),
            session_key_override: None,
        }
    }

    pub fn session_key(&self) -> String {
        self.session_key_override.clone()
            .unwrap_or_else(|| format!("{}:{}", self.channel, self.chat_id))
    }
}

#[derive(Debug, Clone)]
pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    pub reply_to: Option<String>,
    pub media: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl OutboundMessage {
    pub fn new(channel: &str, chat_id: &str, content: &str) -> Self {
        Self {
            channel: channel.into(),
            chat_id: chat_id.into(),
            content: content.into(),
            reply_to: None,
            media: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}
```

**Step 4: Implement queue**

```rust
// bus/queue.rs
use tokio::sync::mpsc;
use super::events::{InboundMessage, OutboundMessage};

#[derive(Clone)]
pub struct MessageBus {
    inbound_tx: mpsc::UnboundedSender<InboundMessage>,
    outbound_tx: mpsc::UnboundedSender<OutboundMessage>,
}

impl MessageBus {
    /// Create bus + receivers. AgentLoop gets inbound_rx, ChannelManager gets outbound_rx.
    pub fn new() -> (Self, mpsc::UnboundedReceiver<InboundMessage>, mpsc::UnboundedReceiver<OutboundMessage>) {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        (Self { inbound_tx, outbound_tx }, inbound_rx, outbound_rx)
    }

    pub async fn publish_inbound(&self, msg: InboundMessage) {
        let _ = self.inbound_tx.send(msg);
    }

    pub async fn publish_outbound(&self, msg: OutboundMessage) {
        let _ = self.outbound_tx.send(msg);
    }
}
```

**Step 5: Run tests, verify PASS**

Run: `cargo test -p rustoctopus-core bus`

**Step 6: Commit**

```bash
git commit -am "feat: message bus with tokio::mpsc channels"
```

---

### Task 4: Provider Traits + Registry

**Files:**
- Create: `crates/rustoctopus-core/src/providers/mod.rs`
- Create: `crates/rustoctopus-core/src/providers/traits.rs`
- Create: `crates/rustoctopus-core/src/providers/registry.rs`

**Step 1: Write tests**

```rust
// providers/mod.rs tests
#[cfg(test)]
mod tests {
    use super::registry::*;

    #[test]
    fn test_find_by_model_anthropic() {
        let spec = find_by_model("anthropic/claude-sonnet-4-5");
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().name, "anthropic");
    }

    #[test]
    fn test_find_by_model_deepseek() {
        let spec = find_by_model("deepseek-chat");
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().name, "deepseek");
    }

    #[test]
    fn test_find_gateway_by_key_prefix() {
        let spec = find_gateway(None, Some("sk-or-abc123"), None);
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().name, "openrouter");
    }

    #[test]
    fn test_find_gateway_by_base_keyword() {
        let spec = find_gateway(None, None, Some("https://aihubmix.com/v1"));
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().name, "aihubmix");
    }

    #[test]
    fn test_find_by_name() {
        let spec = find_by_name("dashscope");
        assert!(spec.is_some());
        assert_eq!(spec.unwrap().display_name, "DashScope");
    }
}
```

**Step 2: Run tests — expect FAIL**

**Step 3: Implement traits**

```rust
// providers/traits.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Option<serde_json::Value>, // String or array of content blocks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FinishReason { Stop, ToolCalls, MaxTokens, Error }

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
    pub reasoning_content: Option<String>,
}

impl LlmResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct ChatParams {
    pub max_tokens: u32,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub r#type: String, // "function"
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
        model: &str,
        params: &ChatParams,
    ) -> anyhow::Result<LlmResponse>;

    fn default_model(&self) -> &str;
}
```

**Step 4: Implement registry** (same structure as Python, static array)

```rust
// providers/registry.rs
pub struct ProviderSpec {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
    pub env_key: &'static str,
    pub display_name: &'static str,
    pub default_api_base: &'static str,
    pub model_prefix: &'static str,
    pub strip_model_prefix: bool,
    pub is_gateway: bool,
    pub is_local: bool,
    pub is_oauth: bool,
    pub supports_prompt_caching: bool,
    pub detect_by_key_prefix: &'static str,
    pub detect_by_base_keyword: &'static str,
}

pub static PROVIDERS: &[ProviderSpec] = &[
    // Gateways first (priority order)
    ProviderSpec { name: "openrouter", keywords: &["openrouter"], env_key: "OPENROUTER_API_KEY",
        display_name: "OpenRouter", default_api_base: "https://openrouter.ai/api/v1",
        model_prefix: "", strip_model_prefix: false,
        is_gateway: true, is_local: false, is_oauth: false,
        supports_prompt_caching: true,
        detect_by_key_prefix: "sk-or-", detect_by_base_keyword: "openrouter" },
    ProviderSpec { name: "aihubmix", keywords: &["aihubmix"], env_key: "OPENAI_API_KEY",
        display_name: "AiHubMix", default_api_base: "https://aihubmix.com/v1",
        model_prefix: "", strip_model_prefix: true,
        is_gateway: true, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "aihubmix" },
    // Standard providers
    ProviderSpec { name: "anthropic", keywords: &["anthropic", "claude"], env_key: "ANTHROPIC_API_KEY",
        display_name: "Anthropic", default_api_base: "",
        model_prefix: "", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: true,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    ProviderSpec { name: "openai", keywords: &["openai", "gpt"], env_key: "OPENAI_API_KEY",
        display_name: "OpenAI", default_api_base: "",
        model_prefix: "", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    ProviderSpec { name: "deepseek", keywords: &["deepseek"], env_key: "DEEPSEEK_API_KEY",
        display_name: "DeepSeek", default_api_base: "",
        model_prefix: "deepseek", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    ProviderSpec { name: "gemini", keywords: &["gemini"], env_key: "GEMINI_API_KEY",
        display_name: "Gemini", default_api_base: "",
        model_prefix: "gemini", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    ProviderSpec { name: "dashscope", keywords: &["qwen", "dashscope"], env_key: "DASHSCOPE_API_KEY",
        display_name: "DashScope", default_api_base: "",
        model_prefix: "dashscope", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    ProviderSpec { name: "moonshot", keywords: &["moonshot", "kimi"], env_key: "MOONSHOT_API_KEY",
        display_name: "Moonshot", default_api_base: "https://api.moonshot.ai/v1",
        model_prefix: "moonshot", strip_model_prefix: false,
        is_gateway: false, is_local: false, is_oauth: false,
        supports_prompt_caching: false,
        detect_by_key_prefix: "", detect_by_base_keyword: "" },
    // ... remaining providers follow same pattern
];

pub fn find_by_model(model: &str) -> Option<&'static ProviderSpec> {
    let lower = model.to_lowercase();
    let normalized = lower.replace('-', "_");
    let prefix = lower.split('/').next().unwrap_or("");
    let norm_prefix = prefix.replace('-', "_");

    let std_specs: Vec<_> = PROVIDERS.iter()
        .filter(|s| !s.is_gateway && !s.is_local)
        .collect();

    // Explicit prefix match first
    for spec in &std_specs {
        if !prefix.is_empty() && norm_prefix == spec.name {
            return Some(spec);
        }
    }
    // Keyword match
    for spec in &std_specs {
        if spec.keywords.iter().any(|kw| lower.contains(kw) || normalized.contains(&kw.replace('-', "_"))) {
            return Some(spec);
        }
    }
    None
}

pub fn find_gateway(name: Option<&str>, api_key: Option<&str>, api_base: Option<&str>) -> Option<&'static ProviderSpec> {
    if let Some(name) = name {
        if let Some(spec) = find_by_name(name) {
            if spec.is_gateway || spec.is_local {
                return Some(spec);
            }
        }
    }
    for spec in PROVIDERS {
        if !spec.detect_by_key_prefix.is_empty() {
            if let Some(key) = api_key {
                if key.starts_with(spec.detect_by_key_prefix) {
                    return Some(spec);
                }
            }
        }
        if !spec.detect_by_base_keyword.is_empty() {
            if let Some(base) = api_base {
                if base.contains(spec.detect_by_base_keyword) {
                    return Some(spec);
                }
            }
        }
    }
    None
}

pub fn find_by_name(name: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|s| s.name == name)
}
```

**Step 5: Run tests, verify PASS**

Run: `cargo test -p rustoctopus-core providers`

**Step 6: Commit**

```bash
git commit -am "feat: LLM provider traits and static registry"
```

---

### Task 5: OpenAI-Compatible Client

**Files:**
- Create: `crates/rustoctopus-core/src/providers/openai_compat.rs`

**Step 1: Write test (uses mock or integration test marker)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_gateway() {
        let client = OpenAiCompatClient::new_for_test("openrouter", "sk-or-test");
        assert_eq!(client.resolve_model("anthropic/claude-sonnet-4-5"), "anthropic/claude-sonnet-4-5");
    }

    #[test]
    fn test_resolve_model_deepseek() {
        let client = OpenAiCompatClient::new_for_test("deepseek", "sk-test");
        assert_eq!(client.resolve_model("deepseek-chat"), "deepseek/deepseek-chat");
    }

    #[test]
    fn test_resolve_model_no_double_prefix() {
        let client = OpenAiCompatClient::new_for_test("deepseek", "sk-test");
        assert_eq!(client.resolve_model("deepseek/deepseek-chat"), "deepseek/deepseek-chat");
    }

    #[test]
    fn test_resolve_endpoint_default() {
        let client = OpenAiCompatClient::new_for_test("openai", "sk-test");
        assert!(client.resolve_endpoint().contains("chat/completions"));
    }
}
```

**Step 2: Implement OpenAiCompatClient**

Core struct implementing `LlmProvider` trait. Uses `reqwest::Client` for HTTP.
Key methods: `resolve_model()`, `resolve_endpoint()`, `build_headers()`, `chat()` → POST to `/v1/chat/completions`, parse response into `LlmResponse`.

Handles: bearer auth, extra headers, model prefixing, cache control injection, empty content sanitization.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: OpenAI-compatible LLM client"
```

---

### Task 6: Tool Trait + Registry

**Files:**
- Create: `crates/rustoctopus-core/src/tools/mod.rs`
- Create: `crates/rustoctopus-core/src/tools/registry.rs`
- Create: `crates/rustoctopus-core/src/tools/traits.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Echoes input" }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            })
        }
        async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
            Ok(params["text"].as_str().unwrap_or("").to_string())
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        assert!(registry.has("echo"));
        let result = registry.execute("echo", serde_json::json!({"text": "hello"})).await;
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent", serde_json::json!({})).await;
        assert!(result.contains("not found"));
    }

    #[test]
    fn test_get_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let defs = registry.get_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "echo");
    }
}
```

**Step 2: Implement trait + registry** (as designed in Part 2)

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: Tool trait and ToolRegistry"
```

---

### Task 7: Filesystem Tools

**Files:**
- Create: `crates/rustoctopus-core/src/tools/filesystem.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();

        let tool = ReadFileTool::new(dir.path().to_path_buf(), None);
        let result = tool.execute(serde_json::json!({"path": path.to_str().unwrap()})).await.unwrap();
        assert!(result.contains("hello world"));
    }

    #[tokio::test]
    async fn test_write_file_creates_parents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub/dir/test.txt");

        let tool = WriteFileTool::new(dir.path().to_path_buf(), None);
        let result = tool.execute(serde_json::json!({
            "path": path.to_str().unwrap(),
            "content": "new content"
        })).await.unwrap();
        assert!(result.contains("bytes"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_edit_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "foo bar baz").unwrap();

        let tool = EditFileTool::new(dir.path().to_path_buf(), None);
        let result = tool.execute(serde_json::json!({
            "path": path.to_str().unwrap(),
            "old_text": "bar",
            "new_text": "qux"
        })).await.unwrap();
        assert!(result.contains("Updated"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "foo qux baz");
    }

    #[tokio::test]
    async fn test_list_dir() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();

        let tool = ListDirTool::new(dir.path().to_path_buf(), None);
        let result = tool.execute(serde_json::json!({"path": dir.path().to_str().unwrap()})).await.unwrap();
        assert!(result.contains("a.txt"));
        assert!(result.contains("subdir"));
    }

    #[tokio::test]
    async fn test_path_escape_blocked() {
        let dir = TempDir::new().unwrap();
        let tool = ReadFileTool::new(dir.path().to_path_buf(), Some(dir.path().to_path_buf()));
        let result = tool.execute(serde_json::json!({"path": "/etc/passwd"})).await;
        assert!(result.is_err());
    }
}
```

Note: add `tempfile = "3"` to dev-dependencies.

**Step 2: Implement 4 filesystem tools** following Python logic: path resolution with `~` expansion, workspace-relative paths, allowed_dir boundary check, UTF-8 read/write.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: filesystem tools (read, write, edit, list_dir)"
```

---

### Task 8: Shell Tool

**Files:**
- Create: `crates/rustoctopus-core/src/tools/shell.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exec_simple() {
        let tool = ExecTool::new(".", 60, false);
        let result = tool.execute(serde_json::json!({"command": "echo hello"})).await.unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_exec_deny_pattern() {
        let tool = ExecTool::new(".", 60, false);
        let result = tool.execute(serde_json::json!({"command": "rm -rf /"})).await;
        assert!(result.is_err() || result.unwrap().contains("Error"));
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let tool = ExecTool::new(".", 1, false); // 1 second timeout
        let result = tool.execute(serde_json::json!({"command": "sleep 10"})).await.unwrap();
        assert!(result.contains("timed out") || result.contains("killed"));
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit() {
        let tool = ExecTool::new(".", 60, false);
        let result = tool.execute(serde_json::json!({"command": "false"})).await.unwrap();
        assert!(result.contains("exit code"));
    }
}
```

**Step 2: Implement** using `tokio::process::Command`. Safety deny patterns, timeout with `tokio::time::timeout`, output truncation at 10K chars.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: shell exec tool with safety guards"
```

---

### Task 9: Web Tools

**Files:**
- Create: `crates/rustoctopus-core/src/tools/web.rs`

**Step 1: Write tests** (unit tests for URL validation and HTML stripping; actual API tests marked `#[ignore]`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn test_validate_url_no_scheme() {
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn test_strip_html_tags() {
        let result = strip_tags("<p>Hello <b>world</b></p>");
        assert_eq!(result.trim(), "Hello world");
    }

    #[tokio::test]
    #[ignore] // requires network
    async fn test_web_fetch_real() {
        let tool = WebFetchTool::new();
        let result = tool.execute(serde_json::json!({"url": "https://example.com"})).await.unwrap();
        assert!(result.contains("Example Domain"));
    }
}
```

**Step 2: Implement** `WebSearchTool` (Brave API) and `WebFetchTool` (reqwest + HTML-to-text via regex).

**Step 3: Run tests, verify PASS** (non-ignored ones)

**Step 4: Commit**

```bash
git commit -am "feat: web search and fetch tools"
```

---

### Task 10: Session Manager

**Files:**
- Create: `crates/rustoctopus-core/src/session/mod.rs`
- Create: `crates/rustoctopus-core/src/session/manager.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_add_message() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "hello");
        session.add_message("assistant", "hi there");
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_session_get_history_trims_leading_non_user() {
        let mut session = Session::new("test:chat");
        session.add_message("assistant", "orphan");
        session.add_message("user", "hello");
        session.add_message("assistant", "hi");
        let history = session.get_history(100);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["role"], "user");
    }

    #[test]
    fn test_manager_save_and_load() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let mut session = manager.get_or_create("test:chat");
        session.add_message("user", "hello");
        manager.save(&session).unwrap();

        // Create new manager (simulates restart)
        let manager2 = SessionManager::new(dir.path().to_path_buf());
        let loaded = manager2.get_or_create("test:chat");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0]["content"], "hello");
    }

    #[test]
    fn test_session_clear() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "hello");
        session.clear();
        assert_eq!(session.messages.len(), 0);
        assert_eq!(session.last_consolidated, 0);
    }
}
```

**Step 2: Implement** Session struct (JSONL persistence) and SessionManager (file-based with in-memory cache), matching Python format.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: session manager with JSONL persistence"
```

---

### Task 11: Memory System

**Files:**
- Create: `crates/rustoctopus-core/src/agent/memory.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_write_long_term() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        assert_eq!(store.read_long_term(), "");
        store.write_long_term("User prefers dark mode");
        assert_eq!(store.read_long_term(), "User prefers dark mode");
    }

    #[test]
    fn test_append_history() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        store.append_history("[2026-02-24] User asked about Rust");
        store.append_history("[2026-02-24] Discussed architecture");
        let content = std::fs::read_to_string(store.history_file()).unwrap();
        assert!(content.contains("Rust"));
        assert!(content.contains("architecture"));
    }

    #[test]
    fn test_memory_context() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        assert_eq!(store.get_memory_context(), "");
        store.write_long_term("Some facts");
        assert!(store.get_memory_context().contains("Long-term Memory"));
    }
}
```

**Step 2: Implement** MemoryStore (MEMORY.md + HISTORY.md), consolidate method that calls LLM with save_memory tool.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: dual-layer memory system"
```

---

### Task 12: Context Builder

**Files:**
- Create: `crates/rustoctopus-core/src/agent/context.rs`
- Create: `crates/rustoctopus-core/src/agent/skills.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_system_prompt_includes_identity() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("rustoctopus"));
        assert!(prompt.contains("Workspace"));
    }

    #[test]
    fn test_build_messages_structure() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let messages = ctx.build_messages(&[], "hello", None, Some("cli"), Some("direct"));
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn test_bootstrap_files_loaded() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("SOUL.md"), "Be kind").unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Be kind"));
    }
}
```

**Step 2: Implement** ContextBuilder (system prompt assembly: identity + bootstrap files + memory + skills summary), build_messages(), add_tool_result(), add_assistant_message().

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: context builder with bootstrap files and memory"
```

---

### Task 13: Agent Loop

**Files:**
- Create: `crates/rustoctopus-core/src/agent/loop.rs`

**Step 1: Write test with mock provider**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::queue::MessageBus;
    use crate::providers::traits::*;
    use tempfile::TempDir;

    /// Mock provider that returns a fixed response
    struct MockProvider { response: String }

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn chat(&self, _messages: &[ChatMessage], _tools: Option<&[ToolDefinition]>,
                      _model: &str, _params: &ChatParams) -> anyhow::Result<LlmResponse> {
            Ok(LlmResponse {
                content: Some(self.response.clone()),
                tool_calls: vec![],
                finish_reason: FinishReason::Stop,
                usage: TokenUsage::default(),
                reasoning_content: None,
            })
        }
        fn default_model(&self) -> &str { "mock" }
    }

    #[tokio::test]
    async fn test_process_direct_simple() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider { response: "Hello!".into() });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        let result = agent.process_direct("hi", "test:chat").await.unwrap();
        assert_eq!(result, "Hello!");
    }

    #[tokio::test]
    async fn test_slash_new() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider { response: "done".into() });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        // Add some history first
        agent.process_direct("hello", "test:chat").await.unwrap();
        let result = agent.process_direct("/new", "test:chat").await.unwrap();
        assert!(result.contains("New session") || result.contains("new session"));
    }
}
```

**Step 2: Implement AgentLoop** — the core `run()` loop, `process_message()`, `run_agent_loop()` (iterative tool execution), `save_turn()`, async memory consolidation trigger.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: agent loop with tool iteration and session management"
```

---

### Task 14: Message + Spawn + Cron Tools

**Files:**
- Create: `crates/rustoctopus-core/src/tools/message.rs`
- Create: `crates/rustoctopus-core/src/tools/spawn.rs`
- Create: `crates/rustoctopus-core/src/tools/cron_tool.rs`

**Step 1: Write tests for each**

Message tool: test that it calls the send callback with correct OutboundMessage.
Spawn tool: test that it returns a status string containing the task label.
Cron tool: test add/list/remove operations.

**Step 2: Implement** — MessageTool holds `Arc<dyn Fn(OutboundMessage)>` callback, SpawnTool delegates to SubagentManager, CronTool delegates to CronService.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: message, spawn, and cron tools"
```

---

### Task 15: Subagent Manager

**Files:**
- Create: `crates/rustoctopus-core/src/agent/subagent.rs`

**Step 1: Write test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_prompt_contains_workspace() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        let prompt = manager.build_subagent_prompt("test task");
        assert!(prompt.contains("/tmp/workspace"));
        assert!(prompt.contains("Subagent"));
    }
}
```

**Step 2: Implement** — spawn() creates `tokio::spawn` background task, runs isolated agent loop (max 15 iterations), announces result via system InboundMessage on bus.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: subagent manager for background tasks"
```

---

### Task 16: Cron Service

**Files:**
- Create: `crates/rustoctopus-core/src/cron/mod.rs`
- Create: `crates/rustoctopus-core/src/cron/types.rs`
- Create: `crates/rustoctopus-core/src/cron/service.rs`

Note: add `cron = "0.15"` to dependencies for cron expression parsing.

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cron_schedule_serialization() {
        let schedule = CronSchedule::every(5000);
        let json = serde_json::to_string(&schedule).unwrap();
        let parsed: CronSchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.every_ms, Some(5000));
    }

    #[tokio::test]
    async fn test_add_and_list_jobs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        service.add_job("test", CronSchedule::every(60000), "hello", false, None, None).unwrap();
        let jobs = service.list_jobs(false);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "test");
    }

    #[tokio::test]
    async fn test_remove_job() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("jobs.json");
        let mut service = CronService::new(path);

        let job = service.add_job("test", CronSchedule::every(60000), "hello", false, None, None).unwrap();
        assert!(service.remove_job(&job.id));
        assert_eq!(service.list_jobs(true).len(), 0);
    }
}
```

**Step 2: Implement** CronSchedule/CronPayload/CronJob types (serde camelCase), CronService with JSON file persistence, async timer via `tokio::time::sleep`, job execution callback.

**Step 3: Run tests, verify PASS**

**Step 4: Commit**

```bash
git commit -am "feat: cron service with timer scheduling"
```

---

### Task 17: Integration Test — Full Agent Round Trip

**Files:**
- Create: `crates/rustoctopus-core/tests/integration.rs`

**Step 1: Write integration test**

```rust
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::agent::AgentLoop;
use rustoctopus_core::providers::traits::*;
use tempfile::TempDir;

struct EchoProvider;

#[async_trait::async_trait]
impl LlmProvider for EchoProvider {
    async fn chat(&self, messages: &[ChatMessage], tools: Option<&[ToolDefinition]>,
                  _model: &str, _params: &ChatParams) -> anyhow::Result<LlmResponse> {
        let last_user = messages.iter().rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.as_ref())
            .and_then(|c| c.as_str())
            .unwrap_or("no input");

        Ok(LlmResponse {
            content: Some(format!("Echo: {}", last_user)),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: TokenUsage::default(),
            reasoning_content: None,
        })
    }
    fn default_model(&self) -> &str { "echo" }
}

#[tokio::test]
async fn test_full_agent_round_trip() {
    let dir = TempDir::new().unwrap();
    let provider = Box::new(EchoProvider);
    let (bus, inbound_rx, _) = MessageBus::new();

    let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
    let result = agent.process_direct("hello world", "test:chat").await.unwrap();
    assert_eq!(result, "Echo: hello world");
}

#[tokio::test]
async fn test_session_persistence_across_turns() {
    let dir = TempDir::new().unwrap();
    let provider = Box::new(EchoProvider);
    let (bus, inbound_rx, _) = MessageBus::new();

    let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
    agent.process_direct("first", "test:chat").await.unwrap();
    agent.process_direct("second", "test:chat").await.unwrap();

    // Session should have history from both turns
    let session = agent.sessions().get_or_create("test:chat");
    assert!(session.messages.len() >= 4); // 2 user + 2 assistant
}
```

**Step 2: Run integration tests**

Run: `cargo test -p rustoctopus-core --test integration`
Expected: PASS

**Step 3: Commit**

```bash
git commit -am "test: integration tests for full agent round trip"
```

---

### Task 18: Final Cleanup + README

**Step 1: Run full test suite**

Run: `cargo test -p rustoctopus-core`
Expected: all tests PASS

**Step 2: Run clippy**

Run: `cargo clippy -p rustoctopus-core -- -D warnings`
Fix any warnings.

**Step 3: Commit**

```bash
git commit -am "chore: clippy fixes and Phase 1 complete"
```

---

## Summary

| Task | Module | Description |
|------|--------|-------------|
| 1 | workspace | Cargo workspace scaffolding |
| 2 | config | Schema + loader (serde camelCase compat) |
| 3 | bus | Message types + tokio::mpsc queue |
| 4 | providers | LlmProvider trait + static registry |
| 5 | providers | OpenAI-compatible HTTP client |
| 6 | tools | Tool trait + ToolRegistry |
| 7 | tools | Filesystem tools (read/write/edit/list) |
| 8 | tools | Shell exec tool |
| 9 | tools | Web search + fetch tools |
| 10 | session | Session manager (JSONL) |
| 11 | agent | Memory system (MEMORY.md + HISTORY.md) |
| 12 | agent | Context builder + skills loader |
| 13 | agent | Agent loop (core engine) |
| 14 | tools | Message, spawn, cron tools |
| 15 | agent | Subagent manager |
| 16 | cron | Cron types + service |
| 17 | test | Integration test |
| 18 | cleanup | Clippy + final verification |

**Phase 1 deliverable:** `rustoctopus-core` lib crate, fully tested, with agent loop that can process messages end-to-end using a mock or real LLM provider.
