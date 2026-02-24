# Nanobot Rust Port — Phase 2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a runnable `nanobot-cli` binary with CLI commands (agent, gateway, onboard, status, cron) and Telegram/Feishu channel support, enabling end-to-end testing of the Rust port.

**Architecture:** New `crates/nanobot-cli` binary crate using `clap` for CLI, depending on `nanobot-core`. Add `channels` module to `nanobot-core` with Channel trait, ChannelManager, and Telegram/Feishu implementations. Add config helpers (workspace path resolution, provider factory) to bridge config → runtime objects.

**Tech Stack:** clap (CLI), teloxide (Telegram), reqwest + tokio-tungstenite (Feishu), rustyline (interactive input), crossterm (terminal), tokio (async runtime)

---

### Task 1: Config Helpers — Workspace Resolution + Provider Factory

**Files:**
- Modify: `crates/nanobot-core/src/config/loader.rs`
- Modify: `crates/nanobot-core/src/config/mod.rs`
- Create: `crates/nanobot-core/src/config/factory.rs`
- Test: `crates/nanobot-core/src/config/factory.rs` (inline tests)

**Context:** `AgentDefaults.workspace` stores `"~/.nanobot/workspace"` as a raw string. We need tilde expansion. We also need a factory to create an `OpenAiCompatClient` from config (model string → ProviderSpec → client).

**Step 1: Write failing tests for `resolve_workspace_path` and `create_provider_from_config`**

```rust
// In factory.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{Config, ProviderConfig};

    #[test]
    fn test_resolve_workspace_expands_tilde() {
        let path = resolve_workspace_path("~/.nanobot/workspace");
        assert!(!path.to_string_lossy().contains('~'));
        assert!(path.to_string_lossy().contains("nanobot"));
    }

    #[test]
    fn test_resolve_workspace_absolute_unchanged() {
        let path = resolve_workspace_path("/tmp/workspace");
        assert_eq!(path, PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn test_create_provider_anthropic() {
        let mut config = Config::default();
        config.providers.anthropic.api_key = "test-key".to_string();
        config.agents.defaults.model = "anthropic/claude-sonnet-4-20250514".to_string();
        let result = create_provider(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_missing_key() {
        let config = Config::default();
        let result = create_provider(&config);
        // Should still create provider (key can be empty, will fail at runtime)
        assert!(result.is_ok());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p nanobot-core --lib config::factory`
Expected: FAIL — module doesn't exist

**Step 3: Implement `factory.rs`**

```rust
use std::path::PathBuf;
use anyhow::{Result, Context};
use crate::config::schema::Config;
use crate::providers::openai_compat::OpenAiCompatClient;
use crate::providers::registry::find_by_model;
use crate::providers::traits::LlmProvider;

/// Expand `~` to the user's home directory.
pub fn resolve_workspace_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

/// Create an LLM provider from the global config.
///
/// Uses `config.agents.defaults.model` to look up the provider spec,
/// then matches the provider name to the corresponding API key in
/// `config.providers`.
pub fn create_provider(config: &Config) -> Result<Box<dyn LlmProvider>> {
    let model = &config.agents.defaults.model;
    let spec = find_by_model(model)
        .with_context(|| format!("Unknown model: {model}"))?;

    let provider_cfg = get_provider_config(config, spec.name);
    let api_base = provider_cfg.api_base
        .clone()
        .unwrap_or_else(|| spec.api_base.to_string());

    Ok(Box::new(OpenAiCompatClient::new(
        &provider_cfg.api_key,
        &api_base,
        spec.default_model,
        provider_cfg.extra_headers.clone().unwrap_or_default(),
        spec.name,
    )))
}

fn get_provider_config<'a>(config: &'a Config, provider_name: &str) -> &'a crate::config::schema::ProviderConfig {
    match provider_name {
        "anthropic" => &config.providers.anthropic,
        "openai" => &config.providers.openai,
        "openrouter" => &config.providers.openrouter,
        "deepseek" => &config.providers.deepseek,
        "groq" => &config.providers.groq,
        "gemini" => &config.providers.gemini,
        "moonshot" => &config.providers.moonshot,
        "zhipu" => &config.providers.zhipu,
        "dashscope" => &config.providers.dashscope,
        "vllm" => &config.providers.vllm,
        "minimax" => &config.providers.minimax,
        "aihubmix" => &config.providers.aihubmix,
        "siliconflow" => &config.providers.siliconflow,
        "volcengine" => &config.providers.volcengine,
        _ => &config.providers.custom,
    }
}
```

