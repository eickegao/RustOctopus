use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use cron::Schedule;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::types::*;

/// Returns current time as milliseconds since epoch.
fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Callback invoked when a cron job fires.
/// Receives the CronJob and returns an optional error message on failure.
pub type OnJobCallback = Arc<
    dyn Fn(CronJob) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<String>>> + Send>>
        + Send
        + Sync,
>;

/// Service status summary.
#[derive(Debug, Clone)]
pub struct CronServiceStatus {
    pub running: bool,
    pub job_count: usize,
    pub enabled_count: usize,
    pub next_fire_at_ms: Option<i64>,
}

/// Parameters for adding a new cron job.
pub struct AddJobParams<'a> {
    pub name: &'a str,
    pub schedule: CronSchedule,
    pub message: &'a str,
    pub deliver: bool,
    pub channel: Option<&'a str>,
    pub to: Option<&'a str>,
    pub delete_after_run: bool,
}

/// Manages scheduled cron jobs with JSON file persistence and async timer scheduling.
pub struct CronService {
    store_path: PathBuf,
    store: CronStore,
    on_job: Option<OnJobCallback>,
    timer_handle: Option<JoinHandle<()>>,
    /// Shared reference for the timer task to call back into the service.
    inner: Option<Arc<Mutex<CronServiceInner>>>,
}

/// Shared interior state used by the timer task.
struct CronServiceInner {
    store_path: PathBuf,
    store: CronStore,
    on_job: Option<OnJobCallback>,
}

impl CronService {
    /// Create a new CronService that persists jobs to the given path.
    pub fn new(store_path: PathBuf) -> Self {
        Self {
            store_path,
            store: CronStore::default(),
            on_job: None,
            timer_handle: None,
            inner: None,
        }
    }

    /// Set the callback invoked when a job fires.
    pub fn set_on_job(&mut self, callback: OnJobCallback) {
        self.on_job = Some(callback);
    }

    /// Load the store from disk, recompute next runs, and arm the timer.
    pub fn start(&mut self) -> anyhow::Result<()> {
        self.load_store()?;
        let now = now_ms();
        for job in &mut self.store.jobs {
            if job.enabled {
                job.state.next_run_at_ms = compute_next_run(&job.schedule, now);
            }
        }
        self.save_store()?;
        self.arm_timer();
        info!("CronService started with {} jobs", self.store.jobs.len());
        Ok(())
    }

