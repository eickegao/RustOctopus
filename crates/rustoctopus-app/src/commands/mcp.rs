use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;

use rustoctopus_core::config::loader::save_config;
use rustoctopus_core::config::schema::McpServerConfig;
use rustoctopus_core::mcp::manager::McpServerStatus;

use crate::state::AppState;

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
