use anyhow::{bail, Result};
use clap::Subcommand;

use rustoctopus_core::config::factory::resolve_workspace_path;
use rustoctopus_core::config::loader::load_config;
use rustoctopus_core::cron::{CronSchedule, CronService};

#[derive(Subcommand)]
pub enum CronAction {
    /// List all cron jobs
    List {
        /// Include disabled jobs
        #[arg(long)]
        all: bool,
    },
    /// Add a new cron job
    Add {
        /// Cron schedule: "every <N>s/m/h" or a cron expression (6 fields, sec-level)
        schedule: String,
        /// Message to send when triggered
        message: String,
        /// Session / channel to deliver to
        #[arg(long)]
        session: Option<String>,
    },
    /// Remove a cron job
    Remove {
        /// Job ID
        id: String,
    },
    /// Enable a cron job
    Enable {
        /// Job ID
        id: String,
    },
    /// Disable a cron job
    Disable {
        /// Job ID
        id: String,
    },
    /// Run a cron job immediately (prints its message)
    Run {
        /// Job ID
        id: String,
    },
}

fn cron_store_path() -> Result<std::path::PathBuf> {
    let config = load_config(None)?;
    let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
    Ok(workspace.join("cron_jobs.json"))
}

/// Parse a human-friendly schedule string into a CronSchedule.
///
/// Supports:
/// - "every 30s" / "every 5m" / "every 2h" → interval-based
/// - cron expression (6 fields) → cron-based
fn parse_schedule(input: &str) -> Result<CronSchedule> {
    let trimmed = input.trim();

    // "every <N><unit>" pattern
    if let Some(rest) = trimmed.strip_prefix("every ").or_else(|| trimmed.strip_prefix("every")) {
        let rest = rest.trim();
        if let Some(num_str) = rest.strip_suffix('s') {
            let secs: i64 = num_str.trim().parse().map_err(|_| anyhow::anyhow!("Invalid number: {}", num_str))?;
            return Ok(CronSchedule::every(secs * 1000));
        }
        if let Some(num_str) = rest.strip_suffix('m') {
            let mins: i64 = num_str.trim().parse().map_err(|_| anyhow::anyhow!("Invalid number: {}", num_str))?;
            return Ok(CronSchedule::every(mins * 60 * 1000));
        }
        if let Some(num_str) = rest.strip_suffix('h') {
            let hours: i64 = num_str.trim().parse().map_err(|_| anyhow::anyhow!("Invalid number: {}", num_str))?;
            return Ok(CronSchedule::every(hours * 3600 * 1000));
        }
        bail!("Invalid interval format: '{}'. Use e.g. 'every 30s', 'every 5m', 'every 2h'", rest);
    }

    // Otherwise treat as a cron expression
    Ok(CronSchedule::cron_expr(trimmed, None))
}

pub fn run(action: CronAction) -> Result<()> {
    let store_path = cron_store_path()?;
    let mut service = CronService::new(store_path);
    // Load existing jobs if the store exists
    let _ = service.start();

    match action {
        CronAction::List { all } => {
            let jobs = service.list_jobs(all);
            if jobs.is_empty() {
                println!("No cron jobs.");
            } else {
                println!("{:<38} {:<16} {:<8} Message", "ID", "Name", "Enabled");
                println!("{}", "-".repeat(80));
                for job in &jobs {
                    println!(
                        "{:<38} {:<16} {:<8} {}",
                        job.id,
                        job.name,
                        if job.enabled { "yes" } else { "no" },
                        job.payload.message,
                    );
                }
                println!("\n{} job(s) total", jobs.len());
            }
        }
        CronAction::Add {
            schedule,
            message,
            session,
        } => {
            let sched = parse_schedule(&schedule)?;
            let job = service.add_job(
                &message[..message.len().min(30)], // name = truncated message
                sched,
                &message,
                false,
                session.as_deref(),
                None,
            )?;
            println!("Added cron job: {}", job.id);
        }
        CronAction::Remove { id } => {
            if service.remove_job(&id) {
                println!("Removed job {}", id);
            } else {
                bail!("Job not found: {}", id);
            }
        }
        CronAction::Enable { id } => {
            if service.enable_job(&id, true) {
                println!("Enabled job {}", id);
            } else {
                bail!("Job not found: {}", id);
            }
        }
        CronAction::Disable { id } => {
            if service.enable_job(&id, false) {
                println!("Disabled job {}", id);
            } else {
                bail!("Job not found: {}", id);
            }
        }
        CronAction::Run { id } => {
            let jobs = service.list_jobs(true);
            if let Some(job) = jobs.iter().find(|j| j.id == id) {
                println!("Job '{}' message: {}", job.name, job.payload.message);
                println!("(To actually execute, run the gateway which processes cron callbacks)");
            } else {
                bail!("Job not found: {}", id);
            }
        }
    }

    service.stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_schedule_every_seconds() {
        let s = parse_schedule("every 30s").unwrap();
        assert_eq!(s.every_ms, Some(30_000));
    }

    #[test]
    fn test_parse_schedule_every_minutes() {
        let s = parse_schedule("every 5m").unwrap();
        assert_eq!(s.every_ms, Some(300_000));
    }

    #[test]
    fn test_parse_schedule_every_hours() {
        let s = parse_schedule("every 2h").unwrap();
        assert_eq!(s.every_ms, Some(7_200_000));
    }

    #[test]
    fn test_parse_schedule_cron_expr() {
        let s = parse_schedule("0 0 9 * * *").unwrap();
        assert_eq!(s.expr.as_deref(), Some("0 0 9 * * *"));
    }

    #[test]
    fn test_parse_schedule_invalid_every() {
        assert!(parse_schedule("every abc").is_err());
    }
}
