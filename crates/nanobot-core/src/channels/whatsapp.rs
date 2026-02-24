use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Child;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::traits::Channel;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::queue::MessageBus;
use crate::config::schema::WhatsAppConfig;

/// Maximum message length before splitting.
const MAX_MESSAGE_LEN: usize = 4000;

// ---------------------------------------------------------------------------
// Bridge protocol types
// ---------------------------------------------------------------------------

/// Events received from the WhatsApp bridge via WebSocket.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[allow(dead_code)] // Fields are populated by serde deserialization
pub(crate) enum BridgeEvent {
    /// An incoming WhatsApp message.
    #[serde(rename = "message")]
    Message {
        #[serde(default)]
        id: String,
        sender: String,
        #[serde(default)]
        pn: String,
        #[serde(default)]
        content: String,
        #[serde(default)]
        timestamp: u64,
        #[serde(default, rename = "isGroup")]
        is_group: bool,
    },
    /// QR code for pairing.
    #[serde(rename = "qr")]
    Qr {
        #[serde(default)]
        qr: String,
    },
    /// Bridge status change.
    #[serde(rename = "status")]
    Status {
        #[serde(default)]
        status: String,
        #[serde(default)]
        message: Option<String>,
    },
    /// Error from the bridge.
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: Option<String>,
    },
    /// Confirmation that a message was sent.
    #[serde(rename = "sent")]
    Sent {
        #[serde(default, alias = "messageId")]
        message_id: Option<String>,
    },
}

/// Authentication handshake sent to the bridge on connect.
#[derive(Serialize, Debug)]
pub(crate) struct AuthMessage<'a> {
    #[serde(rename = "type")]
    pub msg_type: &'a str,
    pub token: &'a str,
}

/// Command to send a message through the bridge.
#[derive(Serialize, Debug)]
pub(crate) struct SendCommand<'a> {
    #[serde(rename = "type")]
    pub msg_type: &'a str,
    pub to: &'a str,
    pub text: &'a str,
}

// ---------------------------------------------------------------------------
// Type alias for the WebSocket write half
// ---------------------------------------------------------------------------

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    tokio_tungstenite::tungstenite::Message,
>;

// ---------------------------------------------------------------------------
// WhatsAppChannel
// ---------------------------------------------------------------------------

