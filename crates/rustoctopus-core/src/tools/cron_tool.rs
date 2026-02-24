use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::traits::{Tool, ToolError};

// ---------------------------------------------------------------------------
// CronJobEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CronJobEntry {
    pub id: String,
    pub name: String,
    pub message: String,
    pub schedule_desc: String,
}

// ---------------------------------------------------------------------------
// Internal mutable state
// ---------------------------------------------------------------------------

struct CronToolState {
    channel: String,
    chat_id: String,
    jobs: Vec<CronJobEntry>,
}

// ---------------------------------------------------------------------------
// CronTool
// ---------------------------------------------------------------------------

pub struct CronTool {
    state: Mutex<CronToolState>,
}

impl CronTool {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(CronToolState {
                channel: String::new(),
                chat_id: String::new(),
                jobs: Vec::new(),
            }),
        }
    }

    /// Set the current session context for delivery.
    pub fn set_context(&self, channel: &str, chat_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.channel = channel.to_string();
        state.chat_id = chat_id.to_string();
    }
}

impl Default for CronTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Schedule reminders and recurring tasks. Actions: add, list, remove."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "remove"],
                    "description": "Action to perform"
                },
                "message": {
                    "type": "string",
                    "description": "Reminder message (for add)"
                },
                "every_seconds": {
                    "type": "integer",
                    "description": "Interval in seconds (for recurring tasks)"
                },
                "cron_expr": {
                    "type": "string",
                    "description": "Cron expression like '0 9 * * *' (for scheduled tasks)"
                },
                "tz": {
                    "type": "string",
                    "description": "IANA timezone for cron expressions (e.g. 'America/Vancouver')"
                },
                "at": {
                    "type": "string",
                    "description": "ISO datetime for one-time execution (e.g. '2026-02-12T10:30:00')"
                },
                "job_id": {
                    "type": "string",
                    "description": "Job ID (for remove)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let action = params["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: action".into()))?;

        match action {
            "add" => self.add_job(&params),
            "list" => self.list_jobs(),
            "remove" => self.remove_job(&params),
            _ => Err(ToolError::InvalidParams(format!(
                "Unknown action: {action}"
            ))),
        }
    }
}

impl CronTool {
    fn add_job(&self, params: &serde_json::Value) -> Result<String, ToolError> {
        let message = params["message"]
            .as_str()
            .unwrap_or("")
            .to_string();

        if message.is_empty() {
            return Err(ToolError::InvalidParams(
                "message is required for add".to_string(),
            ));
        }

        let mut state = self.state.lock().unwrap();

        if state.channel.is_empty() || state.chat_id.is_empty() {
            return Err(ToolError::ExecutionFailed(
                "No session context (channel/chat_id)".to_string(),
            ));
        }

        let every_seconds = params["every_seconds"].as_u64();
        let cron_expr = params["cron_expr"].as_str();
        let tz = params["tz"].as_str();
        let at = params["at"].as_str();

        // Validate: tz only makes sense with cron_expr
        if tz.is_some() && cron_expr.is_none() {
            return Err(ToolError::InvalidParams(
                "tz can only be used with cron_expr".to_string(),
            ));
        }

        // Build a human-readable schedule description
        let schedule_desc = if let Some(secs) = every_seconds {
            format!("every {secs}s")
        } else if let Some(expr) = cron_expr {
            if let Some(tz_val) = tz {
                format!("cron({expr}, tz={tz_val})")
            } else {
                format!("cron({expr})")
            }
        } else if let Some(at_val) = at {
            format!("at {at_val}")
        } else {
            return Err(ToolError::InvalidParams(
                "Either every_seconds, cron_expr, or at is required".to_string(),
            ));
        };

        let name = if message.len() > 30 {
            message[..30].to_string()
        } else {
            message.clone()
        };

        let id = Uuid::new_v4().to_string();

        let entry = CronJobEntry {
            id: id.clone(),
            name: name.clone(),
            message,
            schedule_desc,
        };

        state.jobs.push(entry);

        Ok(format!("Created job '{name}' (id: {id})"))
    }

    fn list_jobs(&self) -> Result<String, ToolError> {
        let state = self.state.lock().unwrap();

        if state.jobs.is_empty() {
            return Ok("No scheduled jobs.".to_string());
        }

        let mut lines = vec!["Scheduled jobs:".to_string()];
        for job in &state.jobs {
            lines.push(format!(
                "- {} (id: {}, {})",
                job.name, job.id, job.schedule_desc
            ));
        }

        Ok(lines.join("\n"))
    }

