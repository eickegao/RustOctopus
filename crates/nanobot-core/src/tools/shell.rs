use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use tokio::process::Command;

use super::traits::{Tool, ToolError};

/// Dangerous command patterns that are always blocked.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "rm -rf .",
    "del /f",
    "format ",
    "mkfs",
    "dd if=",
    "shutdown",
    "reboot",
    ":(){ :|:& };:",
];

pub struct ExecTool {
    working_dir: String,
    timeout_secs: u64,
    restrict_to_workspace: bool,
}

impl ExecTool {
    pub fn new(working_dir: &str, timeout_secs: u64, restrict_to_workspace: bool) -> Self {
        Self {
            working_dir: working_dir.to_string(),
            timeout_secs,
            restrict_to_workspace,
        }
    }

    /// Check if a command matches any dangerous pattern.
    fn is_dangerous(command: &str) -> bool {
        let trimmed = command.trim();
        for pattern in DANGEROUS_PATTERNS {
            if trimmed.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Check if a command tries to escape the workspace when restriction is enabled.
    fn escapes_workspace(&self, command: &str) -> bool {
        if !self.restrict_to_workspace {
            return false;
        }

        // Reject commands containing ../ that could navigate above workspace
        if command.contains("../") {
            return true;
        }

        // Reject absolute paths that are outside the working directory.
        // Simple heuristic: look for tokens starting with / that don't start with working_dir.
        for token in command.split_whitespace() {
            if token.starts_with('/') && !token.starts_with(&self.working_dir) {
                return true;
            }
        }

        false
    }
}

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use with caution."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: command".into()))?;

        // 1. Safety check
        if Self::is_dangerous(command) {
            return Err(ToolError::ExecutionFailed(
                "Command blocked by safety filter".to_string(),
            ));
        }

        // 2. Workspace restriction
        if self.escapes_workspace(command) {
            return Err(ToolError::ExecutionFailed(
                "Command blocked: attempts to access paths outside the workspace".to_string(),
            ));
        }

        // Determine working directory
        let cwd = params["working_dir"]
            .as_str()
            .unwrap_or(&self.working_dir);

        // 3. Execute with timeout
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to spawn command: {e}")))?;

        let timeout_duration = Duration::from_secs(self.timeout_secs);

        let result = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;

        match result {
            Err(_) => {
                // Timeout — child is killed on drop via kill_on_drop(true)
                Ok(format!(
                    "Command timed out after {} seconds",
                    self.timeout_secs
                ))
            }
            Ok(Err(e)) => Err(ToolError::ExecutionFailed(format!(
                "Failed to execute command: {e}"
            ))),
            Ok(Ok(output)) => {
                // 4. Format output
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result = String::new();

                // Add stdout lines
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }

                // Add stderr lines with prefix
                if !stderr.is_empty() {
                    for line in stderr.lines() {
                        if !result.is_empty() && !result.ends_with('\n') {
                            result.push('\n');
                        }
                        result.push_str("STDERR: ");
                        result.push_str(line);
                        result.push('\n');
                    }
                }

                // Append exit code if non-zero
                let exit_code = output.status.code().unwrap_or(-1);
                if exit_code != 0 {
                    if !result.is_empty() && !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push_str(&format!("(exit code: {})", exit_code));
                }

                // Truncate if too long
                const MAX_OUTPUT: usize = 10000;
                if result.len() > MAX_OUTPUT {
                    result.truncate(MAX_OUTPUT);
                    result.push_str("... (truncated)");
                }

                Ok(result)
            }
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
    async fn test_exec_simple() {
        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool
            .execute(json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(
            result.contains("hello"),
            "Expected 'hello' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_exec_stderr() {
        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool
            .execute(json!({"command": "echo err >&2"}))
            .await
            .unwrap();
        assert!(
            result.contains("STDERR"),
            "Expected 'STDERR' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_exec_deny_rm_rf() {
        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool.execute(json!({"command": "rm -rf /"})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Command blocked by safety filter"),
            "Expected safety filter message in: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_exec_deny_fork_bomb() {
        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool
            .execute(json!({"command": ":(){ :|:& };:"}))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Command blocked by safety filter"),
            "Expected safety filter message in: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_exec_nonzero_exit() {
        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool
            .execute(json!({"command": "false"}))
            .await
            .unwrap();
        assert!(
            result.contains("exit code"),
            "Expected 'exit code' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_exec_timeout() {
        let tool = ExecTool::new("/tmp", 1, false);
        let result = tool
            .execute(json!({"command": "sleep 10"}))
            .await
            .unwrap();
        assert!(
            result.contains("timed out"),
            "Expected 'timed out' in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_exec_custom_working_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tmp_path = tmp.path().canonicalize().unwrap();
        let tmp_str = tmp_path.to_str().unwrap();

        let tool = ExecTool::new("/tmp", 30, false);
        let result = tool
            .execute(json!({"command": "pwd", "working_dir": tmp_str}))
            .await
            .unwrap();
        assert!(
            result.contains(tmp_str),
            "Expected '{}' in: {}",
            tmp_str,
            result
        );
    }
}