    /// Stop the timer.
    pub fn stop(&mut self) {
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
            debug!("CronService timer stopped");
        }
        self.inner = None;
    }

    /// List jobs, optionally including disabled ones. Sorted by next_run_at_ms.
    pub fn list_jobs(&self, include_disabled: bool) -> Vec<CronJob> {
        let mut jobs: Vec<CronJob> = self
            .store
            .jobs
            .iter()
            .filter(|j| include_disabled || j.enabled)
            .cloned()
            .collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    /// Add a new job. Returns the created CronJob.
    pub fn add_job(
        &mut self,
        name: &str,
        schedule: CronSchedule,
        message: &str,
        deliver: bool,
        channel: Option<&str>,
        to: Option<&str>,
    ) -> anyhow::Result<CronJob> {
        self.add_job_ext(AddJobParams {
            name,
            schedule,
            message,
            deliver,
            channel,
            to,
            delete_after_run: false,
        })
    }

    /// Add a new job with all options including delete_after_run. Returns the created CronJob.
    pub fn add_job_ext(&mut self, params: AddJobParams<'_>) -> anyhow::Result<CronJob> {
        let now = now_ms();
        let next_run = compute_next_run(&params.schedule, now);

        let job = CronJob {
            id: uuid::Uuid::new_v4().to_string(),
            name: params.name.to_string(),
            enabled: true,
            schedule: params.schedule,
            payload: CronPayload {
                kind: PayloadKind::AgentTurn,
                message: params.message.to_string(),
                deliver: params.deliver,
                channel: params.channel.map(|s| s.to_string()),
                to: params.to.map(|s| s.to_string()),
            },
            state: CronJobState {
                next_run_at_ms: next_run,
                ..Default::default()
            },
            created_at_ms: now,
            updated_at_ms: now,
            delete_after_run: params.delete_after_run,
        };

        self.store.jobs.push(job.clone());
        self.save_store()?;
        self.arm_timer();
        info!("Added cron job '{}' (id={})", params.name, job.id);
        Ok(job)
    }

    /// Remove a job by ID. Returns true if found and removed.
    pub fn remove_job(&mut self, job_id: &str) -> bool {
        let before = self.store.jobs.len();
        self.store.jobs.retain(|j| j.id != job_id);
        let removed = self.store.jobs.len() < before;
        if removed {
            let _ = self.save_store();
            self.arm_timer();
            info!("Removed cron job id={}", job_id);
        }
        removed
    }

    /// Enable or disable a job by ID. Returns true if the job was found.
    pub fn enable_job(&mut self, job_id: &str, enabled: bool) -> bool {
        if let Some(job) = self.store.jobs.iter_mut().find(|j| j.id == job_id) {
            job.enabled = enabled;
            job.updated_at_ms = now_ms();
            if enabled {
                job.state.next_run_at_ms = compute_next_run(&job.schedule, now_ms());
            } else {
                job.state.next_run_at_ms = None;
            }
            let _ = self.save_store();
            self.arm_timer();
            info!(
                "Cron job id={} {}",
                job_id,
                if enabled { "enabled" } else { "disabled" }
            );
            true
        } else {
            false
        }
    }

    /// Returns the current service status.
    pub fn status(&self) -> CronServiceStatus {
        let enabled_count = self.store.jobs.iter().filter(|j| j.enabled).count();
        let next_fire = self
            .store
            .jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.state.next_run_at_ms)
            .min();

        CronServiceStatus {
            running: self.timer_handle.is_some(),
            job_count: self.store.jobs.len(),
            enabled_count,
            next_fire_at_ms: next_fire,
        }
    }

    // --- internal ---

    fn load_store(&mut self) -> anyhow::Result<()> {
        if self.store_path.exists() {
            let data = std::fs::read_to_string(&self.store_path)?;
            self.store = serde_json::from_str(&data)?;
            debug!("Loaded cron store from {:?}", self.store_path);
        } else {
            self.store = CronStore::default();
            debug!("No cron store found, starting fresh");
        }
        Ok(())
    }

    fn save_store(&self) -> anyhow::Result<()> {
        if let Some(parent) = self.store_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.store)?;
        std::fs::write(&self.store_path, json)?;
        debug!("Saved cron store to {:?}", self.store_path);
        Ok(())
    }

    fn arm_timer(&mut self) {
        // Cancel previous timer
        if let Some(handle) = self.timer_handle.take() {
            handle.abort();
        }

        // Find the earliest next_run_at_ms among enabled jobs
        let next_fire = self
            .store
            .jobs
            .iter()
            .filter(|j| j.enabled)
            .filter_map(|j| j.state.next_run_at_ms)
            .min();

        let Some(next_ms) = next_fire else {
            debug!("No jobs to schedule, timer not armed");
            return;
        };

        let delay_ms = (next_ms - now_ms()).max(0) as u64;

        // Build shared inner state
        let inner = Arc::new(Mutex::new(CronServiceInner {
            store_path: self.store_path.clone(),
            store: self.store.clone(),
            on_job: self.on_job.clone(),
        }));
        self.inner = Some(inner.clone());

        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            on_timer(inner).await;
        });

        self.timer_handle = Some(handle);
        debug!("Timer armed, next fire in {}ms", delay_ms);
    }
}

/// Called when the timer fires. Executes due jobs and rearms.
/// Returns a boxed Send future to allow recursive spawning via tokio::spawn.
fn on_timer(inner: Arc<Mutex<CronServiceInner>>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(on_timer_impl(inner))
}