**Step 4: Wire module — add `pub mod factory;` to `config/mod.rs`, re-export key functions**

**Step 5: Run tests to verify they pass**

Run: `cargo test -p nanobot-core --lib config::factory`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/nanobot-core/src/config/factory.rs crates/nanobot-core/src/config/mod.rs
git commit -m "feat: add config factory (workspace path resolution + provider creation)"
```

---

### Task 2: AgentLoop Config-Driven Constructor

**Files:**
- Modify: `crates/nanobot-core/src/agent/agent_loop.rs`

**Context:** Current `AgentLoop::new()` takes raw parts and hardcodes defaults (max_iterations=40, temperature=0.1, etc). Add `AgentLoop::from_config()` that reads from `Config` and applies settings.

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_from_config() {
    let dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.agents.defaults.max_tool_iterations = 10;
    config.agents.defaults.temperature = 0.5;
    config.agents.defaults.max_tokens = 2048;
    config.agents.defaults.memory_window = 50;
    config.agents.defaults.workspace = dir.path().to_string_lossy().to_string();

    let provider = Box::new(MockProvider { response: "ok".into() });
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

    let agent = AgentLoop::from_config(config, bus, provider, inbound_rx);
    // Verify settings applied via process_direct behavior
    let result = agent; // just verify it compiles and constructs
    assert_eq!(result.max_iterations, 10);
}
```

Note: `max_iterations` is currently private. We need to either make it `pub(crate)` for testing or add a getter. Prefer making the fields `pub(crate)`.

**Step 2: Run test to verify it fails**

Run: `cargo test -p nanobot-core --lib agent::agent_loop::tests::test_from_config`
Expected: FAIL — `from_config` doesn't exist

**Step 3: Implement `from_config`**

```rust
use crate::config::schema::Config;
use crate::config::factory::resolve_workspace_path;

impl AgentLoop {
    pub fn from_config(
        config: Config,
        bus: MessageBus,
        provider: Box<dyn LlmProvider>,
        inbound_rx: mpsc::UnboundedReceiver<InboundMessage>,
    ) -> Self {
        let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
        let mut agent = Self::new(bus, provider, workspace, inbound_rx);
        agent.max_iterations = config.agents.defaults.max_tool_iterations as usize;
        agent.temperature = config.agents.defaults.temperature;
        agent.max_tokens = config.agents.defaults.max_tokens;
        agent.memory_window = config.agents.defaults.memory_window as usize;
        agent
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p nanobot-core --lib agent::agent_loop`
Expected: PASS (all existing + new test)

**Step 5: Commit**

```bash
git add crates/nanobot-core/src/agent/agent_loop.rs
git commit -m "feat: add AgentLoop::from_config() config-driven constructor"
```

---

### Task 3: Channel Trait + ChannelManager in nanobot-core

**Files:**
- Create: `crates/nanobot-core/src/channels/mod.rs`
- Create: `crates/nanobot-core/src/channels/traits.rs`
- Create: `crates/nanobot-core/src/channels/manager.rs`
- Modify: `crates/nanobot-core/src/lib.rs` — add `pub mod channels;`

**Context:** Port the Python `BaseChannel` ABC → Rust `Channel` async trait, and `ChannelManager` → Rust struct. The manager consumes `outbound_rx` and dispatches to channels. Channels publish inbound messages to the bus.

**Step 1: Write failing tests**

