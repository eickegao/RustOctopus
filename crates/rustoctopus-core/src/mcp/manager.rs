use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;

use crate::config::{build_mcp_tool_name, McpServerConfig};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A running MCP server instance managed by the McpManager.
pub struct McpServerInstance {
    pub name: String,
    pub config: McpServerConfig,
    pub service: RunningService<RoleClient, ()>,
    pub tools: Vec<McpToolInfo>,
}

/// Metadata about a single tool exposed by an MCP server.
#[derive(Debug, Clone)]
pub struct McpToolInfo {
    /// Original tool name as reported by the MCP server (e.g. "read_file").
    pub name: String,
    /// Human-readable description of the tool.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: serde_json::Value,
}

/// Snapshot of a server's status for reporting to the UI / API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub name: String,
    pub enabled: bool,
    pub running: bool,
    pub tool_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// McpManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of multiple MCP server child-processes.
///
/// Each server is spawned via `TokioChildProcess` and communicates over stdio
/// using the Model Context Protocol. On startup the manager connects to every
/// enabled server, discovers its tools, and makes them available for
/// tool-calling via the agent loop.
pub struct McpManager {
    servers: HashMap<String, McpServerInstance>,
    errors: HashMap<String, String>,
}

impl McpManager {
    /// Create a new, empty manager.
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            errors: HashMap::new(),
        }
    }

    // -- lifecycle ----------------------------------------------------------

    /// Start all *enabled* servers from the provided config map.
    pub async fn start_all(&mut self, configs: &HashMap<String, McpServerConfig>) {
        for (name, config) in configs {
            if !config.enabled {
                info!(server = %name, "MCP server is disabled, skipping");
                continue;
            }
            if let Err(e) = self.start_server(name, config.clone()).await {
                error!(server = %name, err = %e, "Failed to start MCP server");
                self.errors.insert(name.clone(), e.to_string());
            }
        }
    }

    /// Spawn a single MCP server, perform the handshake, and discover tools.
    pub async fn start_server(&mut self, name: &str, config: McpServerConfig) -> Result<()> {
        // Remove any previous error for this server.
        self.errors.remove(name);

        // Build the tokio Command.
        let mut cmd = tokio::process::Command::new(&config.command);
        cmd.args(&config.args);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        // Spawn the child-process transport and initialise the MCP session.
        let transport = TokioChildProcess::new(cmd)
            .context("Failed to spawn MCP server process")?;
        let service = ().serve(transport)
            .await
            .context("MCP handshake / initialisation failed")?;

        // Discover tools.
        let tools_result = service
            .list_all_tools()
            .await
            .context("Failed to list tools from MCP server")?;

        let tools: Vec<McpToolInfo> = tools_result
            .into_iter()
            .map(|t| McpToolInfo {
                name: t.name.to_string(),
                description: t
                    .description
                    .as_deref()
                    .unwrap_or("")
                    .to_string(),
                input_schema: serde_json::to_value(&*t.input_schema)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
            })
            .collect();

        info!(
            server = %name,
            tool_count = tools.len(),
            "MCP server started"
        );

        self.servers.insert(
            name.to_string(),
            McpServerInstance {
                name: name.to_string(),
                config,
                service,
                tools,
            },
        );

        Ok(())
    }

    /// Stop (cancel) a running server.  Returns `true` if it was running.
    pub fn stop_server(&mut self, name: &str) -> bool {
        if let Some(instance) = self.servers.remove(name) {
            // RunningService is dropped here which will terminate the child.
            info!(server = %name, "MCP server stopped");
            drop(instance);
            true
        } else {
            false
        }
    }

    /// Stop all running servers.
    pub fn stop_all(&mut self) {
        let names: Vec<String> = self.servers.keys().cloned().collect();
        for name in names {
            self.stop_server(&name);
        }
    }

    // -- tool discovery ------------------------------------------------------

    /// Return namespaced tool definitions for *all* running servers.
    ///
    /// Each tuple is `(namespaced_name, description, input_schema)` where the
    /// namespaced name follows the pattern `mcp_<server>_<tool>`.
    pub fn all_tool_defs(&self) -> Vec<(String, String, serde_json::Value)> {
        let mut defs = Vec::new();
        for (server_name, instance) in &self.servers {
            for tool in &instance.tools {
                defs.push((
                    build_mcp_tool_name(server_name, &tool.name),
                    tool.description.clone(),
                    tool.input_schema.clone(),
                ));
            }
        }
        defs
    }

    // -- tool invocation ----------------------------------------------------

    /// Call a tool on a specific running server.
    ///
    /// `server_name` is the key in the config map (e.g. "filesystem").
    /// `tool_name` is the *original* (un-namespaced) MCP tool name.
    /// `arguments` is a JSON object matching the tool's input schema.
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<String> {
        let instance = self
            .servers
            .get(server_name)
            .with_context(|| format!("MCP server '{server_name}' is not running"))?;

        let args_obj = arguments.as_object().cloned();

        let result = instance
            .service
            .call_tool(CallToolRequestParams {
                name: tool_name.to_string().into(),
                arguments: args_obj,
                meta: None,
                task: None,
            })
            .await
            .with_context(|| {
                format!("call_tool '{tool_name}' on MCP server '{server_name}' failed")
            })?;

        // Check for MCP-level errors.
        if result.is_error == Some(true) {
            let text = result
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.as_str())
                .collect::<Vec<&str>>()
                .join("\n");
            anyhow::bail!("MCP tool error: {text}");
        }

        // Concatenate text content.
        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text())
            .map(|t| t.text.as_ref())
            .collect::<Vec<&str>>()
            .join("\n");

        Ok(text)
    }

    // -- status & query -----------------------------------------------------

    /// Build status objects for every *configured* server (running or not).
    pub fn server_statuses(
        &self,
        configs: &HashMap<String, McpServerConfig>,
    ) -> Vec<McpServerStatus> {
        let mut statuses = Vec::new();

        for (name, config) in configs {
            let running_instance = self.servers.get(name);
            statuses.push(McpServerStatus {
                name: name.clone(),
                enabled: config.enabled,
                running: running_instance.is_some(),
                tool_count: running_instance
                    .map(|i| i.tools.len())
                    .unwrap_or(0),
                error: self.errors.get(name).cloned(),
            });
        }

        statuses
    }

    /// Check whether a tool on a given server is in the auto-approve list.
    pub fn is_auto_approved(&self, server_name: &str, tool_name: &str) -> bool {
        self.servers
            .get(server_name)
            .map(|i| i.config.auto_approve.contains(&tool_name.to_string()))
            .unwrap_or(false)
    }

    /// Number of servers that are currently running.
    pub fn running_count(&self) -> usize {
        self.servers.len()
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        servers.insert(
            "test".to_string(),
            McpServerConfig {
                command: "echo".into(),
                ..Default::default()
            },
        );
        let statuses = mgr.server_statuses(&servers);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "test");
        assert!(!statuses[0].running);
    }

    #[test]
    fn test_is_auto_approved_empty() {
        let mgr = McpManager::new();
        assert!(!mgr.is_auto_approved("test", "tool"));
    }

    #[test]
    fn test_stop_server_not_running() {
        let mut mgr = McpManager::new();
        assert!(!mgr.stop_server("nonexistent"));
    }

    #[test]
    fn test_stop_all_empty() {
        let mut mgr = McpManager::new();
        mgr.stop_all(); // should not panic
        assert_eq!(mgr.running_count(), 0);
    }

    #[test]
    fn test_default_impl() {
        let mgr = McpManager::default();
        assert_eq!(mgr.running_count(), 0);
    }

    #[test]
    fn test_server_status_serialization() {
        let status = McpServerStatus {
            name: "test-server".into(),
            enabled: true,
            running: false,
            tool_count: 0,
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"name\":\"test-server\""));
        // error field should be absent (skip_serializing_if = None)
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_server_status_with_error() {
        let status = McpServerStatus {
            name: "broken".into(),
            enabled: true,
            running: false,
            tool_count: 0,
            error: Some("connection refused".into()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("connection refused"));
    }

    #[test]
    fn test_statuses_multiple_servers() {
        let mgr = McpManager::new();
        let mut servers = HashMap::new();
        servers.insert(
            "a".to_string(),
            McpServerConfig {
                command: "cmd_a".into(),
                enabled: true,
                ..Default::default()
            },
        );
        servers.insert(
            "b".to_string(),
            McpServerConfig {
                command: "cmd_b".into(),
                enabled: false,
                ..Default::default()
            },
        );
        let statuses = mgr.server_statuses(&servers);
        assert_eq!(statuses.len(), 2);

        let a = statuses.iter().find(|s| s.name == "a").unwrap();
        assert!(a.enabled);
        assert!(!a.running);

        let b = statuses.iter().find(|s| s.name == "b").unwrap();
        assert!(!b.enabled);
        assert!(!b.running);
    }
}
