use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use super::traits::Channel;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::queue::MessageBus;
use crate::config::schema::FeishuConfig;

/// Feishu API base URL.
const FEISHU_API: &str = "https://open.feishu.cn/open-apis";

/// Maximum dedup cache size.
const MAX_DEDUP_SIZE: usize = 1000;

/// Maximum message length before splitting.
const MAX_MESSAGE_LEN: usize = 4000;

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TokenRequest<'a> {
    app_id: &'a str,
    app_secret: &'a str,
}

#[derive(Deserialize)]
struct TokenResponse {
    code: i32,
    tenant_access_token: Option<String>,
    msg: Option<String>,
}

#[derive(Serialize)]
struct SendMessageRequest<'a> {
    receive_id: &'a str,
    msg_type: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct SendMessageResponse {
    code: i32,
    msg: Option<String>,
}

#[derive(Deserialize)]
struct WsEndpointResponse {
    code: i32,
    data: Option<WsEndpointData>,
    msg: Option<String>,
}

#[derive(Deserialize)]
struct WsEndpointData {
    #[serde(rename = "URL")]
    url: Option<String>,
}

/// Feishu WebSocket frame (simplified).
#[derive(Deserialize, Debug)]
struct WsEvent {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    event_type: Option<String>,
    header: Option<WsEventHeader>,
    event: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct WsEventHeader {
    event_type: Option<String>,
}

// ---------------------------------------------------------------------------
// FeishuChannel
// ---------------------------------------------------------------------------

/// Feishu/Lark channel using reqwest (REST) + tokio-tungstenite (WebSocket).
pub struct FeishuChannel {
    config: FeishuConfig,
    bus: MessageBus,
    running: Arc<AtomicBool>,
    http: reqwest::Client,
    token: Arc<RwLock<String>>,
    dedup: Arc<Mutex<VecDeque<String>>>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl FeishuChannel {
    pub fn new(config: FeishuConfig, bus: MessageBus) -> Self {
        Self {
            config,
            bus,
            running: Arc::new(AtomicBool::new(false)),
            http: reqwest::Client::new(),
            token: Arc::new(RwLock::new(String::new())),
            dedup: Arc::new(Mutex::new(VecDeque::new())),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Refresh the tenant_access_token.
    async fn refresh_token(
        http: &reqwest::Client,
        app_id: &str,
        app_secret: &str,
        token: &RwLock<String>,
    ) -> Result<()> {
        let resp: TokenResponse = http
            .post(format!("{}/auth/v3/tenant_access_token/internal", FEISHU_API))
            .json(&TokenRequest { app_id, app_secret })
            .send()
            .await?
            .json()
            .await?;

        if resp.code != 0 {
            anyhow::bail!(
                "Feishu token error (code={}): {}",
                resp.code,
                resp.msg.unwrap_or_default()
            );
        }

        let new_token = resp
            .tenant_access_token
            .context("Missing tenant_access_token in response")?;

        *token.write().await = new_token;
        debug!("Feishu tenant token refreshed");
        Ok(())
    }

    /// Send a message via Feishu REST API.
    async fn send_text(
        http: &reqwest::Client,
        token: &RwLock<String>,
        chat_id: &str,
        text: &str,
    ) -> Result<()> {
        let content = serde_json::json!({ "text": text }).to_string();
        let token_val = token.read().await.clone();

        let resp: SendMessageResponse = http
            .post(format!(
                "{}/im/v1/messages?receive_id_type=chat_id",
                FEISHU_API
            ))
            .bearer_auth(&token_val)
            .json(&SendMessageRequest {
                receive_id: chat_id,
                msg_type: "text",
                content: &content,
            })
            .send()
            .await?
            .json()
            .await?;

        if resp.code != 0 {
            warn!(
                "Feishu send_message error (code={}): {}",
                resp.code,
                resp.msg.unwrap_or_default()
            );
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for FeishuChannel {
    fn name(&self) -> &str {
        "feishu"
    }

    async fn start(&mut self) -> Result<()> {
        // Refresh token
        Self::refresh_token(
            &self.http,
            &self.config.app_id,
            &self.config.app_secret,
            &self.token,
        )
        .await?;

        self.running.store(true, Ordering::SeqCst);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let running = Arc::clone(&self.running);
        let bus = self.bus.clone();
        let http = self.http.clone();
        let token = Arc::clone(&self.token);
        let dedup = Arc::clone(&self.dedup);
        let app_id = self.config.app_id.clone();
        let app_secret = self.config.app_secret.clone();
        let allow_from = self.config.allow_from.clone();

        tokio::spawn(async move {
            info!("Feishu channel started");
            tokio::select! {
                _ = ws_loop(running.clone(), bus, http, token, dedup, app_id, app_secret, allow_from) => {}
                _ = shutdown_rx => {
                    info!("Feishu channel received shutdown signal");
                }
            }
            running.store(false, Ordering::SeqCst);
            info!("Feishu channel stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<()> {
        for chunk in split_message(&msg.content, MAX_MESSAGE_LEN) {
            Self::send_text(&self.http, &self.token, &msg.chat_id, &chunk).await?;
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// WebSocket receive loop with auto-reconnect.
#[allow(clippy::too_many_arguments)]
async fn ws_loop(
    running: Arc<AtomicBool>,
    bus: MessageBus,
    http: reqwest::Client,
    token: Arc<RwLock<String>>,
    dedup: Arc<Mutex<VecDeque<String>>>,
    app_id: String,
    app_secret: String,
    allow_from: Vec<String>,
) {
    while running.load(Ordering::SeqCst) {
        // Refresh token before connecting
        if let Err(e) =
            FeishuChannel::refresh_token(&http, &app_id, &app_secret, &token).await
        {
            warn!("Feishu token refresh failed: {}", e);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        // Get WebSocket endpoint
        let ws_url = match get_ws_endpoint(&http, &token).await {
            Ok(url) => url,
            Err(e) => {
                warn!("Failed to get Feishu WS endpoint: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        info!("Connecting to Feishu WebSocket...");

        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                info!("Feishu WebSocket connected");
                use futures_util::StreamExt;
                let (_, mut read) = ws_stream.split();

                loop {
                    match read.next().await {
                        Some(Ok(msg)) => {
                            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                                handle_ws_message(
                                    &text,
                                    &bus,
                                    &dedup,
                                    &allow_from,
                                )
                                .await;
                            }
                        }
                        Some(Err(e)) => {
                            warn!("Feishu WebSocket error: {}", e);
                            break;
                        }
                        None => {
                            info!("Feishu WebSocket closed");
                            break;
                        }
                    }

                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to connect Feishu WebSocket: {}", e);
            }
        }

        if running.load(Ordering::SeqCst) {
            info!("Reconnecting Feishu WebSocket in 5s...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}

/// Get the WebSocket long-connection endpoint URL.
async fn get_ws_endpoint(http: &reqwest::Client, token: &RwLock<String>) -> Result<String> {
    let token_val = token.read().await.clone();
    let resp: WsEndpointResponse = http
        .post(format!("{}/callback/ws/endpoint", FEISHU_API))
        .bearer_auth(&token_val)
        .json(&serde_json::json!({}))
        .send()
        .await?
        .json()
        .await?;

    if resp.code != 0 {
        anyhow::bail!(
            "Feishu WS endpoint error (code={}): {}",
            resp.code,
            resp.msg.unwrap_or_default()
        );
    }

    resp.data
        .and_then(|d| d.url)
        .context("Missing WebSocket URL in response")
}

/// Handle a single WebSocket message frame.
async fn handle_ws_message(
    raw: &str,
    bus: &MessageBus,
    dedup: &Mutex<VecDeque<String>>,
    allow_from: &[String],
) {
    let event: WsEvent = match serde_json::from_str(raw) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Only process message receive events
    let is_message_event = event
        .header
        .as_ref()
        .and_then(|h| h.event_type.as_deref())
        == Some("im.message.receive_v1");

    if !is_message_event {
        return;
    }

    let Some(event_data) = event.event else {
        return;
    };

    // Extract message fields
    let message = match event_data.get("message") {
        Some(m) => m,
        None => return,
    };

    let message_id = message
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    // Dedup
    {
        let mut cache = dedup.lock().await;
        if cache.contains(&message_id.to_string()) {
            return;
        }
        cache.push_back(message_id.to_string());
        while cache.len() > MAX_DEDUP_SIZE {
            cache.pop_front();
        }
    }

    // Extract sender
    let sender = event_data.get("sender");
    let sender_id = sender
        .and_then(|s| s.get("sender_id"))
        .and_then(|s| s.get("open_id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let sender_type = sender
        .and_then(|s| s.get("sender_type"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    // Skip bot messages
    if sender_type == "bot" {
        return;
    }

    // ACL check
    if !is_allowed(sender_id, allow_from) {
        warn!("Feishu access denied for sender {}", sender_id);
        return;
    }

    let chat_id = message
        .get("chat_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let msg_type = message
        .get("message_type")
        .and_then(|v| v.as_str())
        .unwrap_or("text");

    let content_str = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract text content based on message type
    let content = extract_content(msg_type, content_str);

    if content.is_empty() {
        return;
    }

    debug!(
        "Feishu message from {} in {}: {}...",
        sender_id,
        chat_id,
        &content[..content.len().min(50)]
    );

    let inbound = InboundMessage::new("feishu", sender_id, chat_id, &content);
    bus.publish_inbound(inbound).await;
}

/// Extract text content from Feishu message based on type.
fn extract_content(msg_type: &str, content_json: &str) -> String {
    match msg_type {
        "text" => {
            // {"text":"hello"} or {"text":"@_user_1 hello"}
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(content_json) {
                let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
                // Strip @mentions (format: @_user_N)
                let cleaned = text
                    .split_whitespace()
                    .filter(|w| !w.starts_with("@_user_"))
                    .collect::<Vec<_>>()
                    .join(" ");
                cleaned.trim().to_string()
            } else {
                content_json.to_string()
            }
        }
        "post" => {
            // Rich text — extract plain text from nested structure
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(content_json) {
                extract_post_text(&v)
            } else {
                content_json.to_string()
            }
        }
        _ => {
            format!("[{}]", msg_type)
        }
    }
}

/// Extract plain text from a Feishu "post" message.
fn extract_post_text(post: &serde_json::Value) -> String {
    let mut parts = Vec::new();

    // Try to find content under a locale key (zh_cn, en_us, etc.)
    let content = if let Some(obj) = post.as_object() {
        obj.values()
            .next()
            .and_then(|locale| locale.get("content"))
    } else {
        post.get("content")
    };

    if let Some(paragraphs) = content.and_then(|c| c.as_array()) {
        for paragraph in paragraphs {
            let mut para_text = String::new();
            if let Some(elements) = paragraph.as_array() {
                for elem in elements {
                    if let Some(text) = elem.get("text").and_then(|t| t.as_str()) {
                        para_text.push_str(text);
                    }
                }
            }
            let trimmed = para_text.trim().to_string();
            if !trimmed.is_empty() {
                parts.push(trimmed);
            }
        }
    }

    parts.join(" ")
}

/// ACL check (same logic as Telegram).
fn is_allowed(sender_id: &str, allow_from: &[String]) -> bool {
    if allow_from.is_empty() {
        return true;
    }
    if allow_from.iter().any(|a| a == sender_id) {
        return true;
    }
    if sender_id.contains('|') {
        for part in sender_id.split('|') {
            if !part.is_empty() && allow_from.iter().any(|a| a == part) {
                return true;
            }
        }
    }
    false
}

/// Split message into chunks (same logic as Telegram).
fn split_message(content: &str, max_len: usize) -> Vec<String> {
    if content.len() <= max_len {
        return vec![content.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = content;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let cut = &remaining[..max_len];
        let pos = cut.rfind('\n').or_else(|| cut.rfind(' ')).unwrap_or(max_len);
        chunks.push(remaining[..pos].to_string());
        remaining = remaining[pos..].trim_start();
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_content_text() {
        let content = extract_content("text", r#"{"text":"hello world"}"#);
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_extract_content_text_with_mention() {
        let content = extract_content("text", r#"{"text":"@_user_1 hello"}"#);
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_extract_content_post() {
        let post = r#"{"zh_cn":{"content":[[{"tag":"text","text":"hello "},{"tag":"text","text":"world"}]]}}"#;
        let content = extract_content("post", post);
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_extract_content_unknown_type() {
        let content = extract_content("image", "{}");
        assert_eq!(content, "[image]");
    }

    #[test]
    fn test_is_allowed_empty() {
        assert!(is_allowed("anyone", &[]));
    }

    #[test]
    fn test_is_allowed_match() {
        let list = vec!["user123".to_string()];
        assert!(is_allowed("user123", &list));
        assert!(!is_allowed("other", &list));
    }

    #[test]
    fn test_dedup_logic() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let dedup = Mutex::new(VecDeque::new());

            // Add an entry
            {
                let mut cache = dedup.lock().await;
                cache.push_back("msg1".to_string());
            }

            // Check it's there
            {
                let cache = dedup.lock().await;
                assert!(cache.contains(&"msg1".to_string()));
                assert!(!cache.contains(&"msg2".to_string()));
            }
        });
    }

    #[test]
    fn test_extract_post_text_nested() {
        let json = serde_json::json!({
            "en_us": {
                "title": "Test",
                "content": [
                    [
                        {"tag": "text", "text": "Line 1"},
                        {"tag": "text", "text": " continued"}
                    ],
                    [
                        {"tag": "text", "text": "Line 2"}
                    ]
                ]
            }
        });
        let text = extract_post_text(&json);
        assert_eq!(text, "Line 1 continued Line 2");
    }
}
