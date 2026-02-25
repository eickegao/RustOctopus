use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub name: String,
    pub enabled: bool,
}

#[tauri::command]
pub async fn get_channel_status(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ChannelInfo>, String> {
    let active_names = state.channel_names.lock().await;

    let channels = vec![
        ChannelInfo {
            name: "telegram".to_string(),
            enabled: active_names.contains(&"telegram".to_string()),
        },
        ChannelInfo {
            name: "feishu".to_string(),
            enabled: active_names.contains(&"feishu".to_string()),
        },
        ChannelInfo {
            name: "whatsapp".to_string(),
            enabled: active_names.contains(&"whatsapp".to_string()),
        },
    ];

    Ok(channels)
}
