# WhatsApp Channel Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a WhatsApp communication channel to RustOctopus, using a Node.js bridge (baileys) managed as a child process, with the Rust channel connecting via local WebSocket.

**Architecture:** Rust `WhatsAppChannel` implements the `Channel` trait and connects to a local Node.js bridge process over WebSocket (`ws://127.0.0.1:3001`). The bridge uses `@whiskeysockets/baileys` to speak WhatsApp Web protocol. The Rust channel auto-spawns the bridge as a child process and handles reconnection.

**Tech Stack:** tokio-tungstenite (WebSocket client), futures-util (stream), tokio::process (child process management), serde_json (protocol messages)

**Design doc:** `docs/plans/2026-02-24-whatsapp-channel-design.md`

---

### Task 1: Copy Node.js Bridge

**Files:**
- Create: `bridge/package.json`
- Create: `bridge/tsconfig.json`
- Create: `bridge/src/index.ts`
- Create: `bridge/src/server.ts`
- Create: `bridge/src/whatsapp.ts`
- Create: `bridge/src/types.d.ts`

**Step 1: Copy bridge files from original nanobot**

Clone/download the bridge directory from https://github.com/HKUDS/nanobot/tree/main/bridge and copy into `bridge/` at the project root. The key files are:

- `package.json` — deps: `@whiskeysockets/baileys`, `ws`, `qrcode-terminal`, `pino`
- `tsconfig.json` — TypeScript config
- `src/index.ts` — Entry point (reads env vars `BRIDGE_PORT`, `AUTH_DIR`, `BRIDGE_TOKEN`)
- `src/server.ts` — WebSocket server on 127.0.0.1, optional token auth, command routing
- `src/whatsapp.ts` — Baileys client wrapper (QR auth, message extraction, reconnect)
- `src/types.d.ts` — Type declarations

**Step 2: Add bridge/ to .gitignore for node_modules and dist**

Add to root `.gitignore`:
```
bridge/node_modules/
bridge/dist/
```

**Step 3: Commit**

```bash
git add bridge/ .gitignore
git commit -m "feat: add Node.js WhatsApp bridge from upstream nanobot"
```

---

### Task 2: Add WhatsAppConfig to config schema

**Files:**
- Modify: `crates/rustoctopus-core/src/config/schema.rs`

**Step 1: Write failing test**

Add to `crates/rustoctopus-core/src/config/schema.rs` (or a new test file) — test that WhatsAppConfig deserializes from JSON with camelCase keys:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core --lib config::schema::tests::test_whatsapp_config_defaults`
Expected: FAIL — `WhatsAppConfig` does not exist yet.

**Step 3: Implement WhatsAppConfig**

Add to `crates/rustoctopus-core/src/config/schema.rs`, after `FeishuConfig`:

```rust
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
```

Then add `whatsapp` field to `ChannelsConfig`:

```rust
pub struct ChannelsConfig {
    pub send_progress: bool,
    pub send_tool_hints: bool,
    pub telegram: TelegramConfig,
    pub feishu: FeishuConfig,
    pub whatsapp: WhatsAppConfig,  // <-- add this
}
```

And update the `Default` impl for `ChannelsConfig`:

```rust
impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            send_progress: true,
            send_tool_hints: false,
            telegram: TelegramConfig::default(),
            feishu: FeishuConfig::default(),
            whatsapp: WhatsAppConfig::default(),  // <-- add this
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core --lib config::schema`
Expected: All PASS.

**Step 5: Commit**

```bash
git commit -am "feat: add WhatsAppConfig to config schema"
```

---

### Task 3: Add whatsapp feature to Cargo.toml

**Files:**
- Modify: `crates/rustoctopus-core/Cargo.toml`
- Modify: `crates/rustoctopus-cli/Cargo.toml`

**Step 1: Update rustoctopus-core Cargo.toml**

Add `whatsapp` to the features section. It reuses the same deps as `feishu` (tokio-tungstenite, futures-util) plus needs `tokio/process` for child process management:

```toml
[features]
default = ["telegram", "feishu", "whatsapp"]
telegram = ["dep:teloxide"]
feishu = ["dep:tokio-tungstenite", "dep:url", "dep:futures-util"]
whatsapp = ["dep:tokio-tungstenite", "dep:url", "dep:futures-util"]
```

**Step 2: Update rustoctopus-cli Cargo.toml**

Add whatsapp feature passthrough:

```toml
[dependencies]
rustoctopus-core = { path = "../rustoctopus-core", features = ["telegram", "feishu", "whatsapp"] }
```

**Step 3: Verify it compiles**

Run: `cargo check -p rustoctopus-core --features whatsapp`
Expected: Compiles without error.

**Step 4: Commit**

```bash
git commit -am "feat: add whatsapp feature flag to Cargo.toml"
```

---

### Task 4: Implement WhatsAppChannel — core struct and Channel trait

**Files:**
- Create: `crates/rustoctopus-core/src/channels/whatsapp.rs`
- Modify: `crates/rustoctopus-core/src/channels/mod.rs`

**Step 1: Write failing test — channel creation and name**

Create `crates/rustoctopus-core/src/channels/whatsapp.rs` with tests at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::queue::MessageBus;
    use crate::config::schema::WhatsAppConfig;

    #[test]
    fn test_channel_name() {
        let config = WhatsAppConfig::default();
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let ch = WhatsAppChannel::new(config, bus);
        assert_eq!(ch.name(), "whatsapp");
    }

    #[test]
    fn test_is_allowed_empty_list() {
        assert!(is_allowed("anyone", &[]));
    }

    #[test]
    fn test_is_allowed_match() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890", &list));
        assert!(!is_allowed("+9999999999", &list));
    }

    #[test]
    fn test_is_allowed_composite() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890|user", &list));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core --features whatsapp --lib channels::whatsapp`