```rust
// In channels/manager.rs
#[cfg(test)]
mod tests {
    use super::*;

    struct MockChannel {
        name: String,
        started: Arc<AtomicBool>,
        sent: Arc<Mutex<Vec<OutboundMessage>>>,
    }

    #[async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &str { &self.name }
        async fn start(&mut self) -> Result<()> {
            self.started.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn stop(&mut self) -> Result<()> {
            self.started.store(false, Ordering::SeqCst);
            Ok(())
        }
        async fn send(&self, msg: OutboundMessage) -> Result<()> {
            self.sent.lock().unwrap().push(msg);
            Ok(())
        }
        fn is_running(&self) -> bool {
            self.started.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_manager_registers_channel() {
        let (bus, _inbound_rx, outbound_rx) = MessageBus::new();
        let mut mgr = ChannelManager::new(bus, outbound_rx);
        let ch = MockChannel { /* ... */ };
        mgr.add_channel(Box::new(ch));
        assert_eq!(mgr.channel_names().len(), 1);
    }

    #[tokio::test]
    async fn test_manager_dispatches_to_channel() {
        // Create bus, manager with mock channel
        // Publish outbound message via bus
        // Start manager dispatch loop briefly
        // Verify mock channel received the message
    }
}
```

**Step 2: Run tests to verify fail**

**Step 3: Implement Channel trait**

```rust
// traits.rs
use anyhow::Result;
use async_trait::async_trait;
use crate::bus::events::OutboundMessage;

#[async_trait]
pub trait Channel: Send {
    fn name(&self) -> &str;
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn send(&self, msg: OutboundMessage) -> Result<()>;
    fn is_running(&self) -> bool;
}
```

**Step 4: Implement ChannelManager**

```rust
// manager.rs
pub struct ChannelManager {
    bus: MessageBus,
    channels: HashMap<String, Box<dyn Channel>>,
    outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    send_progress: bool,
    send_tool_hints: bool,
}

impl ChannelManager {
    pub fn new(bus: MessageBus, outbound_rx: mpsc::UnboundedReceiver<OutboundMessage>) -> Self;
    pub fn add_channel(&mut self, channel: Box<dyn Channel>);
    pub async fn start_all(&mut self) -> Result<()>;
    pub async fn stop_all(&mut self);
    pub async fn run_dispatch(&mut self);  // consumes outbound_rx, routes to channels
    pub fn channel_names(&self) -> Vec<String>;
}
```

**Step 5: Run tests, verify pass**

**Step 6: Commit**

```bash
git add crates/nanobot-core/src/channels/ crates/nanobot-core/src/lib.rs
git commit -m "feat: add Channel trait and ChannelManager"
```

---

### Task 4: nanobot-cli Crate Scaffolding

**Files:**
- Create: `crates/nanobot-cli/Cargo.toml`
- Create: `crates/nanobot-cli/src/main.rs`
- Modify: `Cargo.toml` (workspace members)

**Context:** Binary crate with clap CLI. Subcommands: agent, gateway, onboard, status, cron.

**Step 1: Create Cargo.toml**

```toml
[package]
name = "nanobot-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "nanobot"
path = "src/main.rs"

[dependencies]
nanobot-core = { path = "../nanobot-core" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
clap = { version = "4", features = ["derive"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
dirs = { workspace = true }
```

**Step 2: Create main.rs with clap skeleton**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nanobot", about = "nanobot - Personal AI Assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Agent { /* flags */ },
    Gateway,
    Onboard,
    Status,
    Cron { #[command(subcommand)] action: CronAction },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    // dispatch to handlers
    Ok(())
}
```

**Step 3: Add workspace member, verify `cargo build -p nanobot-cli` compiles**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/ Cargo.toml
git commit -m "feat: scaffold nanobot-cli crate with clap subcommands"
```

---

### Task 5: CLI Agent Command — Single Message Mode

**Files:**
- Create: `crates/nanobot-cli/src/cmd_agent.rs`
- Modify: `crates/nanobot-cli/src/main.rs`

**Context:** `nanobot agent -m "hello"` loads config, creates provider + bus + AgentLoop, calls `process_direct()`, prints result, exits.

**Step 1: Implement `cmd_agent::run_single()`**

```rust
pub async fn run_single(message: &str, session_id: &str, config: Config) -> Result<()> {
    let provider = create_provider(&config)?;
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let mut agent = AgentLoop::from_config(config, bus, provider, inbound_rx);
    let response = agent.process_direct(message, session_id).await?;
    println!("{}", response);
    Ok(())
}
```

**Step 2: Wire to clap in main.rs**

