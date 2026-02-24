# RustOctopus Rust Port - Design Document

**Date:** 2026-02-24
**Status:** Approved

## Motivation

Port RustOctopus from Python to Rust for three core goals:
1. **Extreme performance** — minimal latency, memory footprint, fast startup
2. **Production-grade reliability** — type safety, memory safety, fewer runtime bugs
3. **Single-binary distribution** — no Python dependency, installable desktop app (macOS/Windows/Linux)

## Architecture: Approach B — Core Library + Tauri Shell

Rust workspace with three crates:
- `rustoctopus-core` — pure logic library, zero UI dependencies
- `rustoctopus-cli` — thin CLI binary using clap
- `rustoctopus-app` — Tauri desktop application (React + TypeScript frontend)

The GUI serves as a **full control console** (config, monitoring, chat, cron, memory, skills management), while **core interaction remains through chat channels** (Telegram, Feishu, etc.).

## Project Structure

```
rustoctopus/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── rustoctopus-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── agent/
│   │       │   ├── mod.rs
│   │       │   ├── loop.rs       # Agent Loop main loop
│   │       │   ├── context.rs    # Context builder (system prompt)
│   │       │   ├── memory.rs     # Dual-layer memory system
│   │       │   ├── subagent.rs   # Subagent manager
│   │       │   └── skills.rs     # Skills loader
│   │       ├── tools/
│   │       │   ├── mod.rs
│   │       │   ├── registry.rs   # Tool trait + dynamic registration
│   │       │   ├── filesystem.rs # read_file, write_file, edit_file, list_dir
│   │       │   ├── shell.rs      # exec (tokio::process)
│   │       │   ├── web.rs        # web_search, web_fetch
│   │       │   ├── message.rs    # Message sending
│   │       │   ├── spawn.rs      # Subagent spawning
│   │       │   └── cron.rs       # Cron tool
│   │       ├── providers/
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs     # LlmProvider trait + LlmResponse
│   │       │   ├── registry.rs   # ProviderSpec static registry
│   │       │   └── openai_compat.rs  # Unified OpenAI-compatible client
│   │       ├── channels/
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs     # Channel trait
│   │       │   ├── manager.rs    # ChannelManager
│   │       │   ├── telegram.rs   # Phase 2
│   │       │   └── feishu.rs     # Phase 2
│   │       ├── bus/
│   │       │   ├── mod.rs
│   │       │   └── queue.rs      # tokio::mpsc message bus
│   │       ├── session/
│   │       │   ├── mod.rs
│   │       │   └── manager.rs    # JSONL session persistence
│   │       ├── cron/
│   │       │   ├── mod.rs
│   │       │   ├── service.rs
│   │       │   └── types.rs
│   │       └── config/
│   │           ├── mod.rs
│   │           ├── schema.rs     # serde config structs
│   │           └── loader.rs     # JSON load/save
│   │
│   ├── rustoctopus-cli/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── main.rs           # clap CLI (agent, gateway, status, cron...)
│   │
│   └── rustoctopus-app/              # Tauri GUI (Phase 3)
│       ├── Cargo.toml
│       ├── src-tauri/
│       │   └── src/
│       │       ├── main.rs
│       │       ├── commands/     # Tauri IPC commands
│       │       └── state.rs      # Arc<AppState>
│       └── src/                  # React + TypeScript frontend
│           ├── App.tsx
│           └── views/
│               ├── Chat.tsx
│               ├── Dashboard.tsx
│               ├── Config.tsx
│               ├── Channels.tsx
│               ├── Cron.tsx
│               ├── Memory.tsx
│               └── Skills.tsx
│
├── docs/plans/
└── tests/                        # Integration tests
```

## Core Type System

### Message Bus

```rust
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub media: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub session_key_override: Option<String>,
}

pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
    pub reply_to: Option<String>,
    pub media: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

// tokio::mpsc replaces asyncio.Queue
// Compile-time guarantee: single consumer per channel
pub struct MessageBus {
    inbound_tx: mpsc::UnboundedSender<InboundMessage>,
    inbound_rx: mpsc::UnboundedReceiver<InboundMessage>,
    outbound_tx: mpsc::UnboundedSender<OutboundMessage>,
    outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
}
```

### LLM Provider

```rust
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub enum FinishReason { Stop, ToolCalls, MaxTokens, Error }

pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
    pub reasoning_content: Option<String>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
        tools: Option<&[ToolDefinition]>,
        model: &str,
        params: &ChatParams,
    ) -> Result<LlmResponse>;

    fn default_model(&self) -> &str;
}
```

### Tool System

```rust
pub enum ToolError {
    InvalidParams(String),
    ExecutionFailed(String),
    NotFound(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}
```

### Channel

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&mut self) -> Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    fn is_running(&self) -> bool;
}
```

### Config

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]  // Compatible with Python config.json
pub struct Config {
    pub agents: AgentsConfig,
    pub channels: ChannelsConfig,
    pub providers: ProvidersConfig,
    pub gateway: GatewayConfig,
    pub tools: ToolsConfig,
}
```

## Data Flow