Expected: FAIL — module does not exist.

**Step 3: Implement WhatsAppChannel**

Write the full `crates/rustoctopus-core/src/channels/whatsapp.rs`:

```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::traits::Channel;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::queue::MessageBus;
use crate::config::schema::WhatsAppConfig;

/// Maximum message length before splitting.
const MAX_MESSAGE_LEN: usize = 4000;

/// Reconnect delay in seconds.
const RECONNECT_DELAY_SECS: u64 = 5;

// ---------------------------------------------------------------------------
// Bridge protocol types
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
struct BridgeEvent {
    #[serde(rename = "type")]
    event_type: String,
    // message fields
    sender: Option<String>,
    content: Option<String>,
    timestamp: Option<i64>,
    #[serde(rename = "isGroup")]
    is_group: Option<bool>,
    // qr field
    qr: Option<String>,
    // status field
    status: Option<String>,
    // error field
    error: Option<String>,
}

#[derive(Serialize)]
struct AuthMessage<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
    token: &'a str,
}

#[derive(Serialize)]
struct SendCommand<'a> {
    #[serde(rename = "type")]
    msg_type: &'a str,
    to: &'a str,
    text: &'a str,
}

// ---------------------------------------------------------------------------
// WhatsAppChannel
// ---------------------------------------------------------------------------

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    tokio_tungstenite::tungstenite::Message,
>;

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

    /// Find the bridge directory (bridge/ relative to the executable or cwd).
    fn find_bridge_dir() -> Option<PathBuf> {
        // Try relative to current exe
        if let Ok(exe) = std::env::current_exe() {
            let bridge = exe.parent().unwrap_or(&exe).join("bridge");
            if bridge.join("package.json").exists() {
                return Some(bridge);
            }
        }
        // Try relative to cwd
        let cwd = std::env::current_dir().ok()?;
        let bridge = cwd.join("bridge");
        if bridge.join("package.json").exists() {
            return Some(bridge);
        }
        None
    }

    /// Spawn the Node.js bridge as a child process.
    async fn spawn_bridge(
        config: &WhatsAppConfig,
    ) -> Result<Child> {
        let bridge_dir = Self::find_bridge_dir()
            .ok_or_else(|| anyhow::anyhow!(
                "Cannot find bridge/ directory. Make sure it exists with package.json and dist/index.js"
            ))?;

        let dist_entry = bridge_dir.join("dist").join("index.js");
        if !dist_entry.exists() {
            anyhow::bail!(
                "Bridge not built. Run 'npm install && npm run build' in {}",
                bridge_dir.display()
            );
        }

        info!(dir = %bridge_dir.display(), "Spawning WhatsApp bridge");

        let mut cmd = Command::new("node");
        cmd.arg(&dist_entry)
            .env("BRIDGE_PORT", config.bridge_port.to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        if let Some(ref token) = config.bridge_token {
            cmd.env("BRIDGE_TOKEN", token);
        }

        let auth_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".rustoctopus")
            .join("whatsapp-auth");
        cmd.env("AUTH_DIR", auth_dir.to_string_lossy().to_string());

        let child = cmd.spawn()?;
        info!("WhatsApp bridge spawned (pid: {:?})", child.id());

        // Give the bridge time to start
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        Ok(child)
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    async fn start(&mut self) -> Result<()> {
        // Spawn bridge if configured
        if self.config.auto_start_bridge {
            match Self::spawn_bridge(&self.config).await {
                Ok(child) => {
                    *self.bridge_child.lock().await = Some(child);
                }
                Err(e) => {
                    warn!("Failed to spawn WhatsApp bridge: {}. Attempting to connect to existing bridge.", e);
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
        if let Some(mut sink) = self.ws_sink.lock().await.take() {
            let _ = sink.close().await;
        }

        // Kill bridge child process
        if let Some(mut child) = self.bridge_child.lock().await.take() {
            info!("Stopping WhatsApp bridge child process");
            let _ = child.kill().await;
        }

        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<()> {
        let sink_guard = self.ws_sink.lock().await;
        let Some(_) = sink_guard.as_ref() else {
            anyhow::bail!("WhatsApp WebSocket not connected");
        };
        drop(sink_guard);

        for chunk in split_message(&msg.content, MAX_MESSAGE_LEN) {
            let cmd = SendCommand {
                msg_type: "send",
                to: &msg.chat_id,
                text: &chunk,
            };
            let payload = serde_json::to_string(&cmd)?;
            let mut sink_guard = self.ws_sink.lock().await;
            if let Some(sink) = sink_guard.as_mut() {
                sink.send(tokio_tungstenite::tungstenite::Message::Text(payload.into()))
                    .await?;
            }
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
        let url = format!("ws://127.0.0.1:{}", port);
        info!("Connecting to WhatsApp bridge at {}...", url);

        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws_stream, _)) => {
                info!("WhatsApp bridge WebSocket connected");
                let (mut write, mut read) = ws_stream.split();

                // Send auth if token configured
                if let Some(ref tok) = token {
                    let auth = AuthMessage {
                        msg_type: "auth",
                        token: tok,
                    };
                    let payload = serde_json::to_string(&auth).unwrap();
                    if let Err(e) = write
                        .send(tokio_tungstenite::tungstenite::Message::Text(payload.into()))
                        .await
                    {
                        warn!("Failed to send auth to bridge: {}", e);
                        continue;
                    }
                    debug!("Sent auth token to WhatsApp bridge");
                }

                // Store write half for send()
                *ws_sink.lock().await = Some(write);

                // Read loop
                loop {
                    match read.next().await {
                        Some(Ok(msg)) => {
                            if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                                handle_bridge_message(
                                    &text,
                                    &bus,
                                    &allow_from,
                                )
                                .await;
                            }
                        }
                        Some(Err(e)) => {
                            warn!("WhatsApp WebSocket error: {}", e);
                            break;
                        }
                        None => {
                            info!("WhatsApp WebSocket closed");
                            break;
                        }
                    }

                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                }

                // Clear sink on disconnect
                *ws_sink.lock().await = None;
            }
            Err(e) => {
                warn!("Failed to connect to WhatsApp bridge: {}", e);
            }
        }

        if running.load(Ordering::SeqCst) {
            info!(
                "Reconnecting to WhatsApp bridge in {}s...",
                RECONNECT_DELAY_SECS
            );
            tokio::time::sleep(std::time::Duration::from_secs(RECONNECT_DELAY_SECS)).await;
        }
    }
}

/// Handle a single message from the bridge.
async fn handle_bridge_message(
    raw: &str,
    bus: &MessageBus,
    allow_from: &[String],
) {
    let event: BridgeEvent = match serde_json::from_str(raw) {
        Ok(e) => e,
        Err(e) => {
            debug!("Failed to parse bridge message: {}", e);
            return;
        }
    };

    match event.event_type.as_str() {
        "message" => {
            let sender = event.sender.as_deref().unwrap_or_default();
            let content = event.content.as_deref().unwrap_or_default();

            if sender.is_empty() || content.is_empty() {
                return;
            }

            // ACL check
            if !is_allowed(sender, allow_from) {
                warn!("WhatsApp access denied for sender {}", sender);
                return;
            }

            // Use sender as chat_id for DMs, or group JID for groups
            let chat_id = sender;

            debug!(
                "WhatsApp message from {}: {}...",
                sender,
                &content[..content.len().min(50)]
            );

            let inbound = InboundMessage::new("whatsapp", sender, chat_id, content);
            bus.publish_inbound(inbound).await;
        }
        "qr" => {
            if let Some(qr) = &event.qr {
                info!("WhatsApp QR code received. Scan with your phone to authenticate.");
                debug!("QR data: {}", qr);
            }
        }
        "status" => {
            if let Some(status) = &event.status {
                info!("WhatsApp connection status: {}", status);
            }
        }
        "error" => {
            if let Some(err) = &event.error {
                error!("WhatsApp bridge error: {}", err);
            }
        }
        "sent" => {
            debug!("WhatsApp message sent successfully");
        }
        other => {
            debug!("Unknown bridge message type: {}", other);
        }
    }
}

/// ACL check (same pattern as Telegram/Feishu).
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

/// Split message into chunks.
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
    use crate::bus::queue::MessageBus;
    use crate::config::schema::WhatsAppConfig;

    #[test]
    fn test_channel_name() {
        let config = WhatsAppConfig::default();
        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let ch = WhatsAppChannel::new(config, bus);
        assert_eq!(ch.name(), "whatsapp");
    }

    #[test]
    fn test_is_allowed_empty_list() {
        assert!(is_allowed("anyone", &[]));
    }

    #[test]
    fn test_is_allowed_match() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890", &list));
        assert!(!is_allowed("+9999999999", &list));
    }

    #[test]
    fn test_is_allowed_composite() {
        let list = vec!["+1234567890".to_string()];
        assert!(is_allowed("+1234567890|user", &list));
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
    }

    #[test]
    fn test_parse_bridge_message_event() {
        let json = r#"{"type":"message","sender":"123@s.whatsapp.net","content":"hello","timestamp":1708000000,"isGroup":false}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.sender.as_deref(), Some("123@s.whatsapp.net"));
        assert_eq!(event.content.as_deref(), Some("hello"));
    }

    #[test]
    fn test_parse_bridge_qr_event() {
        let json = r#"{"type":"qr","qr":"2@abc123"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "qr");
        assert_eq!(event.qr.as_deref(), Some("2@abc123"));
    }

    #[test]
    fn test_parse_bridge_status_event() {
        let json = r#"{"type":"status","status":"open"}"#;
        let event: BridgeEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "status");
        assert_eq!(event.status.as_deref(), Some("open"));
    }
}
```

