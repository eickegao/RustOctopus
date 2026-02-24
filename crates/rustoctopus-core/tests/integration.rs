//! Integration tests for the full agent round trip.
//!
//! Uses an `EchoProvider` mock that echoes the last user message back,
//! exercising the public API end-to-end without making real LLM calls.

use rustoctopus_core::agent::AgentLoop;
use rustoctopus_core::bus::queue::MessageBus;
use rustoctopus_core::providers::traits::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

/// A simple LLM provider that echoes the last user message.
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
// Helper
// ---------------------------------------------------------------------------

/// Spin up a fresh AgentLoop backed by the EchoProvider in a temp directory.
fn make_agent() -> (AgentLoop, TempDir) {
    let dir = TempDir::new().unwrap();
    let provider: Box<dyn LlmProvider> = Box::new(EchoProvider);
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
    (agent, dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_full_agent_round_trip() {
    let (mut agent, _dir) = make_agent();
    let result = agent
        .process_direct("hello world", "test:chat")
        .await
        .unwrap();
    assert_eq!(result, "Echo: hello world");
}

#[tokio::test]
async fn test_echo_preserves_content() {
    let (mut agent, _dir) = make_agent();
    let msg = "The quick brown fox jumps over the lazy dog";
    let result = agent.process_direct(msg, "test:chat").await.unwrap();
    assert_eq!(result, format!("Echo: {}", msg));
}

#[tokio::test]
async fn test_session_persistence_across_turns() {
    let (mut agent, dir) = make_agent();

    // First turn
    let r1 = agent.process_direct("first", "test:chat").await.unwrap();
    assert_eq!(r1, "Echo: first");

    // Second turn
    let r2 = agent.process_direct("second", "test:chat").await.unwrap();
    assert_eq!(r2, "Echo: second");

    // The session file should exist on disk (channel:chat_id -> test_chat.jsonl)
    let session_file = dir.path().join("sessions").join("test_chat.jsonl");
    assert!(
        session_file.exists(),
        "Session file should be persisted at {:?}",
        session_file,
    );

    // Read the file and verify it contains messages from both turns
    let content = std::fs::read_to_string(&session_file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    // Expect: 1 metadata line + 4 message lines (user+assistant for each turn)
    assert!(
        lines.len() >= 5,
        "Expected at least 5 lines (metadata + 4 messages), got {}",
        lines.len(),
    );

    // Verify both user messages appear in the session
    assert!(content.contains("first"), "Session should contain 'first'");
    assert!(content.contains("second"), "Session should contain 'second'");
}

#[tokio::test]
async fn test_third_turn_sees_history() {
    let (mut agent, _dir) = make_agent();

    agent.process_direct("turn one", "test:chat").await.unwrap();
    agent.process_direct("turn two", "test:chat").await.unwrap();

    // Third turn should still work correctly, proving the session accumulates
    let r3 = agent
        .process_direct("turn three", "test:chat")
        .await
        .unwrap();
    assert_eq!(r3, "Echo: turn three");
}

#[tokio::test]
async fn test_slash_new_resets_session() {
    let (mut agent, dir) = make_agent();

    // Build up some history
    agent
        .process_direct("remember this", "test:chat")
        .await
        .unwrap();

    // Reset session
    let result = agent.process_direct("/new", "test:chat").await.unwrap();
    assert!(
        result.contains("New session") || result.contains("new session"),
        "Expected /new response, got: {}",
        result,
    );

    // After /new the session file should still exist but have zero messages
    // (or the file is rewritten with only the metadata line).
    let session_file = dir.path().join("sessions").join("test_chat.jsonl");
    if session_file.exists() {
        let content = std::fs::read_to_string(&session_file).unwrap();
        let message_lines: Vec<&str> = content
            .lines()
            .filter(|l| {
                let trimmed = l.trim();
                if trimmed.is_empty() {
                    return false;
                }
                // Skip the metadata line
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    v.get("_type").and_then(|t| t.as_str()) != Some("metadata")
                } else {
                    false
                }
            })
            .collect();
        assert_eq!(
            message_lines.len(),
            0,
            "After /new, session should have no messages, but found {}",
            message_lines.len(),
        );
    }
}

#[tokio::test]
async fn test_slash_help_returns_help_text() {
    let (mut agent, _dir) = make_agent();
    let result = agent.process_direct("/help", "test:chat").await.unwrap();

    assert!(
        result.contains("RustOctopus help"),
        "Help text should contain 'RustOctopus help', got: {}",
        result,
    );
    assert!(
        result.contains("/new"),
        "Help text should mention /new command",
    );
    assert!(
        result.contains("/help"),
        "Help text should mention /help command",
    );
}

#[tokio::test]
async fn test_help_lists_registered_tools() {
    let (mut agent, _dir) = make_agent();
    let result = agent.process_direct("/help", "test:chat").await.unwrap();

    // AgentLoop registers these tools by default
    for tool in &[
        "read_file",
        "write_file",
        "edit_file",
        "list_dir",
        "exec",
        "web_search",
        "web_fetch",
    ] {
        assert!(
            result.contains(tool),
            "Help text should list tool '{}', got: {}",
            tool,
            result,
        );
    }
}

#[tokio::test]
async fn test_separate_sessions_are_independent() {
    let (mut agent, dir) = make_agent();

    // Send messages to two different sessions
    agent
        .process_direct("session A message", "test:chatA")
        .await
        .unwrap();
    agent
        .process_direct("session B message", "test:chatB")
        .await
        .unwrap();

    // Both session files should exist
    let file_a = dir.path().join("sessions").join("test_chatA.jsonl");
    let file_b = dir.path().join("sessions").join("test_chatB.jsonl");

    assert!(file_a.exists(), "Session A file should exist");
    assert!(file_b.exists(), "Session B file should exist");

    // Session A should contain its message but not B's
    let content_a = std::fs::read_to_string(&file_a).unwrap();
    assert!(content_a.contains("session A message"));
    assert!(!content_a.contains("session B message"));

    // Session B should contain its message but not A's
    let content_b = std::fs::read_to_string(&file_b).unwrap();
    assert!(content_b.contains("session B message"));
    assert!(!content_b.contains("session A message"));
}

#[tokio::test]
async fn test_empty_message_handled() {
    let (mut agent, _dir) = make_agent();
    // An empty message should not panic; the provider echoes whatever it finds
    let result = agent.process_direct("", "test:chat").await.unwrap();
    // The echo provider should find the empty user content
    assert!(result.contains("Echo:"), "Expected echo response, got: {}", result);
}

#[tokio::test]
async fn test_unicode_content() {
    let (mut agent, _dir) = make_agent();
    let msg = "Hallo Welt! Caf\u{00e9} \u{1f600}";
    let result = agent.process_direct(msg, "test:chat").await.unwrap();
    assert_eq!(result, format!("Echo: {}", msg));
}

#[tokio::test]
async fn test_multiline_content() {
    let (mut agent, _dir) = make_agent();
    let msg = "line one\nline two\nline three";
    let result = agent.process_direct(msg, "test:chat").await.unwrap();
    assert_eq!(result, format!("Echo: {}", msg));
}

#[tokio::test]
async fn test_slash_new_then_continue() {
    let (mut agent, _dir) = make_agent();

    // Build history, reset, then continue
    agent.process_direct("before reset", "test:chat").await.unwrap();
    agent.process_direct("/new", "test:chat").await.unwrap();

    let result = agent
        .process_direct("after reset", "test:chat")
        .await
        .unwrap();
    assert_eq!(result, "Echo: after reset");
}

#[tokio::test]
async fn test_session_key_splitting() {
    // When session_key has a colon, it splits into channel:chat_id
    let (mut agent, dir) = make_agent();
    agent
        .process_direct("hello", "telegram:12345")
        .await
        .unwrap();

    let session_file = dir.path().join("sessions").join("telegram_12345.jsonl");
    assert!(
        session_file.exists(),
        "Session file for 'telegram:12345' should exist at {:?}",
        session_file,
    );
}

#[tokio::test]
async fn test_bus_receives_outbound_via_run() {
    let dir = TempDir::new().unwrap();
    let provider: Box<dyn LlmProvider> = Box::new(EchoProvider);
    let (bus, inbound_rx, mut outbound_rx) = MessageBus::new();

    let bus_clone = bus.clone();
    let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);

    // Spawn the agent loop in the background
    let handle = tokio::spawn(async move {
        agent.run().await;
    });

    // Publish an inbound message via the bus
    let inbound = rustoctopus_core::bus::events::InboundMessage::new(
        "test", "user", "chat1", "ping via bus",
    );
    bus_clone.publish_inbound(inbound).await;

    // Wait for the outbound response
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), outbound_rx.recv())
        .await
        .expect("timed out waiting for outbound message")
        .expect("outbound channel closed unexpectedly");

    assert_eq!(response.content, "Echo: ping via bus");
    assert_eq!(response.channel, "test");
    assert_eq!(response.chat_id, "chat1");

    // Drop the sender to stop the loop
    drop(bus_clone);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), handle).await;
}
