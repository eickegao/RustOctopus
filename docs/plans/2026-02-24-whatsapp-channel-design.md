# WhatsApp Channel Design

## Overview

Add WhatsApp as a communication channel to rustoctopus-core, mirroring the architecture of the original Python nanobot. WhatsApp lacks an open Bot API, so the implementation uses a **Node.js bridge** (based on `@whiskeysockets/baileys`) that speaks WhatsApp Web protocol, with the Rust channel connecting to it via local WebSocket.

## Architecture

```
┌─────────────┐   WebSocket    ┌──────────────┐  baileys   ┌──────────┐
│  Rust        │◄─────────────►│  Node.js      │◄──────────►│ WhatsApp │
│  WhatsApp    │  127.0.0.1    │  Bridge       │  Web Proto │ Servers  │
│  Channel     │  :3001        │  (child proc) │            │          │
└─────────────┘               └──────────────┘            └──────────┘
```

### Data Flow

- **Inbound**: WhatsApp → baileys → Bridge WS broadcast `{type:"message",...}` → Rust Channel → MessageBus
- **Outbound**: MessageBus → Rust Channel → Bridge WS `{type:"send",to,text}` → baileys → WhatsApp

## Components

### 1. Node.js Bridge (`bridge/`)

Copied from the original nanobot project ([HKUDS/nanobot](https://github.com/HKUDS/nanobot/tree/main/bridge)). Contains:

- `src/whatsapp.ts` — Baileys client wrapper (QR auth, message extraction, reconnect)
- `src/server.ts` — Local WebSocket server (`ws://127.0.0.1:PORT`), optional token auth
- `src/index.ts` — CLI entry point, env-based config (`BRIDGE_PORT`, `AUTH_DIR`, `BRIDGE_TOKEN`)
- `package.json` — Dependencies: `@whiskeysockets/baileys`, `ws`, `qrcode-terminal`, `pino`

The bridge is a standalone Node.js process managed by the Rust channel as a child process.

### 2. WhatsAppConfig (`config/schema.rs`)

```rust
pub struct WhatsAppConfig {
    pub enabled: bool,
    pub allow_from: Vec<String>,        // phone numbers or sender IDs
    pub bridge_port: u16,               // default: 3001
    pub bridge_token: Option<String>,   // optional auth token
    pub auto_start_bridge: bool,        // default: true
}
```

Added to `ChannelsConfig` alongside `telegram` and `feishu`.

### 3. WhatsAppChannel (`channels/whatsapp.rs`)

Implements the `Channel` trait. Key behaviors:

- **`start()`**: If `auto_start_bridge`, spawn Node.js bridge as child process via `tokio::process::Command`. Then establish WebSocket connection to `ws://127.0.0.1:{bridge_port}`. If `bridge_token` is set, send auth handshake as first message. Start message receive loop in a spawned task.
- **`send()`**: Send JSON `{"type":"send","to":"<chat_id>","text":"<content>"}` over WebSocket. Split long messages if needed.
- **`stop()`**: Close WebSocket connection, kill bridge child process.
- **Reconnect**: On WebSocket disconnect, retry every 5 seconds (matching original behavior).
- **ACL**: Use `is_allowed()` pattern (same as Telegram/Feishu).
- **Message parsing**: Handle bridge messages of type `message`, `status`, `qr`, `error`.

### 4. Feature Gate

```toml
# crates/rustoctopus-core/Cargo.toml
[features]
whatsapp = ["tokio-tungstenite", "futures-util"]  # already available from feishu feature
```

Module gated with `#[cfg(feature = "whatsapp")]`.

### 5. Gateway Integration (`cmd_gateway.rs`)

Same pattern as Telegram/Feishu:

```rust
if config.channels.whatsapp.enabled {
    let whatsapp = WhatsAppChannel::new(config.channels.whatsapp.clone(), bus.clone());
    channel_mgr.add_channel(Box::new(whatsapp));
}
```

### 6. Bridge Process Management

The Rust channel manages the bridge lifecycle:

1. On `start()`: Check if bridge is already running (try connect first). If not, spawn `node bridge/dist/index.js` with env vars (`BRIDGE_PORT`, `BRIDGE_TOKEN`, `AUTH_DIR`).
2. Wait briefly for bridge to initialize, then connect WebSocket.
3. On `stop()`: Send SIGTERM to child process, wait with timeout, then SIGKILL if needed.
4. Bridge stdout/stderr forwarded to tracing logs.

## Bridge WebSocket Protocol

### Inbound (Bridge → Rust)

```json
{"type": "message", "sender": "1234567890@s.whatsapp.net", "content": "hello", "timestamp": 1708000000, "isGroup": false}
{"type": "qr", "qr": "2@..."}
{"type": "status", "status": "open"}
{"type": "error", "error": "..."}
```

### Outbound (Rust → Bridge)

```json
{"type": "auth", "token": "..."}       // optional, first message if token configured
{"type": "send", "to": "...", "text": "..."}
```

### Response (Bridge → Rust, after send)

```json
{"type": "sent", "to": "..."}
{"type": "error", "error": "..."}
```

## Config Example

```json
{
  "channels": {
    "whatsapp": {
      "enabled": true,
      "allowFrom": ["+1234567890"],
      "bridgePort": 3001,
      "bridgeToken": "optional-secret",
      "autoStartBridge": true
    }
  }
}
```

## Files Changed

| File | Change |
|------|--------|
| `bridge/` (new dir) | Copy from original nanobot |
| `crates/rustoctopus-core/src/channels/whatsapp.rs` | New: WhatsAppChannel implementation |
| `crates/rustoctopus-core/src/channels/mod.rs` | Add whatsapp module + feature gate |
| `crates/rustoctopus-core/src/config/schema.rs` | Add WhatsAppConfig, update ChannelsConfig |
| `crates/rustoctopus-core/Cargo.toml` | Add `whatsapp` feature |
| `crates/rustoctopus-cli/src/cmd_gateway.rs` | Register WhatsApp channel |
| `crates/rustoctopus-cli/Cargo.toml` | Add `whatsapp` feature passthrough |