**Step 4: Register module in mod.rs**

Update `crates/rustoctopus-core/src/channels/mod.rs`:

```rust
pub mod manager;
pub mod traits;

#[cfg(feature = "telegram")]
pub mod telegram;

#[cfg(feature = "feishu")]
pub mod feishu;

#[cfg(feature = "whatsapp")]
pub mod whatsapp;

pub use manager::ChannelManager;
pub use traits::Channel;

#[cfg(feature = "telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "feishu")]
pub use feishu::FeishuChannel;

#[cfg(feature = "whatsapp")]
pub use whatsapp::WhatsAppChannel;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core --features whatsapp --lib channels::whatsapp`
Expected: All PASS.

**Step 6: Commit**

```bash
git commit -am "feat: implement WhatsAppChannel with bridge protocol"
```

---

### Task 5: Integrate into Gateway

**Files:**
- Modify: `crates/rustoctopus-cli/src/cmd_gateway.rs`
- Modify: `crates/rustoctopus-cli/src/cmd_status.rs`

**Step 1: Update cmd_gateway.rs to register WhatsApp channel**

Add after the Feishu channel registration block:

```rust
#[cfg(feature = "whatsapp")]
if config.channels.whatsapp.enabled {
    let whatsapp = rustoctopus_core::channels::WhatsAppChannel::new(
        config.channels.whatsapp.clone(),
        bus.clone(),
    );
    channel_mgr.add_channel(Box::new(whatsapp));
    info!("WhatsApp channel registered");
}
```

Also update the import — add `WhatsAppChannel` if feature-gated. Or just use the full path as above.

**Step 2: Update cmd_status.rs to show WhatsApp status**

Add after the Feishu line in the Channels section:

```rust
println!(
    "  WhatsApp:  {}",
    if config.channels.whatsapp.enabled { "enabled" } else { "disabled" }
);
```

**Step 3: Verify it compiles**

Run: `cargo build -p rustoctopus-cli`
Expected: Compiles without error.

**Step 4: Commit**

```bash
git commit -am "feat: integrate WhatsApp channel into gateway and status"
```

---

### Task 6: Build verification and final cleanup

**Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

**Step 2: Run clippy**

Run: `cargo clippy --workspace --all-features -- -D warnings`
Expected: No warnings.

**Step 3: Verify feature-gated builds**

Run: `cargo check -p rustoctopus-core --no-default-features`
Expected: Compiles (whatsapp module excluded).

Run: `cargo check -p rustoctopus-core --features whatsapp`
Expected: Compiles (whatsapp module included).

**Step 4: Final commit if any cleanup needed**

```bash
git commit -am "chore: WhatsApp channel cleanup and lint fixes"
```
