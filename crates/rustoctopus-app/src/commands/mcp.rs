use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use rustoctopus_core::config::loader::save_config;
use rustoctopus_core::config::schema::McpServerConfig;
use rustoctopus_core::mcp::manager::McpServerStatus;

use crate::state::AppState;

// ── MCP Registry types ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct RegistryEnvVar {
    pub name: String,
    pub description: Option<String>,
    pub is_required: bool,
}

#[derive(Serialize)]
pub struct RegistryPackage {
    pub registry_type: Option<String>,
    pub identifier: Option<String>,
    pub environment_variables: Vec<RegistryEnvVar>,
}

#[derive(Serialize)]
pub struct RegistryServer {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub repository_url: Option<String>,
    pub packages: Vec<RegistryPackage>,
}

#[derive(Serialize)]
pub struct RegistrySearchResult {
    pub servers: Vec<RegistryServer>,
    pub next_cursor: Option<String>,
}

// Raw JSON shapes returned by the registry API
#[derive(Deserialize)]
struct RawRegistryResponse {
    servers: Vec<RawServerWrapper>,
    metadata: Option<RawMetadata>,
}

#[derive(Deserialize)]
struct RawServerWrapper {
    server: RawServer,
}

#[derive(Deserialize)]
struct RawServer {
    name: String,
    description: Option<String>,
    version: Option<String>,
    repository: Option<RawRepository>,
    #[serde(default)]
    packages: Vec<RawPackage>,
}

#[derive(Deserialize)]
struct RawRepository {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPackage {
    registry_type: Option<String>,
    identifier: Option<String>,
    #[serde(default)]
    environment_variables: Vec<RawEnvVar>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEnvVar {
    name: String,
    description: Option<String>,
    #[serde(default)]
    is_required: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMetadata {
    next_cursor: Option<String>,
}

#[tauri::command]
pub async fn search_mcp_registry(
    query: String,
    limit: Option<u32>,
) -> Result<RegistrySearchResult, String> {
    let limit = limit.unwrap_or(20).min(100);
    let url = format!(
        "https://registry.modelcontextprotocol.io/v0/servers?search={}&limit={}&version=latest",
        urlencoding(query.as_str()),
        limit,
    );

    let resp: RawRegistryResponse = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let servers = resp
        .servers
        .into_iter()
        .map(|w| {
            let s = w.server;
            RegistryServer {
                name: s.name,
                description: s.description,
                version: s.version,
                repository_url: s.repository.and_then(|r| r.url),
                packages: s
                    .packages
                    .into_iter()
                    .map(|p| RegistryPackage {
                        registry_type: p.registry_type,
                        identifier: p.identifier,
                        environment_variables: p
                            .environment_variables
                            .into_iter()
                            .map(|ev| RegistryEnvVar {
                                name: ev.name,
                                description: ev.description,
                                is_required: ev.is_required,
                            })
                            .collect(),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(RegistrySearchResult {
        servers,
        next_cursor: resp.metadata.and_then(|m| m.next_cursor),
    })
}

/// Minimal percent-encoding for query strings.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<McpServerStatus>, String> {
    let config = state.config.lock().await;
    let mgr = state.mcp_manager.lock().await;
    Ok(mgr.server_statuses(&config.mcp.servers))
}

#[tauri::command]
pub async fn add_mcp_server(
    state: State<'_, Arc<AppState>>,
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
) -> Result<(), String> {
    let server_config = McpServerConfig {
        command,
        args,
        env,
        enabled: true,
        auto_approve: vec![],
    };

    {
        let mut config = state.config.lock().await;
        config.mcp.enabled = true;
        config.mcp.servers.insert(name.clone(), server_config.clone());
        save_config(&config, None).map_err(|e| e.to_string())?;
    }

    let mut mgr = state.mcp_manager.lock().await;
    mgr.start_server(&name, server_config)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_mcp_server(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().await;
        config.mcp.servers.remove(&name);
        save_config(&config, None).map_err(|e| e.to_string())?;
    }

    let mut mgr = state.mcp_manager.lock().await;
    mgr.stop_server(&name);
    Ok(())
}

#[tauri::command]
pub async fn toggle_mcp_server(
    state: State<'_, Arc<AppState>>,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    let server_config = {
        let mut config = state.config.lock().await;
        if let Some(server) = config.mcp.servers.get_mut(&name) {
            server.enabled = enabled;
            save_config(&config, None).map_err(|e| e.to_string())?;
            if enabled {
                Some(config.mcp.servers[&name].clone())
            } else {
                None
            }
        } else {
            return Err(format!("Server '{}' not found", name));
        }
    };

    let mut mgr = state.mcp_manager.lock().await;
    if enabled {
        if let Some(sc) = server_config {
            mgr.start_server(&name, sc)
                .await
                .map_err(|e| e.to_string())?;
        }
    } else {
        mgr.stop_server(&name);
    }
    Ok(())
}