/// WhatsApp channel that communicates with a local Node.js bridge via WebSocket.
pub struct WhatsAppChannel {
    config: WhatsAppConfig,
    bus: MessageBus,
    running: Arc<AtomicBool>,
    ws_sink: Arc<Mutex<Option<WsSink>>>,
    bridge_child: Arc<Mutex<Option<Child>>>,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl WhatsAppChannel {
    pub fn new(config: WhatsAppConfig, bus: MessageBus) -> Self {
        Self {
            config,
            bus,
            running: Arc::new(AtomicBool::new(false)),
            ws_sink: Arc::new(Mutex::new(None)),
            bridge_child: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn start(&mut self) -> Result<()> {
        // Optionally start the bridge child process
        if self.config.auto_start_bridge {
            match start_bridge_process(self.config.bridge_port, &self.config) {
                Ok(child) => {
                    info!("WhatsApp bridge process started (pid={})", child.id().unwrap_or(0));
                    *self.bridge_child.lock().await = Some(child);
                    // Give the bridge a moment to start up
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                Err(e) => {
                    warn!("Failed to start WhatsApp bridge process: {}. Assuming external bridge.", e);
                }
            }
        }

        self.running.store(true, Ordering::SeqCst);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.shutdown_tx.lock().await = Some(shutdown_tx);

        let running = Arc::clone(&self.running);
        let bus = self.bus.clone();
        let ws_sink = Arc::clone(&self.ws_sink);
        let port = self.config.bridge_port;
        let token = self.config.bridge_token.clone();
        let allow_from = self.config.allow_from.clone();

        tokio::spawn(async move {
            info!("WhatsApp channel started");
            tokio::select! {
                _ = ws_loop(running.clone(), bus, ws_sink, port, token, allow_from) => {}
                _ = shutdown_rx => {
                    info!("WhatsApp channel received shutdown signal");
                }
            }
            running.store(false, Ordering::SeqCst);
            info!("WhatsApp channel stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(());
        }
        self.running.store(false, Ordering::SeqCst);

        // Close WebSocket
        {
            let mut sink = self.ws_sink.lock().await;
            if let Some(ref mut ws) = *sink {
                use futures_util::SinkExt;
                let _ = ws.close().await;
            }
            *sink = None;
        }

        // Kill bridge child process
        {
            let mut child = self.bridge_child.lock().await;
            if let Some(ref mut proc) = *child {
                info!("Killing WhatsApp bridge process");
                let _ = proc.kill().await;
            }
            *child = None;
        }

        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let mut sink_guard = self.ws_sink.lock().await;
        let sink = sink_guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("WhatsApp WebSocket not connected"))?;

        use futures_util::SinkExt;
        for chunk in split_message(&msg.content, MAX_MESSAGE_LEN) {
            let cmd = SendCommand {
                msg_type: "send",
                to: &msg.chat_id,
                text: &chunk,
            };
            let payload = serde_json::to_string(&cmd)?;
            sink.send(tokio_tungstenite::tungstenite::Message::Text(payload))
                .await
                .map_err(|e| anyhow::anyhow!("WhatsApp WS send error: {}", e))?;
        }

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// WebSocket receive loop with auto-reconnect.
async fn ws_loop(
    running: Arc<AtomicBool>,
    bus: MessageBus,
    ws_sink: Arc<Mutex<Option<WsSink>>>,
    port: u16,
    token: Option<String>,
    allow_from: Vec<String>,
) {
    while running.load(Ordering::SeqCst) {
        let ws_url = format!("ws://127.0.0.1:{}", port);
        info!("Connecting to WhatsApp bridge at {}...", ws_url);

        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                info!("WhatsApp bridge WebSocket connected");
                use futures_util::{SinkExt, StreamExt};
                let (mut write, mut read) = ws_stream.split();

                // Optional auth handshake
                if let Some(ref tok) = token {
                    let auth = AuthMessage {
                        msg_type: "auth",
                        token: tok,
                    };
                    if let Ok(json) = serde_json::to_string(&auth) {
                        if let Err(e) = write
                            .send(tokio_tungstenite::tungstenite::Message::Text(json))
                            .await
                        {
                            warn!("WhatsApp auth handshake failed: {}", e);
                        }
                    }
                }

                // Store the write half for outbound messages
                *ws_sink.lock().await = Some(write);

                // Read loop
                loop {
                    match read.next().await {
                        Some(Ok(msg)) => {
                            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                                handle_bridge_message(&text, &bus, &allow_from).await;
                            }
                        }
                        Some(Err(e)) => {
                            warn!("WhatsApp WebSocket error: {}", e);
                            break;
                        }
                        None => {
                            info!("WhatsApp WebSocket closed by bridge");
                            break;
                        }
                    }

                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                }

                // Clear the write half on disconnect
                *ws_sink.lock().await = None;
            }
            Err(e) => {
                warn!("Failed to connect to WhatsApp bridge: {}", e);
            }
        }

        if running.load(Ordering::SeqCst) {
            info!("Reconnecting to WhatsApp bridge in 5s...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}

/// Parse and handle a single bridge event.
async fn handle_bridge_message(raw: &str, bus: &MessageBus, allow_from: &[String]) {
    let event: BridgeEvent = match serde_json::from_str(raw) {
        Ok(e) => e,
        Err(e) => {
            debug!("Ignoring unparseable bridge message: {}", e);
            return;
        }
    };

    match event {
        BridgeEvent::Message {
            id: _,
            sender,
            pn,
            content,
            timestamp: _,
            is_group: _,
        } => {
            if content.is_empty() {
                return;
            }

            // Build composite sender_id: "phone|pushName"
            let sender_id = if !pn.is_empty() {
                format!("{}|{}", sender, pn)
            } else {
                sender.clone()
            };

            // ACL check
            if !is_allowed(&sender_id, allow_from) {
                warn!("WhatsApp access denied for sender {}", sender_id);
                return;
            }

            // For WhatsApp, sender is the chat JID (remoteJid)
            let effective_chat_id = &sender;

            debug!(
                "WhatsApp message from {} in {}: {}...",
                sender_id,
                effective_chat_id,
                &content[..content.len().min(50)]
            );

            let inbound = InboundMessage::new("whatsapp", &sender_id, effective_chat_id, &content);
            bus.publish_inbound(inbound).await;
        }
        BridgeEvent::Qr { qr } => {
            info!("WhatsApp QR code received (length={}). Scan to pair.", qr.len());
        }
        BridgeEvent::Status { status, message } => {
            info!(
                "WhatsApp bridge status: {} {}",
                status,
                message.unwrap_or_default()
            );
        }
        BridgeEvent::Error { message } => {
            error!("WhatsApp bridge error: {}", message.unwrap_or_default());
        }
        BridgeEvent::Sent { message_id } => {
            debug!(
                "WhatsApp message sent (id={})",
                message_id.unwrap_or_default()
            );
        }
    }
}

/// ACL check (same logic as telegram/feishu).
fn is_allowed(sender_id: &str, allow_from: &[String]) -> bool {
    if allow_from.is_empty() {
        return true;
    }
    if allow_from.iter().any(|a| a == sender_id) {
        return true;
    }
    // Check composite parts (e.g. "phone|pushName")
    if sender_id.contains('|') {
        for part in sender_id.split('|') {
            if !part.is_empty() && allow_from.iter().any(|a| a == part) {
                return true;
            }
        }
    }
    false
}

/// Split message into chunks respecting the message length limit.
///
/// Prefers splitting on newlines, then spaces, then at max length.
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

/// Attempt to start the Node.js WhatsApp bridge as a child process.
fn start_bridge_process(port: u16, config: &WhatsAppConfig) -> Result<Child> {
    let bridge_dir = find_bridge_dir()?;
    let dist_entry = bridge_dir.join("dist").join("index.js");
    if !dist_entry.exists() {
        anyhow::bail!(
            "Bridge not built. Run 'npm install && npm run build' in {}",
            bridge_dir.display()
        );
    }

    let mut cmd = tokio::process::Command::new("node");
    cmd.arg(&dist_entry)
        .env("BRIDGE_PORT", port.to_string())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);

    if let Some(ref token) = config.bridge_token {
        cmd.env("BRIDGE_TOKEN", token);
    }

    let auth_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".nanobot")
        .join("whatsapp-auth");
    cmd.env("AUTH_DIR", auth_dir.to_string_lossy().to_string());

    let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn bridge process at {:?}: {}", dist_entry, e))?;

    Ok(child)
}

/// Find the bridge/ directory relative to the executable or the current working directory.
fn find_bridge_dir() -> Result<std::path::PathBuf> {
    // Try relative to the executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("bridge");
            if candidate.is_dir() {
                return Ok(candidate);
            }
            // Also check one level up (common in development layouts)
            if let Some(grandparent) = parent.parent() {
                let candidate = grandparent.join("bridge");
                if candidate.is_dir() {
                    return Ok(candidate);
                }
            }
        }
    }

    // Try relative to cwd
    let cwd = std::env::current_dir()?;
    let candidate = cwd.join("bridge");
    if candidate.is_dir() {
        return Ok(candidate);
    }

    anyhow::bail!("Could not find bridge/ directory relative to executable or cwd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_name() {
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let config = WhatsAppConfig::default();
        let channel = WhatsAppChannel::new(config, bus);
        assert_eq!(channel.name(), "whatsapp");
    }

    #[test]
    fn test_is_allowed_empty() {
        assert!(is_allowed("anyone", &[]));
    }

    #[test]
    fn test_is_allowed_match() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890", &list));
        assert!(!is_allowed("+0000000000", &list));
    }

    #[test]
    fn test_is_allowed_composite() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890|Alice", &list));

        let list2 = vec!["Alice".to_string()];
        assert!(is_allowed("+9999999999|Alice", &list2));

        let list3 = vec!["other".to_string()];
        assert!(!is_allowed("+1234567890|Alice", &list3));
    }

    #[test]
    fn test_split_message_short() {
        let chunks = split_message("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_message_at_newline() {
        let msg = format!("{}\n{}", "a".repeat(50), "b".repeat(50));
        let chunks = split_message(&msg, 60);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "a".repeat(50));
        assert_eq!(chunks[1], "b".repeat(50));
    }

    #[test]
    fn test_parse_bridge_message_event() {
        let json = r#"{
            "type": "message",
            "id": "ABC123",
            "sender": "1234567890@s.whatsapp.net",
            "pn": "Alice",
            "content": "Hello from WhatsApp",
            "timestamp": 1700000000,
            "isGroup": false
        }"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        match event {
            BridgeEvent::Message { id, sender, pn, content, timestamp, is_group } => {
                assert_eq!(id, "ABC123");
                assert_eq!(sender, "1234567890@s.whatsapp.net");
                assert_eq!(pn, "Alice");
                assert_eq!(content, "Hello from WhatsApp");
                assert_eq!(timestamp, 1700000000);
                assert!(!is_group);
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_parse_bridge_message_group() {
        let json = r#"{
            "type": "message",
            "id": "DEF456",
            "sender": "120363001234@g.us",
            "pn": "",
            "content": "Group message",
            "timestamp": 1700000001,
            "isGroup": true
        }"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        match event {
            BridgeEvent::Message { is_group, .. } => {
                assert!(is_group);
            }
            _ => panic!("Expected Message event"),
        }
    }

    #[test]
    fn test_send_command_serialization() {
        let cmd = SendCommand {
            msg_type: "send",
            to: "1234567890@s.whatsapp.net",
            text: "Hello!",
        };
        let json = serde_json::to_string(&cmd).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "send");
        assert_eq!(parsed["to"], "1234567890@s.whatsapp.net");
        assert_eq!(parsed["text"], "Hello!");
        // Ensure the old "body" field is not present
        assert!(parsed.get("body").is_none());
    }

    #[test]
    fn test_parse_bridge_qr_event() {
        let json = r#"{"type": "qr", "qr": "2@ABC123"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        match event {
            BridgeEvent::Qr { qr } => {
                assert_eq!(qr, "2@ABC123");
            }
            _ => panic!("Expected Qr event"),
        }
    }

    #[test]
    fn test_parse_bridge_status_event() {
        let json = r#"{"type": "status", "status": "ready", "message": "Connected"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        match event {
            BridgeEvent::Status { status, message } => {
                assert_eq!(status, "ready");
                assert_eq!(message, Some("Connected".to_string()));
            }
            _ => panic!("Expected Status event"),
        }
    }
}
