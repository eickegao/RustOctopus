# RustOctopus MCP Integration Design

**Date:** 2026-02-24 (updated 2026-02-25)
**Status:** Implemented

## Overview

MCP (Model Context Protocol) support is integrated directly into RustOctopus. MCP servers run as child processes managed by `McpManager` in `rustoctopus-core`, using the official `rmcp` crate (Rust MCP SDK by Anthropic). No separate application needed.

## Architecture

```
RustOctopus
├── AgentLoop
│   └── ToolRegistry
│       ├── built-in tools (read_file, exec, web_search, ...)
│       └── McpTool instances (mcp_filesystem_read_file, mcp_github_list_repos, ...)
│           └── delegates to McpManager
│
├── McpManager (Arc<Mutex<...>>)
│   ├── MCP Server "filesystem" (child process via rmcp TokioChildProcess)
│   ├── MCP Server "github" (child process via rmcp TokioChildProcess)
│   └── ...
│
└── Tauri GUI
    └── MCP page (install/remove/toggle servers)
```

## Key Components

### 1. Config (`config/schema.rs`)

```json
{
  "mcp": {
    "enabled": true,
    "servers": {
      "filesystem": {
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/eicke"],
        "env": {},
        "enabled": true,
        "autoApprove": []
      }
    }
  }
}
```

### 2. McpManager (`mcp/manager.rs`)

Manages running MCP server instances using the `rmcp` crate:
- `start_all()` / `start_server()` — spawn child process, MCP handshake, discover tools
- `call_tool()` — invoke a tool on a running server
- `server_statuses()` — report status for UI
- `is_auto_approved()` — check per-tool approval config

### 3. McpTool (`tools/mcp_tool.rs`)

Implements the `Tool` trait, bridging MCP tools into `ToolRegistry`:
- `execute()` delegates to `McpManager::call_tool()`
- Tool names namespaced as `mcp_{server}_{tool}`

### 4. AgentLoop Integration (`agent/agent_loop.rs`)

On startup (if `config.mcp.enabled`):
1. Creates `McpManager`, starts all enabled servers
2. Discovers tools from each server
3. Registers `McpTool` instances in `ToolRegistry`
4. LLM can now call MCP tools like any built-in tool

### 5. Tauri GUI (`commands/mcp.rs` + `views/Mcp.tsx`)

IPC commands: `list_mcp_servers`, `add_mcp_server`, `remove_mcp_server`, `toggle_mcp_server`

Frontend: Server card grid with status, enable/disable toggle, add/remove, auto-refresh.

## Approval System (Design — Future)

- Default: all MCP tools require user approval before execution
- `autoApprove` list per server: specified tools skip approval
- "Approve + Remember" action adds tool to `autoApprove`
- Approval UI in Tauri GUI (notification/dialog when tool call arrives)

## Design History

Originally designed as a separate app (RustDrum) communicating via WebSocket. Simplified to in-process integration because:
1. MCP servers are already child processes with natural process isolation
2. One app is simpler for users than two
3. No WebSocket overhead
4. The `rmcp` crate handles all MCP protocol complexity

See `~/Projects/rustdrum/` for the original standalone design (archived).
