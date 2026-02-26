# MCP Integration into RustOctopus — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate MCP (Model Context Protocol) support directly into RustOctopus, migrating core logic from the standalone RustDrum project, so that RO can manage MCP servers as child processes and expose their tools to the AgentLoop — with a user approval UI in the Tauri GUI.

**Architecture:** Add an `mcp` module to `rustoctopus-core` using the official `rmcp` SDK crate for MCP protocol communication. The module contains: (1) McpManager using `rmcp::Service` + `TokioChildProcess` to manage MCP server child processes, (2) `McpTool` implementing the existing `Tool` trait to bridge MCP tools into the `ToolRegistry`. The Tauri GUI gets a new "MCP" page for server management. No WebSocket needed — everything runs in-process.

**Tech Stack:** Rust, tokio (async), `rmcp` (official MCP SDK), Tauri 2 IPC, React + TypeScript + Tailwind (frontend)

**Key dependency:** `rmcp` crate — official Rust MCP SDK by Anthropic (https://github.com/modelcontextprotocol/rust-sdk). Provides `TokioChildProcess` transport, `Service` trait with `list_tools()` / `call_tool()`, typed protocol messages.

**Note:** Tasks 2 and 3 from the original plan are removed — `rmcp` handles JSON-RPC protocol and child process management.

---

## Task 1: Add MCP config schema

**Files:**
- Modify: `crates/rustoctopus-core/src/config/schema.rs`
- Test: `crates/rustoctopus-core/src/config/mod.rs` (existing test module)

**Step 1: Write the failing test**

Add to the existing test module in `crates/rustoctopus-core/src/config/mod.rs`:

```rust
#[test]
fn test_mcp_config_defaults() {
    let config: Config = serde_json::from_str("{}").unwrap();
    assert_eq!(config.mcp.servers.len(), 0);
    assert_eq!(config.mcp.enabled, false);
}

#[test]
fn test_mcp_config_deserialize() {
    let json = r#"{
        "mcp": {
            "enabled": true,
            "servers": {
                "filesystem": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
                    "env": {},
                    "enabled": true,
                    "autoApprove": ["read_file"]
                }
            }
        }
    }"#;
    let config: Config = serde_json::from_str(json).unwrap();
    assert!(config.mcp.enabled);
    assert_eq!(config.mcp.servers.len(), 1);
    let fs = &config.mcp.servers["filesystem"];
    assert_eq!(fs.command, "npx");
    assert_eq!(fs.args.len(), 3);
    assert_eq!(fs.auto_approve, vec!["read_file"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core test_mcp_config`
Expected: FAIL — `mcp` field doesn't exist on Config

**Step 3: Write the implementation**

Add to `crates/rustoctopus-core/src/config/schema.rs`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpConfig {
    pub enabled: bool,
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
    pub auto_approve: Vec<String>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            enabled: true,
            auto_approve: Vec::new(),
        }
    }
}
```

Add `mcp: McpConfig` field to the existing `Config` struct.

Add utility functions:

```rust
/// Parse "mcp_filesystem_read_file" → ("filesystem", "read_file")
pub fn parse_mcp_tool_name(namespaced: &str) -> Option<(&str, &str)> {
    let rest = namespaced.strip_prefix("mcp_")?;
    let idx = rest.find('_')?;
    if idx == 0 || idx == rest.len() - 1 {
        return None;
    }
    Some((&rest[..idx], &rest[idx + 1..]))
}

