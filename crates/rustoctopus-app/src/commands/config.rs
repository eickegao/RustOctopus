use std::sync::Arc;

use tauri::State;

use rustoctopus_core::config::loader::save_config;
use rustoctopus_core::config::schema::Config;

use crate::state::AppState;

#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<Config, String> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config_cmd(
    state: State<'_, Arc<AppState>>,
    config: Config,
) -> Result<(), String> {
    save_config(&config, None).map_err(|e| e.to_string())?;
    let mut current = state.config.lock().await;
    *current = config;
    Ok(())
}
