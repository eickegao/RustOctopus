use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use super::traits::{Tool, ToolError};

// ---------------------------------------------------------------------------
// Internal mutable state
// ---------------------------------------------------------------------------

struct SpawnToolState {
    origin_channel: String,
    origin_chat_id: String,
}

// ---------------------------------------------------------------------------
// SpawnTool
// ---------------------------------------------------------------------------

pub struct SpawnTool {
    state: Mutex<SpawnToolState>,
}

impl SpawnTool {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SpawnToolState {
                origin_channel: "cli".to_string(),
                origin_chat_id: "direct".to_string(),
            }),
        }
    }

    /// Set the origin context for subagent announcements.
    pub fn set_context(&self, channel: &str, chat_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.origin_channel = channel.to_string();
        state.origin_chat_id = chat_id.to_string();
    }
}

impl Default for SpawnTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SpawnTool {
    fn name(&self) -> &str {
        "spawn"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to handle a task in the background. \
         Use this for complex or time-consuming tasks that can run independently. \
         The subagent will complete the task and report back when done."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The task for the subagent to complete"
                },
                "label": {
                    "type": "string",
                    "description": "Optional short label for the task (for display)"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let task = params["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: task".into()))?;

        let label = params["label"].as_str().map(|s| s.to_string());

        let _context = {
            let state = self.state.lock().unwrap();
            (state.origin_channel.clone(), state.origin_chat_id.clone())
        };

        // Build a display name from the label or a truncated task
        let display = label.unwrap_or_else(|| {
            if task.len() > 50 {
                format!("{}...", &task[..50])
            } else {
                task.to_string()
            }
        });

        // NOTE: actual SubagentManager integration will be done in Task 15.
        // For now, return a status string.
        Ok(format!(
            "Spawned subagent for task: {display}. Results will be announced when complete."
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_tool_returns_status() {
        let tool = SpawnTool::new();
        let result = tool
            .execute(json!({"task": "do something", "label": "my-task"}))
            .await
            .unwrap();
        assert!(
            result.contains("my-task"),
            "Expected 'my-task' in: {}",
            result
        );
        assert!(
            result.contains("Spawned subagent"),
            "Expected 'Spawned subagent' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_spawn_tool_no_label_short_task() {
        let tool = SpawnTool::new();
        let result = tool
            .execute(json!({"task": "short task"}))
            .await
            .unwrap();
        assert!(
            result.contains("short task"),
            "Expected task text in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_spawn_tool_no_label_long_task() {
        let tool = SpawnTool::new();
        let long_task = "a".repeat(100);
        let result = tool
            .execute(json!({"task": long_task}))
            .await
            .unwrap();
        // Should truncate to 50 chars + "..."
        assert!(
            result.contains("..."),
            "Expected truncation in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_spawn_tool_missing_task() {
        let tool = SpawnTool::new();
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("task"),
            "Expected missing task error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_spawn_tool_set_context() {
        let tool = SpawnTool::new();
        tool.set_context("telegram", "789");
        // Context is stored but not visible in output yet (will be used by SubagentManager)
        let result = tool
            .execute(json!({"task": "do work"}))
            .await
            .unwrap();
        assert!(result.contains("Spawned subagent"));
    }
}
