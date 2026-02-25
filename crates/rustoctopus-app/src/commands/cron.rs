use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use rustoctopus_core::cron::{CronJob, CronSchedule};

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddJobRequest {
    pub name: String,
    pub message: String,
    pub schedule_kind: String,
    pub every_ms: Option<i64>,
    pub cron_expr: Option<String>,
}

#[tauri::command]
pub async fn list_cron_jobs(state: State<'_, Arc<AppState>>) -> Result<Vec<CronJob>, String> {
    let cron = state.cron.lock().await;
    Ok(cron.list_jobs(true))
}

#[tauri::command]
pub async fn add_cron_job(
    state: State<'_, Arc<AppState>>,
    req: AddJobRequest,
) -> Result<CronJob, String> {
    let schedule = match req.schedule_kind.as_str() {
        "every" => {
            let ms = req.every_ms.ok_or("every_ms required for 'every' schedule")?;
            CronSchedule::every(ms)
        }
        "cron" => {
            let expr = req
                .cron_expr
                .as_deref()
                .ok_or("cron_expr required for 'cron' schedule")?;
            CronSchedule::cron_expr(expr, None)
        }
        other => return Err(format!("unknown schedule kind: {}", other)),
    };

    let mut cron = state.cron.lock().await;
    cron.add_job(&req.name, schedule, &req.message, true, None, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_cron_job(
    state: State<'_, Arc<AppState>>,
    job_id: String,
) -> Result<bool, String> {
    let mut cron = state.cron.lock().await;
    Ok(cron.remove_job(&job_id))
}

#[tauri::command]
pub async fn toggle_cron_job(
    state: State<'_, Arc<AppState>>,
    job_id: String,
) -> Result<bool, String> {
    let mut cron = state.cron.lock().await;
    let jobs = cron.list_jobs(true);
    let job = jobs.iter().find(|j| j.id == job_id);
    match job {
        Some(j) => Ok(cron.enable_job(&job_id, !j.enabled)),
        None => Err(format!("job not found: {}", job_id)),
    }
}