/// Build "mcp_filesystem_read_file" from ("filesystem", "read_file")
pub fn build_mcp_tool_name(server: &str, tool: &str) -> String {
    format!("mcp_{}_{}", server, tool)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core test_mcp_config`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rustoctopus-core/src/config/schema.rs crates/rustoctopus-core/src/config/mod.rs
git commit -m "feat(mcp): add MCP config schema with server definitions"
```

---

## Task 2: MCP JSON-RPC client types

**Files:**
- Create: `crates/rustoctopus-core/src/mcp/mod.rs`
- Create: `crates/rustoctopus-core/src/mcp/client.rs`
- Modify: `crates/rustoctopus-core/src/lib.rs` (add `pub mod mcp;`)

**Reference:** Port from `~/Projects/rustdrum/crates/rustdrum-core/src/mcp/client.rs`

**Step 1: Write the failing test**

In `crates/rustoctopus-core/src/mcp/client.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_serialize() {
        let req = JsonRpcRequest::new(1, "tools/list", None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "tools/list");
    }

    #[test]
    fn test_jsonrpc_response_with_error() {
        let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[test]
    fn test_mcp_tool_def_deserialize() {
        let json = r#"{"name":"read_file","description":"Read a file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}}}}"#;
        let tool: McpToolDef = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "read_file");
        assert!(tool.description.is_some());
    }

    #[test]
    fn test_mcp_tool_result_deserialize() {
        let json = r#"{"content":[{"type":"text","text":"hello"}],"isError":false}"#;
        let result: McpToolResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].text.as_deref(), Some("hello"));
        assert!(!result.is_error);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core mcp::client::tests`
Expected: FAIL — module doesn't exist

**Step 3: Write the implementation**

Create `crates/rustoctopus-core/src/mcp/mod.rs`:
```rust
pub mod client;
pub mod process;
pub mod manager;
```

Create `crates/rustoctopus-core/src/mcp/client.rs` — port the following structs from RustDrum:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    pub fn notification(method: &str, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}
```

Add to `crates/rustoctopus-core/src/lib.rs`:
```rust
pub mod mcp;
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core mcp::client::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rustoctopus-core/src/mcp/ crates/rustoctopus-core/src/lib.rs
git commit -m "feat(mcp): add JSON-RPC client types for MCP protocol"
```

---

## Task 3: MCP child process manager

**Files:**
- Create: `crates/rustoctopus-core/src/mcp/process.rs`

**Reference:** Port from `~/Projects/rustdrum/crates/rustdrum-core/src/mcp/process.rs`

**Step 1: Write the failing test**

In `crates/rustoctopus-core/src/mcp/process.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_and_kill() {
        // Use `cat` as a simple stdin/stdout echo process
        let mut proc = McpProcess::spawn("cat", &[], &HashMap::new())
            .await
            .expect("should spawn cat");
        assert!(proc.is_running());
        proc.kill();
        // Give it a moment to die
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!proc.is_running());
    }

    #[tokio::test]
    async fn test_send_request_to_echo() {
        // `cat` echoes stdin to stdout, so we can test framing
        let mut proc = McpProcess::spawn("cat", &[], &HashMap::new())
            .await
            .expect("should spawn cat");

        let req = JsonRpcRequest::new(1, "test/method", None);
        let line = serde_json::to_string(&req).unwrap();
        // Write manually and read back — cat echoes it
        proc.stdin.write_all(line.as_bytes()).await.unwrap();
        proc.stdin.write_all(b"\n").await.unwrap();
        proc.stdin.flush().await.unwrap();

        let mut buf = String::new();
        proc.stdout.read_line(&mut buf).await.unwrap();
        let resp: JsonRpcRequest = serde_json::from_str(&buf).unwrap();
        assert_eq!(resp.method, "test/method");
        proc.kill();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core mcp::process::tests`
Expected: FAIL — module doesn't exist

**Step 3: Write the implementation**

Create `crates/rustoctopus-core/src/mcp/process.rs`:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing::{debug, warn};

use super::client::*;

pub struct McpProcess {
    child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
    stderr_buf: Arc<Mutex<String>>,
    next_id: u64,
}

impl McpProcess {
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take();

        let stderr_buf = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = stderr {
            let buf = stderr_buf.clone();
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(stderr.into_owned_fd().unwrap());
                // Note: use try_into or platform-specific approach
                // Simplified: just read lines in a blocking thread
                for line in reader.lines().flatten() {
                    if let Ok(mut b) = buf.lock() {
                        b.push_str(&line);
                        b.push('\n');
                    }
                }
            });
        }

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr_buf,
            next_id: 1,
        })
    }

    pub fn stderr_output(&self) -> String {
        self.stderr_buf.lock().unwrap_or_default().clone()
    }

    pub async fn send_request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        let id = self.next_id;
        self.next_id += 1;
        let req = JsonRpcRequest::new(id, method, params);
        let line = serde_json::to_string(&req)?;
        debug!("MCP send: {}", line);
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let mut buf = String::new();
        self.stdout.read_line(&mut buf).await?;
        debug!("MCP recv: {}", buf.trim());
        let resp: JsonRpcResponse = serde_json::from_str(&buf)?;
        Ok(resp)
    }

    pub async fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<()> {
        let req = JsonRpcRequest::notification(method, params);
        let line = serde_json::to_string(&req)?;
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<Value> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "rustoctopus",
                "version": env!("CARGO_PKG_VERSION")
            }
        });
        let resp = self.send_request("initialize", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("MCP initialize error: {}", err.message));
        }
        self.send_notification("notifications/initialized", None).await?;
        Ok(resp.result.unwrap_or_default())
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDef>> {
        let resp = self.send_request("tools/list", None).await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("MCP tools/list error: {}", err.message));
        }
        let result = resp.result.unwrap_or_default();
        let tools: Vec<McpToolDef> =
            serde_json::from_value(result["tools"].clone()).unwrap_or_default();
        Ok(tools)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });
        let resp = self.send_request("tools/call", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("MCP tools/call error: {}", err.message));
        }
        let result: McpToolResult =
            serde_json::from_value(resp.result.unwrap_or_default())?;
        Ok(result)
    }

    pub fn kill(&mut self) {
        let _ = self.child.start_kill();
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        self.kill();
    }
}
```

Note: The stderr handling in RustDrum uses `into_owned_fd()` which may not be available. Adapt to use `tokio::spawn` with `tokio::io::BufReader` for stderr instead, or simply use a synchronous thread as RustDrum does. Look at RustDrum's `process.rs` for the exact working implementation.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core mcp::process::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rustoctopus-core/src/mcp/process.rs
git commit -m "feat(mcp): add MCP child process manager"
```

---

## Task 4: MCP server registry manager

**Files:**
- Create: `crates/rustoctopus-core/src/mcp/manager.rs`

**Reference:** Port from `~/Projects/rustdrum/crates/rustdrum-core/src/registry/manager.rs`

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager_is_empty() {
        let mgr = McpManager::new();
        assert_eq!(mgr.running_count(), 0);
        assert!(mgr.all_tool_defs().is_empty());
    }

    #[test]
    fn test_server_statuses_includes_not_running() {
        let mgr = McpManager::new();
        let mut servers = HashMap::new();
        servers.insert("test".to_string(), McpServerConfig {
            command: "echo".into(),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            auto_approve: vec![],
        });
        let statuses = mgr.server_statuses(&servers);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "test");
        assert!(!statuses[0].running);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core mcp::manager::tests`
Expected: FAIL

**Step 3: Write the implementation**

Create `crates/rustoctopus-core/src/mcp/manager.rs`:

```rust
use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::config::McpServerConfig;
use crate::config::{build_mcp_tool_name, parse_mcp_tool_name};
use super::client::McpToolDef;
use super::process::McpProcess;

pub struct McpServerInstance {
    pub name: String,
    pub config: McpServerConfig,
    pub process: McpProcess,
    pub tools: Vec<McpToolDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub name: String,
    pub enabled: bool,
    pub running: bool,
    pub tool_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct McpManager {
    servers: HashMap<String, McpServerInstance>,
    errors: HashMap<String, String>,
}

impl McpManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    pub async fn start_all(&mut self, configs: &HashMap<String, McpServerConfig>) {
        for (name, config) in configs {
            if config.enabled {
                if let Err(e) = self.start_server(name, config.clone()).await {
                    error!("Failed to start MCP server {}: {}", name, e);
                    self.errors.insert(name.clone(), e.to_string());
                }
            }
        }
    }

    pub async fn start_server(&mut self, name: &str, config: McpServerConfig) -> Result<()> {
        info!("Starting MCP server: {} ({})", name, config.command);
        let mut process = McpProcess::spawn(&config.command, &config.args, &config.env).await?;
        process.initialize().await?;
        let tools = process.list_tools().await?;
        info!("MCP server {} provides {} tools", name, tools.len());

        self.errors.remove(name);
        self.servers.insert(name.to_string(), McpServerInstance {
            name: name.to_string(),
            config,
            process,
            tools,
        });
        Ok(())
    }

    pub fn stop_server(&mut self, name: &str) -> bool {
        if let Some(mut server) = self.servers.remove(name) {
            server.process.kill();
            true
        } else {
            false
        }
    }

    pub fn stop_all(&mut self) {
        let names: Vec<String> = self.servers.keys().cloned().collect();
        for name in names {
            self.stop_server(&name);
        }
    }

    /// Returns tool definitions with namespaced names for ToolRegistry
    pub fn all_tool_defs(&self) -> Vec<(String, String, serde_json::Value)> {
        let mut defs = Vec::new();
        for (server_name, instance) in &self.servers {
            for tool in &instance.tools {
                let name = build_mcp_tool_name(server_name, &tool.name);
                let desc = tool.description.clone().unwrap_or_default();
                defs.push((name, desc, tool.input_schema.clone()));
            }
        }
        defs
    }

    /// Call a tool on a specific server
    pub async fn call_tool(
        &mut self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        let server = self.servers.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not found or not running", server_name))?;

        if !server.process.is_running() {
            self.errors.insert(server_name.to_string(), "Server crashed".into());
            return Err(anyhow::anyhow!("MCP server '{}' has crashed", server_name));
        }

        let result = server.process.call_tool(tool_name, arguments).await?;

        // Concatenate text content
        let text = result.content.iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_error {
            Err(anyhow::anyhow!("MCP tool error: {}", text))
        } else {
            Ok(text)
        }
    }

    pub fn server_statuses(&self, configs: &HashMap<String, McpServerConfig>) -> Vec<McpServerStatus> {
        configs.iter().map(|(name, config)| {
            let running = self.servers.get(name).map(|s| true).unwrap_or(false);
            let tool_count = self.servers.get(name).map(|s| s.tools.len()).unwrap_or(0);
            let error = self.errors.get(name).cloned();
            McpServerStatus {
                name: name.clone(),
                enabled: config.enabled,
                running,
                tool_count,
                error,
            }
        }).collect()
    }

    pub fn running_count(&self) -> usize {
        self.servers.len()
    }

    pub fn is_auto_approved(&self, server_name: &str, tool_name: &str) -> bool {
        self.servers.get(server_name)
            .map(|s| s.config.auto_approve.contains(&tool_name.to_string()))
            .unwrap_or(false)
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core mcp::manager::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rustoctopus-core/src/mcp/manager.rs
git commit -m "feat(mcp): add MCP server registry manager"
```

---

## Task 5: McpTool — bridge MCP tools into ToolRegistry

**Files:**
- Create: `crates/rustoctopus-core/src/tools/mcp_tool.rs`
- Modify: `crates/rustoctopus-core/src/tools/mod.rs` (add module export)

**Step 1: Write the failing test**

In `crates/rustoctopus-core/src/tools/mcp_tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_name_and_description() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpTool::new(
            "mcp_filesystem_read_file".into(),
            "Read a file".into(),
            serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}}),
            "filesystem".into(),
            "read_file".into(),
            manager,
            false, // requires approval
        );
        assert_eq!(tool.name(), "mcp_filesystem_read_file");
        assert_eq!(tool.description(), "Read a file");
    }

    #[test]
    fn test_mcp_tool_parameters() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpTool::new(
            "mcp_test_foo".into(),
            "Test".into(),
            serde_json::json!({"type": "object"}),
            "test".into(),
            "foo".into(),
            manager,
            true,
        );
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p rustoctopus-core tools::mcp_tool::tests`
Expected: FAIL

**Step 3: Write the implementation**

Create `crates/rustoctopus-core/src/tools/mcp_tool.rs`:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

use super::traits::{Tool, ToolError};
use crate::mcp::manager::McpManager;

pub struct McpTool {
    tool_name: String,        // namespaced: "mcp_filesystem_read_file"
    tool_description: String,
    tool_parameters: Value,
    server_name: String,      // "filesystem"
    original_name: String,    // "read_file"
    manager: Arc<Mutex<McpManager>>,
    auto_approved: bool,
}

impl McpTool {
    pub fn new(
        tool_name: String,
        tool_description: String,
        tool_parameters: Value,
        server_name: String,
        original_name: String,
        manager: Arc<Mutex<McpManager>>,
        auto_approved: bool,
    ) -> Self {
        Self {
            tool_name,
            tool_description,
            tool_parameters,
            server_name,
            original_name,
            manager,
            auto_approved,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters(&self) -> Value {
        self.tool_parameters.clone()
    }

    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        // TODO: approval check will be added in a later task
        // For now, execute directly
        let mut mgr = self.manager.lock().await;
        mgr.call_tool(&self.server_name, &self.original_name, params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}
```

Add to `crates/rustoctopus-core/src/tools/mod.rs`:

```rust
pub mod mcp_tool;
```

Note: Use `tokio::sync::Mutex` (not `std::sync::Mutex`) for the shared McpManager since we hold the lock across an await point in `call_tool()`.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p rustoctopus-core tools::mcp_tool::tests`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/rustoctopus-core/src/tools/mcp_tool.rs crates/rustoctopus-core/src/tools/mod.rs
git commit -m "feat(mcp): add McpTool bridging MCP tools into ToolRegistry"
```

---

## Task 6: Wire MCP into AgentLoop startup

**Files:**
- Modify: `crates/rustoctopus-core/src/agent/agent_loop.rs`
- Modify: `crates/rustoctopus-core/src/mcp/mod.rs` (add re-exports)

**Step 1: Write the failing test**

In `crates/rustoctopus-core/src/agent/agent_loop.rs` (existing test module):

```rust
#[test]
fn test_mcp_disabled_no_tools() {
    // Config with MCP disabled should not register any MCP tools
    let config = Config::default(); // mcp.enabled = false by default
    // Verify no mcp_ tools would be registered
    assert!(!config.mcp.enabled);
    assert!(config.mcp.servers.is_empty());
}
```

**Step 2: Implement the MCP initialization in AgentLoop**

In `AgentLoop::from_config()` or `AgentLoop::new()`, after registering built-in tools, add:

```rust
// After existing tool registrations...

// MCP tools (if enabled)
let mcp_manager = Arc::new(tokio::sync::Mutex::new(McpManager::new()));
if config.mcp.enabled {
    let mut mgr = mcp_manager.blocking_lock();
    // start_all is async, so we need to block or use a different approach
    // Option: store mcp_manager in AgentLoop, start servers in run()
}
```

The actual approach: Store `Arc<tokio::sync::Mutex<McpManager>>` in `AgentLoop`. In the `run()` method (which is async), start all MCP servers and register their tools into the ToolRegistry before entering the main loop.

Add to `AgentLoop` struct:
```rust
pub(crate) mcp_manager: Option<Arc<tokio::sync::Mutex<McpManager>>>,
```

Add an async method to register MCP tools:
```rust
async fn register_mcp_tools(&mut self) {
    if let Some(ref mcp_mgr) = self.mcp_manager {
        let mut mgr = mcp_mgr.lock().await;
        // Start all servers from config
        mgr.start_all(&self.config_snapshot.mcp.servers).await;

        // Register tools
        for (name, desc, params) in mgr.all_tool_defs() {
            let (server, tool) = parse_mcp_tool_name(&name).unwrap();
            let auto = mgr.is_auto_approved(server, tool);
            self.tools.register(Box::new(McpTool::new(
                name, desc, params,
                server.to_string(), tool.to_string(),
                mcp_mgr.clone(), auto,
            )));
        }
    }
}
```

Call `self.register_mcp_tools().await` at the beginning of `run()`.

**Step 3: Run full test suite**

Run: `cargo test -p rustoctopus-core`
Expected: All existing tests PASS + new test PASS

**Step 4: Commit**

```bash
git add crates/rustoctopus-core/src/agent/agent_loop.rs crates/rustoctopus-core/src/mcp/mod.rs
git commit -m "feat(mcp): wire MCP manager into AgentLoop startup"
```

---

## Task 7: Tauri IPC commands for MCP management

**Files:**
- Create: `crates/rustoctopus-app/src/commands/mcp.rs`
- Modify: `crates/rustoctopus-app/src/commands/mod.rs`
- Modify: `crates/rustoctopus-app/src/state.rs` (add McpManager to AppState)
- Modify: `crates/rustoctopus-app/src/main.rs` (register new commands)

**Reference:** Port from `~/Projects/rustdrum/crates/rustdrum-app/src/commands/servers.rs`

**Step 1: Add McpManager to AppState**

In `crates/rustoctopus-app/src/state.rs`, add:

```rust
use rustoctopus_core::mcp::manager::McpManager;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

pub struct AppState {
    pub config: Mutex<Config>,
    pub cron: Mutex<CronService>,
    pub channel_names: Mutex<Vec<String>>,
    pub started_at: Instant,
    pub mcp_manager: Arc<TokioMutex<McpManager>>,  // NEW
}
```

In `boot()`, initialize the MCP manager:
```rust
let mcp_manager = Arc::new(TokioMutex::new(McpManager::new()));
if config.mcp.enabled {
    let mut mgr = mcp_manager.lock().await;
    mgr.start_all(&config.mcp.servers).await;
}
```

**Step 2: Create MCP commands**

Create `crates/rustoctopus-app/src/commands/mcp.rs`:

```rust
use rustoctopus_core::config::{save_config, McpServerConfig};
use rustoctopus_core::mcp::manager::McpServerStatus;
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServerStatus>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let mgr = state.mcp_manager.lock().await;
    Ok(mgr.server_statuses(&config.mcp.servers))
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, AppState>,
    name: String,
    command: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let server_config = McpServerConfig {
        command,
        args,
        env,
        enabled: true,
        auto_approve: vec![],
    };

    {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        config.mcp.servers.insert(name.clone(), server_config.clone());
        save_config(None, &config).map_err(|e| e.to_string())?;
    }

    let mut mgr = state.mcp_manager.lock().await;
    mgr.start_server(&name, server_config).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        config.mcp.servers.remove(&name);
        save_config(None, &config).map_err(|e| e.to_string())?;
    }
    let mut mgr = state.mcp_manager.lock().await;
    mgr.stop_server(&name);
    Ok(())
}

#[tauri::command]
pub async fn toggle_mcp_server(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().map_err(|e| e.to_string())?;
        if let Some(server) = config.mcp.servers.get_mut(&name) {
            server.enabled = enabled;
            save_config(None, &config).map_err(|e| e.to_string())?;
        }
    }
    let mut mgr = state.mcp_manager.lock().await;
    if enabled {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        if let Some(server_config) = config.mcp.servers.get(&name) {
            mgr.start_server(&name, server_config.clone()).await.map_err(|e| e.to_string())?;
        }
    } else {
        mgr.stop_server(&name);
    }
    Ok(())
}
```

**Step 3: Register in mod.rs and main.rs**

Add to `crates/rustoctopus-app/src/commands/mod.rs`:
```rust
pub mod mcp;
```

Add to `main.rs` invoke handler:
```rust
commands::mcp::list_mcp_servers,
commands::mcp::add_mcp_server,
commands::mcp::remove_mcp_server,
commands::mcp::toggle_mcp_server,
```

**Step 4: Build to verify compilation**

Run: `cargo build -p rustoctopus-app`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add crates/rustoctopus-app/src/commands/mcp.rs crates/rustoctopus-app/src/commands/mod.rs crates/rustoctopus-app/src/state.rs crates/rustoctopus-app/src/main.rs
git commit -m "feat(mcp): add Tauri IPC commands for MCP server management"
```

---

## Task 8: Frontend — MCP management page

**Files:**
- Create: `crates/rustoctopus-app/ui/src/views/Mcp.tsx`
- Modify: `crates/rustoctopus-app/ui/src/lib/invoke.ts` (add MCP API bindings)
- Modify: `crates/rustoctopus-app/ui/src/App.tsx` (add MCP route)

**Reference:** Port from `~/Projects/rustdrum/crates/rustdrum-app/ui/src/views/Servers.tsx`

**Step 1: Add API bindings**

In `crates/rustoctopus-app/ui/src/lib/invoke.ts`, add types and API calls:

```typescript
export interface McpServerStatus {
  name: string;
  enabled: boolean;
  running: boolean;
  tool_count: number;
  error?: string;
}

// Add to api object:
listMcpServers: () => invoke<McpServerStatus[]>("list_mcp_servers"),
addMcpServer: (name: string, command: string, args: string[], env: Record<string, string>) =>
  invoke<void>("add_mcp_server", { name, command, args, env }),
removeMcpServer: (name: string) => invoke<void>("remove_mcp_server", { name }),
toggleMcpServer: (name: string, enabled: boolean) =>
  invoke<void>("toggle_mcp_server", { name, enabled }),
```

**Step 2: Create the MCP view**

Create `crates/rustoctopus-app/ui/src/views/Mcp.tsx`:

Port from RustDrum's `Servers.tsx` with these features:
- Server card grid showing name, status (Running/Stopped/Error), tool count
- Enable/disable toggle per server
- "Add Server" button opening a dialog with command/args/env inputs
- "Remove" button with confirmation
- Auto-refresh every 2 seconds

Simplify compared to RustDrum — remove the official registry search for now (can add later). Keep the manual add form only.

**Step 3: Add route**

In `App.tsx`, add to navItems:
```typescript
{ to: "/mcp", label: "MCP" },
```

Add route:
```tsx
<Route path="/mcp" element={<Mcp />} />
```

**Step 4: Test manually**

Run: `cd crates/rustoctopus-app && cargo tauri dev`
Expected: MCP page shows in sidebar, displays empty server list, Add button works

**Step 5: Commit**

```bash
git add crates/rustoctopus-app/ui/src/views/Mcp.tsx crates/rustoctopus-app/ui/src/lib/invoke.ts crates/rustoctopus-app/ui/src/App.tsx
git commit -m "feat(mcp): add MCP server management page to Tauri GUI"
```

---

## Task 9: Integration test — full MCP flow

**Files:**
- Modify: `crates/rustoctopus-core/src/mcp/manager.rs` (add integration test)

**Step 1: Write integration test**

This test requires an actual MCP server. Use the `@modelcontextprotocol/server-filesystem` if Node.js is available, or create a minimal mock MCP server script.

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test with a simple echo-based mock MCP server
    /// Create a small script that responds to JSON-RPC
    #[tokio::test]
    async fn test_start_and_list_tools_mock() {
        // Only run if `node` is available
        if std::process::Command::new("node").arg("--version").output().is_err() {
            eprintln!("Skipping: node not found");
            return;
        }

        // Create a minimal MCP server mock using node
        let script = r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => {
    const req = JSON.parse(line);
    if (req.method === 'initialize') {
        console.log(JSON.stringify({jsonrpc:'2.0',id:req.id,result:{capabilities:{}}}));
    } else if (req.method === 'tools/list') {
        console.log(JSON.stringify({jsonrpc:'2.0',id:req.id,result:{tools:[{name:'echo',description:'Echo input',inputSchema:{type:'object',properties:{text:{type:'string'}}}}]}}));
    } else if (req.method === 'tools/call') {
        const text = req.params.arguments.text || '';
        console.log(JSON.stringify({jsonrpc:'2.0',id:req.id,result:{content:[{type:'text',text:text}],isError:false}}));
    }
});
"#;
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), script).unwrap();

        let mut mgr = McpManager::new();
        let config = McpServerConfig {
            command: "node".into(),
            args: vec![tmp.path().to_string_lossy().to_string()],
            env: HashMap::new(),
            enabled: true,
            auto_approve: vec!["echo".to_string()],
        };

        mgr.start_server("mock", config).await.unwrap();
        assert_eq!(mgr.running_count(), 1);

        let defs = mgr.all_tool_defs();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].0, "mcp_mock_echo");

        // Call the tool
        let result = mgr.call_tool("mock", "echo", serde_json::json!({"text": "hello"})).await.unwrap();
        assert_eq!(result, "hello");

        assert!(mgr.is_auto_approved("mock", "echo"));
        assert!(!mgr.is_auto_approved("mock", "write_file"));

        mgr.stop_all();
        assert_eq!(mgr.running_count(), 0);
    }
}
```

**Step 2: Run integration test**

Run: `cargo test -p rustoctopus-core mcp::manager::integration_tests -- --nocapture`
Expected: PASS (if Node.js is installed)

**Step 3: Run full test suite**

Run: `cargo test -p rustoctopus-core`
Expected: All tests PASS (229+ existing + new MCP tests)

**Step 4: Commit**

```bash
git add crates/rustoctopus-core/src/mcp/manager.rs
git commit -m "test(mcp): add integration test for full MCP flow"
```

---

## Task 10: Update design docs and clean up

**Files:**
- Modify: `docs/plans/2026-02-24-mcp-integration-design.md` (update to reflect in-process architecture)
- Remove reference to RustDrum as separate project

**Step 1: Update the design doc**

Rewrite `docs/plans/2026-02-24-mcp-integration-design.md` to reflect:
- MCP is now integrated directly into rustoctopus-core
- No WebSocket needed — MCP servers are child processes managed by McpManager
- Tauri GUI has an MCP page for server management
- Approval system design (for future implementation)

**Step 2: Run full build and test**

Run: `cargo build --release -p rustoctopus-cli && cargo test`
Expected: Clean build and all tests pass

**Step 3: Commit**

```bash
git add docs/plans/2026-02-24-mcp-integration-design.md
git commit -m "docs: update MCP design doc to reflect in-process integration"
```