async fn on_timer_impl(inner: Arc<Mutex<CronServiceInner>>) {
    let mut guard = inner.lock().await;
    let now = now_ms();
    let mut jobs_to_run: Vec<usize> = Vec::new();
    let mut jobs_to_delete: Vec<String> = Vec::new();

    // Find due jobs
    for (i, job) in guard.store.jobs.iter().enumerate() {
        if !job.enabled {
            continue;
        }
        if let Some(next) = job.state.next_run_at_ms {
            if next <= now {
                jobs_to_run.push(i);
            }
        }
    }

    // Execute due jobs
    for &idx in &jobs_to_run {
        let job = guard.store.jobs[idx].clone();
        debug!("Firing cron job '{}' (id={})", job.name, job.id);

        if let Some(ref callback) = guard.on_job {
            match callback(job.clone()).await {
                Ok(err_msg) => {
                    guard.store.jobs[idx].state.last_run_at_ms = Some(now);
                    if let Some(msg) = err_msg {
                        guard.store.jobs[idx].state.last_status = Some(JobStatus::Error);
                        guard.store.jobs[idx].state.last_error = Some(msg);
                    } else {
                        guard.store.jobs[idx].state.last_status = Some(JobStatus::Ok);
                        guard.store.jobs[idx].state.last_error = None;
                    }
                }
                Err(e) => {
                    error!("Cron job '{}' failed: {}", job.name, e);
                    guard.store.jobs[idx].state.last_run_at_ms = Some(now);
                    guard.store.jobs[idx].state.last_status = Some(JobStatus::Error);
                    guard.store.jobs[idx].state.last_error = Some(e.to_string());
                }
            }
        } else {
            // No callback, just mark as skipped
            guard.store.jobs[idx].state.last_run_at_ms = Some(now);
            guard.store.jobs[idx].state.last_status = Some(JobStatus::Skipped);
            warn!("No on_job callback set, skipping job '{}'", job.name);
        }

        // Schedule delete if needed
        if guard.store.jobs[idx].delete_after_run {
            jobs_to_delete.push(guard.store.jobs[idx].id.clone());
        } else {
            // Recompute next run
            guard.store.jobs[idx].state.next_run_at_ms =
                compute_next_run(&guard.store.jobs[idx].schedule, now);
            guard.store.jobs[idx].updated_at_ms = now;
        }
    }

    // Delete one-shot jobs
    if !jobs_to_delete.is_empty() {
        guard
            .store
            .jobs
            .retain(|j| !jobs_to_delete.contains(&j.id));
    }

    // Save and rearm
    if let Err(e) = save_store_inner(&guard.store_path, &guard.store) {
        error!("Failed to save cron store: {}", e);
    }

    // Rearm: find next fire time
    let next_fire = guard
        .store
        .jobs
        .iter()
        .filter(|j| j.enabled)
        .filter_map(|j| j.state.next_run_at_ms)
        .min();

    if let Some(next_ms) = next_fire {
        let delay_ms = (next_ms - now_ms()).max(0) as u64;
        let inner_clone = inner.clone();
        drop(guard);
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            on_timer(inner_clone).await;
        });
    }
}

fn save_store_inner(path: &PathBuf, store: &CronStore) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Compute the next run time for a schedule given the current time.
pub fn compute_next_run(schedule: &CronSchedule, now_ms: i64) -> Option<i64> {
    match schedule.kind {
        ScheduleKind::At => {
            // One-shot: fires at the specified time (if in the future)
            schedule.at_ms.filter(|&t| t > now_ms)
        }
        ScheduleKind::Every => {
            // Repeating: next fire is now + interval
            schedule.every_ms.map(|interval| now_ms + interval)
        }
        ScheduleKind::Cron => {
            // Parse cron expression and find next fire time
            let expr = schedule.expr.as_deref()?;
            match expr.parse::<Schedule>() {
                Ok(cron_schedule) => {
                    let next = cron_schedule.upcoming(Utc).next()?;
                    Some(next.timestamp_millis())
                }
                Err(e) => {
                    error!("Invalid cron expression '{}': {}", expr, e);
                    None
                }
            }
        }
    }
}
