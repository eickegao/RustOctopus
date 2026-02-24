use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use super::traits::{Tool, ToolError};
use crate::bus::{MessageBus, OutboundMessage};

// ---------------------------------------------------------------------------
// Internal mutable state
// ---------------------------------------------------------------------------

struct MessageToolState {
    default_channel: String,
    default_chat_id: String,
    sent_in_turn: bool,
}

// ---------------------------------------------------------------------------
// MessageTool
// ---------------------------------------------------------------------------

pub struct MessageTool {
    bus: MessageBus,
    state: Mutex<MessageToolState>,
}

impl MessageTool {
    pub fn new(bus: MessageBus) -> Self {
        Self {
            bus,
            state: Mutex::new(MessageToolState {
                default_channel: String::new(),
                default_chat_id: String::new(),
                sent_in_turn: false,
            }),
        }
    }

    /// Update the default routing context for outbound messages.
    pub fn set_context(&self, channel: &str, chat_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.default_channel = channel.to_string();
        state.default_chat_id = chat_id.to_string();
    }

    /// Reset per-turn tracking. Called at the start of each agent turn.
    pub fn start_turn(&self) {
        let mut state = self.state.lock().unwrap();
        state.sent_in_turn = false;
    }

    /// Returns whether a message was sent during the current turn.
    pub fn sent_in_turn(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.sent_in_turn
    }
}

#[async_trait]
impl Tool for MessageTool {
    fn name(&self) -> &str {
        "message"
    }

    fn description(&self) -> &str {
        "Send a message to the user. Use this when you want to communicate something."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The message content to send"
                },
                "channel": {
                    "type": "string",
                    "description": "Optional: target channel (telegram, discord, etc.)"
                },
                "chat_id": {
                    "type": "string",
                    "description": "Optional: target chat/user ID"
                },
                "media": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional: list of file paths to attach (images, audio, documents)"
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let content = params["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: content".into()))?;

        let (channel, chat_id) = {
            let state = self.state.lock().unwrap();
            let channel = params["channel"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| state.default_channel.clone());
            let chat_id = params["chat_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| state.default_chat_id.clone());
            (channel, chat_id)
        };

        if channel.is_empty() || chat_id.is_empty() {
            return Err(ToolError::ExecutionFailed(
                "No target channel/chat specified".to_string(),
            ));
        }

        let media: Vec<String> = params["media"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let mut msg = OutboundMessage::new(&channel, &chat_id, content);
        msg.media = media.clone();

        self.bus.publish_outbound(msg).await;

        // Mark that we sent a message this turn
        {
            let mut state = self.state.lock().unwrap();
            state.sent_in_turn = true;
        }

        let media_info = if media.is_empty() {
            String::new()
        } else {
            format!(" with {} attachments", media.len())
        };

        Ok(format!("Message sent to {channel}:{chat_id}{media_info}"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_message_tool_sends() {
        let (bus, _inbound_rx, mut outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        tool.set_context("telegram", "123");

        let result = tool.execute(json!({"content": "hello"})).await.unwrap();
        assert!(result.contains("Message sent"));
        assert!(result.contains("telegram:123"));

        let msg = outbound_rx.recv().await.unwrap();
        assert_eq!(msg.content, "hello");
        assert_eq!(msg.channel, "telegram");
        assert_eq!(msg.chat_id, "123");
    }

    #[tokio::test]
    async fn test_message_tool_with_media() {
        let (bus, _inbound_rx, mut outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        tool.set_context("discord", "456");

        let result = tool
            .execute(json!({
                "content": "check this out",
                "media": ["photo.jpg", "doc.pdf"]
            }))
            .await
            .unwrap();

        assert!(result.contains("2 attachments"));

        let msg = outbound_rx.recv().await.unwrap();
        assert_eq!(msg.media.len(), 2);
        assert_eq!(msg.media[0], "photo.jpg");
    }

    #[tokio::test]
    async fn test_message_tool_override_channel() {
        let (bus, _inbound_rx, mut outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        tool.set_context("telegram", "100");

        let result = tool
            .execute(json!({
                "content": "hi",
                "channel": "discord",
                "chat_id": "999"
            }))
            .await
            .unwrap();

        assert!(result.contains("discord:999"));

        let msg = outbound_rx.recv().await.unwrap();
        assert_eq!(msg.channel, "discord");
        assert_eq!(msg.chat_id, "999");
    }

    #[tokio::test]
    async fn test_message_tool_no_context() {
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        // No set_context called

        let result = tool.execute(json!({"content": "hello"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("No target channel/chat"),
            "Expected no-context error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_message_tool_missing_content() {
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        tool.set_context("telegram", "123");

        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("content"),
            "Expected missing content error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_sent_in_turn_tracking() {
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let tool = MessageTool::new(bus);
        tool.set_context("telegram", "123");

        assert!(!tool.sent_in_turn());

        tool.execute(json!({"content": "hello"})).await.unwrap();
        assert!(tool.sent_in_turn());

        tool.start_turn();
        assert!(!tool.sent_in_turn());
    }
}
