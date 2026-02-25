use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub model: String,
    pub uptime_secs: u64,
    pub channels: Vec<String>,
    pub cron_job_count: usize,
    pub cron_enabled_count: usize,
    pub cron_next_fire_ms: Option<i64>,
}

#[tauri::command]
pub async fn get_status(state: State<'_, Arc<AppState>>) -> Result<StatusInfo, String> {
    let config = state.config.lock().await;
    let names = state.channel_names.lock().await;
    let cron = state.cron.lock().await;
    let cron_status = cron.status();

    Ok(StatusInfo {
        model: config.agents.defaults.model.clone(),
        uptime_secs: state.uptime_secs(),
        channels: names.clone(),
        cron_job_count: cron_status.job_count,
        cron_enabled_count: cron_status.enabled_count,
        cron_next_fire_ms: cron_status.next_fire_at_ms,
    })
}