```
                         ┌─────────────────────────────┐
                         │        rustoctopus-core          │
  Telegram ──┐           │  ┌────────┐    ┌──────────┐  │
  Feishu   ──┼─ inbound ─┼─►│ Agent  │───►│ Provider │──┼──► LLM API
  CLI      ──┘   (mpsc)  │  │  Loop  │◄───│ (reqwest)│◄─┼───  Response
                         │  └───┬────┘    └──────────┘  │
                         │      │                       │
                         │      ▼                       │
                         │  ┌────────┐                  │
                         │  │ Tools  │ ← exec/file/web  │
                         │  └────────┘                  │
                         │      │                       │
  Telegram ◄─┐           │      │                       │
  Feishu   ◄─┼─ outbound─┼──────┘                       │
  CLI      ◄─┘   (mpsc)  │                              │
                         └─────────────────────────────┘
```

### Agent Loop Core Logic

1. Receive message from inbound mpsc channel (with timeout)
2. Handle slash commands (/new, /help)
3. Trigger async memory consolidation if threshold exceeded
4. Build context: system prompt + history + current message
5. Iterate: LLM call → tool execution → LLM call → ... until text response or max iterations
6. Save turn to session, send response via outbound channel

### Memory Consolidation (dual-layer, same as Python version)

- MEMORY.md — long-term facts, updated by LLM
- HISTORY.md — grep-searchable timestamped log, append-only
- Triggered asynchronously via tokio::spawn when unconsolidated message count exceeds memory_window

## OpenAI-Compatible Provider Layer

### Provider Registry (declarative, static)

```rust
pub struct ProviderSpec {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
    pub env_key: &'static str,
    pub display_name: &'static str,
    pub default_api_base: &'static str,
    pub chat_path: &'static str,
    pub model_prefix: &'static str,
    pub strip_model_prefix: bool,
    pub is_gateway: bool,
    pub is_oauth: bool,
    pub supports_prompt_caching: bool,
    pub detect_by_key_prefix: &'static str,
    pub detect_by_base_keyword: &'static str,
}

pub static PROVIDERS: &[ProviderSpec] = &[ /* ... */ ];
```

### Unified HTTP Client

Single `OpenAiCompatClient` struct handles all OpenAI-compatible providers.
Differences driven by `ProviderSpec` — no if-elif chains.
`reqwest::Client` provides connection pooling.
Anthropic native API handled via format adapter when direct-connecting (not through gateway).

## Key Differences from Python Version

| Aspect | Python | Rust |
|--------|--------|------|
| Message bus | asyncio.Queue (any end can recv) | mpsc (compile-time single consumer) |
| Error handling | try/except, errors as strings | Result<T,E> + ? operator, forced handling |
| Concurrency | asyncio.Lock (cooperative) | tokio::sync::Mutex (true async lock) |
| Tool params | **kwargs (runtime check) | serde_json::Value + schema validation |
| Provider registry | Runtime dataclass | Compile-time static |
| Config validation | Pydantic (runtime) | serde (compile-time derive) |
| Finish reason | String ("stop") | Enum (compile-time exhaustive) |

## Rust Dependency Map

| Purpose | Rust Crate | Replaces Python |
|---------|-----------|-----------------|
| Async runtime | tokio | asyncio |
| HTTP client | reqwest | httpx / litellm |
| JSON / Config | serde + serde_json | pydantic |
| CLI | clap | typer |
| Logging | tracing | loguru |
| WebSocket | tokio-tungstenite | websockets |
| Time | chrono | datetime |
| Cron expressions | cron (crate) | croniter |
| Regex | regex | re |
| File glob | glob | pathlib.glob |
| Telegram Bot | teloxide | python-telegram-bot |
| Desktop GUI | tauri (Phase 3) | N/A |
| Frontend | React + TypeScript | N/A |

## Tauri GUI Design

Tauri IPC commands expose core functionality:
- `send_message` / `get_sessions` — agent interaction
- `get_config` / `save_config` — configuration management
- `get_channel_status` / `toggle_channel` — channel control
- `list_cron_jobs` / `add_cron_job` — cron management
- `get_memory` / `update_memory` — memory management

Real-time updates via Tauri event system (`app.emit("agent-message", ...)`)

## Phased Implementation

### Phase 1: Core Engine
- config (serde), bus (tokio::mpsc), providers (registry + openai_compat)
- tools (registry, filesystem, shell, web, message, spawn, cron)
- agent (context, loop, memory, subagent), session, cron
- **Deliverable:** `rustoctopus-core` lib + unit tests

### Phase 2: Channels + CLI
- Telegram channel, Feishu channel
- CLI interactive mode + gateway mode
- ChannelManager, integration tests
- **Deliverable:** `rustoctopus-cli` binary, feature-equivalent to Python `rustoctopus agent` / `rustoctopus gateway`

### Phase 3: Tauri GUI
- Tauri project scaffold, IPC commands
- React frontend: Chat, Dashboard, Config, Channels, Cron, Memory, Skills views
- Platform packaging (macOS .dmg, Windows .msi, Linux .AppImage)
- **Deliverable:** Installable desktop application

### Phase 4: Extensions (future)
- More channels (Discord, Slack, DingTalk, QQ, Email, WhatsApp)
- MCP protocol support
- OAuth login flows
- Voice transcription (Whisper)
