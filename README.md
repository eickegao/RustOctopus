<p align="center">
  <img src="assets/icon.png" width="180" alt="RustOctopus Logo" />
</p>

<h1 align="center">RustOctopus</h1>

<p align="center">
  A multi-channel, tool-capable AI assistant framework written in Rust.
</p>

<p align="center">
  <a href="#features">Features</a> &bull;
  <a href="#architecture">Architecture</a> &bull;
  <a href="#quick-start">Quick Start</a> &bull;
  <a href="#usage">Usage</a> &bull;
  <a href="#configuration">Configuration</a> &bull;
  <a href="#building">Building</a> &bull;
  <a href="#license">License</a>
</p>

---

RustOctopus is a personal AI assistant that accepts messages from multiple chat platforms, processes them through LLMs with tool execution capabilities, and returns intelligent responses. It runs as a CLI tool, a headless gateway, or a cross-platform desktop app with system tray support.

## Features

- **Multi-channel support** — Telegram, Feishu (Lark), WhatsApp, and CLI REPL
- **16 LLM providers** — Anthropic, OpenAI, DeepSeek, Gemini, OpenRouter, Zhipu, DashScope, Moonshot, MiniMax, Groq, and more
- **Tool system** — File read/write/edit, shell execution, web search, web fetch, cron scheduling, message sending, subagent spawning
- **Dual-layer memory** — Long-term facts (MEMORY.md) + searchable history log (HISTORY.md), with LLM-triggered consolidation
- **Skill system** — Extensible skill definitions via workspace `skills/` directory
- **Session persistence** — JSONL-based conversation history per channel/chat
- **Cron jobs** — Scheduled tasks with `every`, `at`, and cron expression support
- **Desktop app** — Tauri-based GUI with system tray (macOS, Windows, Linux)
- **Single binary** — No Python dependency, fast startup, low memory footprint

## Architecture

```
User (Telegram / Feishu / WhatsApp / CLI / GUI)
          |
          v
    Channel Layer
          |
          v
    MessageBus (tokio mpsc)
          |
          v
    AgentLoop (context builder -> LLM -> tool loop)
          |
          v
    LLM Provider (OpenAI-compatible HTTP client)
          |
          v
    ToolRegistry (filesystem, shell, web, cron, message, spawn)
```

### Project Structure

```
rustoctopus/
├── crates/
│   ├── rustoctopus-core/     # Core library: agent, bus, channels, config, cron, providers, session, tools
│   ├── rustoctopus-cli/      # CLI binary (roc): agent, gateway, onboard, status, cron commands
│   └── rustoctopus-app/      # Tauri desktop app with React frontend
├── bridge/                   # Node.js WhatsApp bridge (Baileys + WebSocket)
├── assets/                   # App icon
└── docs/plans/               # Design documents
```

## Quick Start

### Prerequisites

- Rust 1.75+ (with cargo)
- Node.js 18+ (for WhatsApp bridge and GUI frontend)
- An API key for at least one LLM provider

### Install and Setup

```bash
# Clone the repository
git clone https://github.com/your-username/rustoctopus.git
cd rustoctopus

# Build the CLI
cargo build --release -p rustoctopus-cli

# Run the setup wizard
./target/release/roc onboard

# Edit the config to add your API key and enable channels
# vim ~/.rustoctopus/config.json
```

### Run the Desktop App

```bash
# Install frontend dependencies
cd crates/rustoctopus-app/ui && npm install && cd -

# Run in development mode
cd crates/rustoctopus-app && cargo tauri dev
```

## Usage

### CLI Mode

```bash
# Interactive REPL
roc agent

# Single message
roc agent -m "What is the capital of France?"

# With a specific session
roc agent -s "cli:myproject"
```

### Gateway Mode

Start all enabled channels and the agent loop as a background service:

```bash
roc gateway
```

### Desktop App

Launch the Tauri desktop application. It embeds the gateway and runs as a system tray app — closing the window hides it to tray, and the service continues running.

### Cron Jobs

```bash
# List jobs
roc cron list

# Add a recurring job
roc cron add "every 30m" "check for new emails"

# Add a cron-expression job
roc cron add "0 0 9 * * *" "good morning briefing"

# Remove a job
roc cron remove <job-id>
```

## Configuration

Config file: `~/.rustoctopus/config.json`

```json
{
  "agents": {
    "defaults": {
      "workspace": "~/.rustoctopus/workspace",
      "model": "openai/gpt-4o",
      "maxTokens": 8192,
      "temperature": 0.1,
      "maxToolIterations": 40,
      "memoryWindow": 100
    }
  },
  "providers": {
    "openai": { "apiKey": "sk-..." },
    "anthropic": { "apiKey": "sk-ant-..." },
    "deepseek": { "apiKey": "..." }
  },
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "123456:ABC...",
      "allowFrom": []
    },
    "feishu": { "enabled": false, "appId": "", "appSecret": "" },
    "whatsapp": { "enabled": false, "bridgePort": 3001 }
  }
}
```

### Model naming

Use `provider/model` format: `openai/gpt-4o`, `anthropic/claude-sonnet-4-5`, `deepseek/deepseek-chat`. The provider prefix is automatically stripped when calling the API.

### Workspace

The workspace at `~/.rustoctopus/workspace/` contains:

| File | Purpose |
|------|---------|
| `IDENTITY.md` | Assistant name and identity |
| `SOUL.md` | Personality and values |
| `USER.md` | Information about the user |
| `AGENTS.md` | Agent personas and behaviors |
| `TOOLS.md` | Custom tool documentation |
| `memory/MEMORY.md` | Long-term facts (LLM-updated) |
| `memory/HISTORY.md` | Append-only timestamped log |
| `sessions/*.jsonl` | Conversation sessions |
| `skills/*/SKILL.md` | Custom skill definitions |

## Supported Providers

| Provider | Key Prefix / Detection | Notes |
|----------|----------------------|-------|
| Anthropic | `anthropic/claude-*` | Claude models |
| OpenAI | `openai/gpt-*` | GPT models |
| DeepSeek | `deepseek/*` | DeepSeek models |
| Gemini | `gemini/*` | Google Gemini |
| OpenRouter | `sk-or-` key prefix | Gateway to any model |
| Zhipu AI | `zhipu/*`, `glm-*` | GLM models |
| DashScope | `qwen-*` | Qwen models |
| Moonshot | `moonshot-*`, `kimi-*` | Kimi models |
| MiniMax | `minimax-*` | MiniMax models |
| Groq | `groq/*` | Fast inference |
| AiHubMix | URL contains `aihubmix` | Gateway |
| SiliconFlow | URL contains `siliconflow` | Gateway |
| VolcEngine | URL contains `volces` | Gateway |
| vLLM | `vllm/*` | Local deployment |

## Building

### CLI only

```bash
cargo build --release -p rustoctopus-cli
```

### Desktop app

```bash
cd crates/rustoctopus-app
cargo tauri build
```

This produces platform-specific installers:
- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.AppImage` / `.deb`

### Feature flags

```bash
# Build without WhatsApp support
cargo build --release -p rustoctopus-cli --no-default-features

# Build with specific channels
cargo build --release -p rustoctopus-cli --features telegram,feishu
```

## Security

- WhatsApp bridge WebSocket binds to `127.0.0.1` only
- All channels support `allowFrom` ACL lists
- Filesystem tools support directory sandboxing
- Shell `exec` tool blocks dangerous commands (rm -rf /, fork bombs, etc.)
- API keys stored locally in `~/.rustoctopus/config.json`

## License

[MIT](LICENSE)
