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
            channel: channel.to_string(),
            sender_id: sender_id.to_string(),
            chat_id: chat_id.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            media: Vec::new(),
            metadata: HashMap::new(),
            session_key_override: None,
        }
    }

    /// Returns `session_key_override` if set, otherwise `"channel:chat_id"`.
    pub fn session_key(&self) -> String {
        if let Some(ref key) = self.session_key_override {
            key.clone()
        } else {
            format!("{}:{}", self.channel, self.chat_id)
        }
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
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            content: content.to_string(),
            reply_to: None,
            media: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}