```rust
Commands::Agent { message, session } => {
    let config = load_config(None)?;
    if let Some(msg) = message {
        cmd_agent::run_single(&msg, &session, config).await?;
    } else {
        cmd_agent::run_interactive(&session, config).await?;
    }
}
```

**Step 3: Verify `cargo build -p nanobot-cli` compiles**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/src/
git commit -m "feat: implement CLI agent command (single message mode)"
```

---

### Task 6: CLI Agent Command — Interactive Mode

**Files:**
- Modify: `crates/nanobot-cli/src/cmd_agent.rs`
- Modify: `crates/nanobot-cli/Cargo.toml` — add `rustyline` dep

**Context:** `nanobot agent` (no -m) enters a REPL. User types messages, gets responses. Handles exit commands. Uses rustyline for readline support (history, editing).

**Step 1: Add rustyline dependency**

**Step 2: Implement `cmd_agent::run_interactive()`**

Two approaches (matching Python):
- **Approach A (simple):** Direct call to `process_direct()` per input — synchronous but simple
- **Approach B (bus-based):** Spawn AgentLoop task, consume outbound — matches gateway architecture

Use Approach A for simplicity (Python's interactive mode also supports direct mode):

```rust
pub async fn run_interactive(session_id: &str, config: Config) -> Result<()> {
    println!("nanobot interactive mode. Type 'exit' to quit.\n");
    let provider = create_provider(&config)?;
    let (bus, inbound_rx, _outbound_rx) = MessageBus::new();
    let mut agent = AgentLoop::from_config(config, bus, provider, inbound_rx);

    let mut rl = rustyline::DefaultEditor::new()?;
    loop {
        match rl.readline("You: ") {
            Ok(line) => {
                let input = line.trim();
                if matches!(input, "exit" | "quit" | "/exit" | "/quit" | ":q") {
                    break;
                }
                if input.is_empty() { continue; }
                rl.add_history_entry(input)?;
                let response = agent.process_direct(input, session_id).await?;
                println!("\nAssistant: {}\n", response);
            }
            Err(_) => break,
        }
    }
    Ok(())
}
```

**Step 3: Verify with `cargo build -p nanobot-cli`**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/
git commit -m "feat: implement CLI agent interactive mode with rustyline"
```

---

### Task 7: CLI Onboard + Status Commands

**Files:**
- Create: `crates/nanobot-cli/src/cmd_onboard.rs`
- Create: `crates/nanobot-cli/src/cmd_status.rs`
- Modify: `crates/nanobot-cli/src/main.rs`

**Context:** `onboard` creates `~/.nanobot/config.json` and workspace directory with template files (AGENTS.md, SOUL.md, etc.). `status` shows config location, model, workspace path, provider API key status.

**Step 1: Implement `cmd_onboard::run()`**

Creates:
- `~/.nanobot/config.json` (default config if missing)
- `~/.nanobot/workspace/` directory
- `~/.nanobot/workspace/AGENTS.md`, `SOUL.md`, `USER.md`, `TOOLS.md`, `IDENTITY.md` (template stubs)
- `~/.nanobot/workspace/skills/` directory
- `~/.nanobot/workspace/sessions/` directory

**Step 2: Implement `cmd_status::run()`**

Reads config and prints:
- Config path + exists?
- Model name
- Workspace path + exists?
- Per-provider API key status (set / not set)
- Enabled channels