    fn remove_job(&self, params: &serde_json::Value) -> Result<String, ToolError> {
        let job_id = params["job_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("job_id is required for remove".into()))?;

        let mut state = self.state.lock().unwrap();

        let before = state.jobs.len();
        state.jobs.retain(|j| j.id != job_id);
        let after = state.jobs.len();

        if before > after {
            Ok(format!("Removed job {job_id}"))
        } else {
            Ok(format!("Job {job_id} not found"))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cron_add_list_remove() {
        let tool = CronTool::new();
        tool.set_context("telegram", "123");

        // Add a job
        let result = tool
            .execute(json!({
                "action": "add",
                "message": "hello",
                "every_seconds": 60
            }))
            .await
            .unwrap();
        assert!(
            result.contains("Created job"),
            "Expected 'Created job' in: {}",
            result
        );
        assert!(
            result.contains("hello"),
            "Expected 'hello' in: {}",
            result
        );

        // Extract job id from "Created job 'hello' (id: <uuid>)"
        let id_start = result.find("(id: ").unwrap() + 5;
        let id_end = result.find(')').unwrap();
        let job_id = &result[id_start..id_end];

        // List jobs
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(
            result.contains("hello"),
            "Expected 'hello' in list: {}",
            result
        );
        assert!(
            result.contains(job_id),
            "Expected job id in list: {}",
            result
        );

        // Remove job
        let result = tool
            .execute(json!({"action": "remove", "job_id": job_id}))
            .await
            .unwrap();
        assert!(
            result.contains("Removed job"),
            "Expected 'Removed job' in: {}",
            result
        );

        // Verify list is now empty
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert_eq!(result, "No scheduled jobs.");
    }

    #[tokio::test]
    async fn test_cron_add_with_cron_expr() {
        let tool = CronTool::new();
        tool.set_context("discord", "456");

        let result = tool
            .execute(json!({
                "action": "add",
                "message": "morning check",
                "cron_expr": "0 9 * * *"
            }))
            .await
            .unwrap();
        assert!(result.contains("Created job"));

        let list = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(list.contains("cron(0 9 * * *)"));
    }

    #[tokio::test]
    async fn test_cron_add_with_at() {
        let tool = CronTool::new();
        tool.set_context("telegram", "789");

        let result = tool
            .execute(json!({
                "action": "add",
                "message": "one-time event",
                "at": "2026-03-01T10:00:00"
            }))
            .await
            .unwrap();
        assert!(result.contains("Created job"));

        let list = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(list.contains("at 2026-03-01T10:00:00"));
    }

    #[tokio::test]
    async fn test_cron_add_missing_message() {
        let tool = CronTool::new();
        tool.set_context("telegram", "123");

        let result = tool
            .execute(json!({"action": "add", "every_seconds": 60}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("message is required"),
            "Expected message required error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_add_missing_schedule() {
        let tool = CronTool::new();
        tool.set_context("telegram", "123");

        let result = tool
            .execute(json!({"action": "add", "message": "hello"}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("every_seconds, cron_expr, or at"),
            "Expected schedule required error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_add_no_context() {
        let tool = CronTool::new();
        // No set_context

        let result = tool
            .execute(json!({
                "action": "add",
                "message": "hello",
                "every_seconds": 60
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("No session context"),
            "Expected no-context error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_remove_not_found() {
        let tool = CronTool::new();

        let result = tool
            .execute(json!({"action": "remove", "job_id": "nonexistent"}))
            .await
            .unwrap();
        assert!(
            result.contains("not found"),
            "Expected 'not found' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_cron_remove_missing_job_id() {
        let tool = CronTool::new();

        let result = tool.execute(json!({"action": "remove"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("job_id is required"),
            "Expected job_id required error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_unknown_action() {
        let tool = CronTool::new();

        let result = tool
            .execute(json!({"action": "invalid"}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Unknown action"),
            "Expected unknown action error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_tz_without_cron_expr() {
        let tool = CronTool::new();
        tool.set_context("telegram", "123");

        let result = tool
            .execute(json!({
                "action": "add",
                "message": "hello",
                "every_seconds": 60,
                "tz": "America/Vancouver"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("tz can only be used with cron_expr"),
            "Expected tz error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cron_list_empty() {
        let tool = CronTool::new();

        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert_eq!(result, "No scheduled jobs.");
    }
}
