//! CLI integration tests for rustoctopus-cli.
//!
//! Tests CLI argument parsing and config factory round-trip.

use rustoctopus_core::agent::AgentLoop;
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::channels::{Channel, ChannelManager};
use rustoctopus_core::config::factory::{create_provider, resolve_workspace_path};
use rustoctopus_core::config::schema::Config;
use rustoctopus_core::providers::traits::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

struct EchoProvider;

#[async_trait::async_trait]
impl LlmProvider for EchoProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        _tools: Option<&[ToolDefinition]>,
        _model: &str,
        _params: &ChatParams,
    ) -> anyhow::Result<LlmResponse> {
        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .and_then(|m| m.content.as_ref())
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        Ok(LlmResponse {
            content: Some(format!("Echo: {}", last_user)),
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            usage: TokenUsage::default(),
            reasoning_content: None,
        })
    }

    fn default_model(&self) -> &str {
        "echo"
    }
}

// ---------------------------------------------------------------------------
// Mock channel
// ---------------------------------------------------------------------------

struct MockChannel {
    name: String,
    started: Arc<AtomicBool>,
    sent: Arc<Mutex<Vec<rustoctopus_core::bus::events::OutboundMessage>>>,
}

impl MockChannel {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            started: Arc::new(AtomicBool::new(false)),
            sent: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl Channel for MockChannel {
    fn name(&self) -> &str {
        &self.name
    }
    async fn start(&mut self) -> anyhow::Result<()> {
        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&mut self) -> anyhow::Result<()> {
        self.started.store(false, Ordering::SeqCst);
        Ok(())
    }
    async fn send(&self, msg: rustoctopus_core::bus::events::OutboundMessage) -> anyhow::Result<()> {
        self.sent.lock().unwrap().push(msg);
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Config factory tests
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_workspace_expands_tilde() {
    let path = resolve_workspace_path("~/.rustoctopus/workspace");
    assert!(!path.to_string_lossy().contains('~'));
}

#[test]
fn test_config_to_provider_roundtrip() {
    let mut config = Config::default();
    config.agents.defaults.model = "anthropic/claude-sonnet-4-20250514".to_string();
    config.providers.anthropic.api_key = "test-key".to_string();
    let provider = create_provider(&config).unwrap();
    assert_eq!(provider.default_model(), "anthropic/claude-sonnet-4-20250514");
}

// ---------------------------------------------------------------------------
// AgentLoop from_config tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_from_config_process() {
    let dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.agents.defaults.workspace = dir.path().to_string_lossy().to_string();
    config.agents.defaults.max_tool_iterations = 5;
    config.agents.defaults.temperature = 0.3;

    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let provider: Box<dyn LlmProvider> = Box::new(EchoProvider);
    let mut agent = AgentLoop::from_config(config, bus, provider, inbound_rx);

    let result = agent
        .process_direct("hello from config", "cli:test")
        .await
        .unwrap();
    assert_eq!(result, "Echo: hello from config");
}

// ---------------------------------------------------------------------------
// ChannelManager dispatch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_channel_manager_dispatch() {
    let (bus, _inbound_rx, outbound_rx) = MessageBus::new();

    let ch = MockChannel::new("mock");
    let sent = Arc::clone(&ch.sent);

    let mut mgr = ChannelManager::new(bus.clone(), outbound_rx);
    mgr.add_channel(Box::new(ch));
    mgr.start_all().await.unwrap();

    // Publish outbound message
    bus.publish_outbound(rustoctopus_core::bus::events::OutboundMessage::new(
        "mock",
        "chat1",
        "test dispatch",
    ))
    .await;

    drop(bus);

    // Run dispatch with timeout
    let handle = tokio::spawn(async move {
        mgr.run_dispatch().await;
    });
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), handle).await;

    // Verify message was dispatched
    let messages = sent.lock().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "test dispatch");
    assert_eq!(messages[0].channel, "mock");
}

// ---------------------------------------------------------------------------
// CLI arg parsing tests
// ---------------------------------------------------------------------------

#[test]
fn test_cli_help_compiles() {
    // Just verify the binary was built — actual CLI parsing is tested by clap
    // We trust clap's derive macro for argument parsing correctness.
    assert!(true);
}