**Step 3: Wire both to main.rs, verify `cargo build`**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/src/
git commit -m "feat: implement onboard and status CLI commands"
```

---

### Task 8: CLI Cron Subcommands

**Files:**
- Create: `crates/nanobot-cli/src/cmd_cron.rs`
- Modify: `crates/nanobot-cli/src/main.rs`

**Context:** `nanobot cron list|add|remove|enable|disable|run` — matches Python cron commands. Uses CronService from nanobot-core.

**Step 1: Define CronAction subcommand enum**

```rust
#[derive(Subcommand)]
enum CronAction {
    List,
    Add { schedule: String, message: String, #[arg(long)] session: Option<String> },
    Remove { id: String },
    Enable { id: String },
    Disable { id: String },
    Run { id: String },
}
```

**Step 2: Implement handlers**

Each subcommand creates a CronService with a file-backed store, performs the operation, prints result.

**Step 3: Wire to main.rs, verify `cargo build`**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/src/
git commit -m "feat: implement cron CLI subcommands"
```

---

### Task 9: Telegram Channel Implementation

**Files:**
- Create: `crates/nanobot-core/src/channels/telegram.rs`
- Modify: `crates/nanobot-core/src/channels/mod.rs`
- Modify: `crates/nanobot-core/Cargo.toml` — add `teloxide` dep

**Context:** Port Python TelegramChannel. Uses `teloxide` crate for Telegram Bot API with long polling. Handles text messages, sends responses with markdown. ACL via `allow_from` config.

**Step 1: Add teloxide dependency**

```toml
teloxide = { version = "0.13", features = ["macros"] }
```

**Step 2: Implement TelegramChannel**

```rust
pub struct TelegramChannel {
    config: TelegramConfig,
    bus: MessageBus,
    running: Arc<AtomicBool>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str { "telegram" }

    async fn start(&mut self) -> Result<()> {
        // Create teloxide Bot from token
        // Set up message handler that:
        //   1. Checks ACL via allow_from
        //   2. Creates InboundMessage
        //   3. Publishes to bus
        // Start long polling
    }

    async fn send(&self, msg: OutboundMessage) -> Result<()> {
        // Send text via bot.send_message()
        // Handle message splitting for long responses
    }

    async fn stop(&mut self) -> Result<()> {
        // Signal shutdown
    }
}
```

Key features to port:
- ACL check (allow_from list, composite sender IDs with `|`)
- `/start`, `/new`, `/help` command handling
- Text message forwarding to bus
- Response sending (split long messages)
- Markdown → HTML conversion (simplified version)

**Step 3: Unit tests with mock bot (ACL logic, message splitting)**

**Step 4: Commit**

```bash
git add crates/nanobot-core/src/channels/telegram.rs crates/nanobot-core/src/channels/mod.rs crates/nanobot-core/Cargo.toml
git commit -m "feat: implement Telegram channel with teloxide"
```

---

### Task 10: Feishu Channel Implementation

**Files:**
- Create: `crates/nanobot-core/src/channels/feishu.rs`
- Modify: `crates/nanobot-core/src/channels/mod.rs`
- Modify: `crates/nanobot-core/Cargo.toml` — add `tokio-tungstenite`, `url` deps

**Context:** Port Python FeishuChannel. No Rust Feishu SDK exists — implement using reqwest (REST API for sending) + tokio-tungstenite (WebSocket for receiving). Key differences from Python: all async-native (no thread bridging needed).

**Step 1: Add dependencies**

```toml
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
url = "2"
```

**Step 2: Implement FeishuChannel**

Core components:
- **Auth**: Get tenant_access_token via `POST /open-apis/auth/v3/tenant_access_token/internal`
- **WebSocket**: Connect to Feishu's long connection endpoint for receiving messages
- **Send**: POST to `/open-apis/im/v1/messages` with card/text content
- **Dedup**: OrderedSet (or LRU) for message_id deduplication

```rust
pub struct FeishuChannel {
    config: FeishuConfig,
    bus: MessageBus,
    running: Arc<AtomicBool>,
    http: reqwest::Client,
    token: Arc<RwLock<String>>,  // tenant_access_token, refreshed periodically
}
```

**Step 3: Tests for ACL, message content extraction, card building**

**Step 4: Commit**

```bash
git add crates/nanobot-core/src/channels/feishu.rs crates/nanobot-core/Cargo.toml
git commit -m "feat: implement Feishu channel with WebSocket + REST API"
```

---

### Task 11: CLI Gateway Command — Full Server Mode

**Files:**
- Create: `crates/nanobot-cli/src/cmd_gateway.rs`
- Modify: `crates/nanobot-cli/src/main.rs`

**Context:** `nanobot gateway` starts the full server: MessageBus + AgentLoop + ChannelManager + CronService, all running concurrently. Graceful shutdown on Ctrl+C.

**Step 1: Implement `cmd_gateway::run()`**

```rust
pub async fn run(config: Config) -> Result<()> {
    // 1. Create bus
    let (bus, inbound_rx, outbound_rx) = MessageBus::new();

    // 2. Create provider
    let provider = create_provider(&config)?;

    // 3. Create AgentLoop
    let mut agent = AgentLoop::from_config(config.clone(), bus.clone(), provider, inbound_rx);

    // 4. Create ChannelManager + register enabled channels
    let mut channel_mgr = ChannelManager::new(bus.clone(), outbound_rx);
    if config.channels.telegram.enabled {
        channel_mgr.add_channel(Box::new(TelegramChannel::new(config.channels.telegram.clone(), bus.clone())));
    }
    if config.channels.feishu.enabled {
        channel_mgr.add_channel(Box::new(FeishuChannel::new(config.channels.feishu.clone(), bus.clone())));
    }

    // 5. Create CronService (optional)
    // ...

    // 6. Spawn all tasks
    let agent_handle = tokio::spawn(async move { agent.run().await });
    let channel_handle = tokio::spawn(async move { channel_mgr.run_dispatch().await });

    // 7. Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    // 8. Graceful shutdown
    // Drop bus senders to signal shutdown
    // Await handles

    Ok(())
}
```

**Step 2: Wire to main.rs**

**Step 3: Verify `cargo build`**

**Step 4: Commit**

```bash
git add crates/nanobot-cli/src/
git commit -m "feat: implement gateway command (full server mode)"
```

---

### Task 12: Integration Tests

**Files:**
- Create: `crates/nanobot-cli/tests/cli_integration.rs`
- Modify: `crates/nanobot-core/tests/integration.rs` — add channel tests

**Context:** Test the full pipeline: config → provider → agent → response. Test channel manager dispatch. Test CLI argument parsing.

**Step 1: Write integration tests**

```rust
// Test config factory
#[test]
fn test_config_to_provider_roundtrip() { ... }

// Test channel manager with mock channel
#[tokio::test]
async fn test_channel_manager_dispatch() { ... }

// Test CLI arg parsing
#[test]
fn test_cli_parse_agent_single() { ... }

// Test agent with from_config
#[tokio::test]
async fn test_agent_from_config_process() { ... }
```

**Step 2: Run all tests**

Run: `cargo test --workspace`
Expected: All pass

**Step 3: Commit**

```bash
git add crates/
git commit -m "test: add Phase 2 integration tests"
```

---

### Task 13: Cleanup, Clippy, Final Verification

**Files:**
- All modified files

**Step 1: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Fix all warnings.

**Step 2: Run full test suite**

Run: `cargo test --workspace`
Expected: All pass (Phase 1 + Phase 2 tests)

**Step 3: Verify binary runs**

Run: `cargo run -p nanobot-cli -- --help`
Expected: Shows help with all subcommands

Run: `cargo run -p nanobot-cli -- onboard`
Expected: Creates ~/.nanobot/ structure

Run: `cargo run -p nanobot-cli -- status`
Expected: Shows config status

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: Phase 2 cleanup — clippy fixes and final polish"
```

---

## Verification Plan

1. **Unit tests**: `cargo test --workspace` — all tests pass
2. **Clippy**: `cargo clippy --workspace -- -D warnings` — no warnings
3. **Build**: `cargo build --release -p nanobot-cli` — compiles
4. **Onboard**: `cargo run -p nanobot-cli -- onboard` — creates ~/.nanobot/
5. **Status**: `cargo run -p nanobot-cli -- status` — shows config
6. **Agent single**: `cargo run -p nanobot-cli -- agent -m "hello"` — gets LLM response (requires API key)
7. **Agent interactive**: `cargo run -p nanobot-cli -- agent` — enters REPL
8. **Gateway**: `cargo run -p nanobot-cli -- gateway` — starts with Telegram (requires token)

## New Dependencies Summary

| Crate | Version | Purpose |
|-------|---------|---------|
| clap | 4 | CLI argument parsing |
| rustyline | 14 | Interactive readline |
| teloxide | 0.13 | Telegram Bot API |
| tokio-tungstenite | 0.24 | WebSocket (Feishu) |
| url | 2 | URL parsing (Feishu) |
