use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::json;

use super::traits::{Tool, ToolError};

/// Resolve a user-provided path against the workspace, with optional sandboxing.
///
/// 1. Expand `~` to the user's home directory.
/// 2. If the path is relative, join it with `workspace`.
/// 3. Normalize the result (without requiring the path to exist).
/// 4. If `allowed_dir` is set, verify the resolved path starts with it.
fn resolve_path(
    path: &str,
    workspace: &Path,
    allowed_dir: Option<&Path>,
) -> Result<PathBuf, ToolError> {
    // 1. Expand ~ to home dir
    let expanded = if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            home.join(path.strip_prefix("~/").unwrap_or(""))
        } else {
            PathBuf::from(path)
        }
    } else {
        PathBuf::from(path)
    };

    // 2. If relative, resolve against workspace
    let absolute = if expanded.is_relative() {
        workspace.join(&expanded)
    } else {
        expanded
    };

    // 3. Normalize (canonicalize if path exists, otherwise normalize manually)
    let resolved = if absolute.exists() {
        absolute
            .canonicalize()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to canonicalize path: {e}")))?
    } else {
        normalize_path(&absolute)
    };

    // 4. If allowed_dir is set, verify resolved path starts with allowed_dir
    if let Some(allowed) = allowed_dir {
        let allowed_canonical = if allowed.exists() {
            allowed.canonicalize().map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to canonicalize allowed_dir: {e}"))
            })?
        } else {
            normalize_path(allowed)
        };
        if !resolved.starts_with(&allowed_canonical) {
            return Err(ToolError::InvalidParams(format!(
                "Path '{}' is outside the allowed directory '{}'",
                path,
                allowed_canonical.display()
            )));
        }
    }

    Ok(resolved)
}

/// Normalize a path by resolving `.` and `..` components without requiring the path to exist.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => {
                components.push(other);
            }
        }
    }
    components.iter().collect()
}

// ---------------------------------------------------------------------------
// ReadFileTool
// ---------------------------------------------------------------------------

pub struct ReadFileTool {
    workspace: PathBuf,
    allowed_dir: Option<PathBuf>,
}

impl ReadFileTool {
    pub fn new(workspace: PathBuf, allowed_dir: Option<PathBuf>) -> Self {
        Self {
            workspace,
            allowed_dir,
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;

        let resolved = resolve_path(path_str, &self.workspace, self.allowed_dir.as_deref())?;

        std::fs::read_to_string(&resolved).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to read '{}': {}",
                resolved.display(),
                e
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// WriteFileTool
// ---------------------------------------------------------------------------

pub struct WriteFileTool {
    workspace: PathBuf,
    allowed_dir: Option<PathBuf>,
}

impl WriteFileTool {
    pub fn new(workspace: PathBuf, allowed_dir: Option<PathBuf>) -> Self {
        Self {
            workspace,
            allowed_dir,
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path. Creates parent directories if needed."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| {
                ToolError::InvalidParams("Missing required parameter: content".into())
            })?;

        let resolved = resolve_path(path_str, &self.workspace, self.allowed_dir.as_deref())?;

        // Create parent directories if needed
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::ExecutionFailed(format!(
                    "Failed to create parent directories for '{}': {}",
                    resolved.display(),
                    e
                ))
            })?;
        }

        let bytes = content.len();
        std::fs::write(&resolved, content).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to write '{}': {}",
                resolved.display(),
                e
            ))
        })?;

        Ok(format!("Wrote {} bytes to {}", bytes, resolved.display()))
    }
}

// ---------------------------------------------------------------------------
// EditFileTool
// ---------------------------------------------------------------------------

pub struct EditFileTool {
    workspace: PathBuf,
    allowed_dir: Option<PathBuf>,
}

impl EditFileTool {
    pub fn new(workspace: PathBuf, allowed_dir: Option<PathBuf>) -> Self {
        Self {
            workspace,
            allowed_dir,
        }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing old_text with new_text. The old_text must exist exactly in the file."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to edit"
                },
                "old_text": {
                    "type": "string",
                    "description": "The text to find and replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;
        let old_text = params["old_text"]
            .as_str()
            .ok_or_else(|| {
                ToolError::InvalidParams("Missing required parameter: old_text".into())
            })?;
        let new_text = params["new_text"]
            .as_str()
            .ok_or_else(|| {
                ToolError::InvalidParams("Missing required parameter: new_text".into())
            })?;

        let resolved = resolve_path(path_str, &self.workspace, self.allowed_dir.as_deref())?;

        let content = std::fs::read_to_string(&resolved).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to read '{}': {}",
                resolved.display(),
                e
            ))
        })?;

        // Count occurrences
        let count = content.matches(old_text).count();

        if count == 0 {
            return Err(ToolError::ExecutionFailed(format!(
                "old_text not found in '{}'",
                resolved.display()
            )));
        }

        if count > 1 {
            return Err(ToolError::ExecutionFailed(format!(
                "old_text found {} times in '{}' (expected exactly 1 match)",
                count,
                resolved.display()
            )));
        }

        let new_content = content.replacen(old_text, new_text, 1);

        std::fs::write(&resolved, new_content).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to write '{}': {}",
                resolved.display(),
                e
            ))
        })?;

        Ok(format!("Updated {}", resolved.display()))
    }
}

// ---------------------------------------------------------------------------
// ListDirTool
// ---------------------------------------------------------------------------

pub struct ListDirTool {
    workspace: PathBuf,
    allowed_dir: Option<PathBuf>,
}

impl ListDirTool {
    pub fn new(workspace: PathBuf, allowed_dir: Option<PathBuf>) -> Self {
        Self {
            workspace,
            allowed_dir,
        }
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List the contents of a directory."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The directory path to list"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;

        let resolved = resolve_path(path_str, &self.workspace, self.allowed_dir.as_deref())?;

        let entries = std::fs::read_dir(&resolved).map_err(|e| {
            ToolError::ExecutionFailed(format!(
                "Failed to list '{}': {}",
                resolved.display(),
                e
            ))
        })?;

        let mut items: Vec<(String, bool)> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to read directory entry: {}", e))
            })?;
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry
                .file_type()
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            items.push((name, is_dir));
        }

        items.sort_by(|a, b| a.0.cmp(&b.0));

        let lines: Vec<String> = items
            .into_iter()
            .map(|(name, is_dir)| {
                if is_dir {
                    format!("\u{1F4C1} {}", name)
                } else {
                    format!("\u{1F4C4} {}", name)
                }
            })
            .collect();

        Ok(lines.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("hello.txt");
        std::fs::write(&file_path, "Hello, world!").unwrap();

        let tool = ReadFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({"path": file_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert_eq!(result, "Hello, world!");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tmp = TempDir::new().unwrap();

        let tool = ReadFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({"path": tmp.path().join("nonexistent.txt").to_str().unwrap()}))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Failed to read"),
            "Expected 'Failed to read' in: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_write_file_creates_parents() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("sub").join("dir").join("file.txt");

        let tool = WriteFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({
                "path": file_path.to_str().unwrap(),
                "content": "nested content"
            }))
            .await
            .unwrap();

        assert!(result.contains("Wrote 14 bytes"));
        assert!(file_path.exists());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "nested content");
    }

    #[tokio::test]
    async fn test_edit_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("edit.txt");
        std::fs::write(&file_path, "foo bar baz").unwrap();

        let tool = EditFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({
                "path": file_path.to_str().unwrap(),
                "old_text": "bar",
                "new_text": "qux"
            }))
            .await
            .unwrap();

        assert!(result.contains("Updated"));
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "foo qux baz");
    }

    #[tokio::test]
    async fn test_edit_file_not_found_text() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("edit2.txt");
        std::fs::write(&file_path, "foo bar baz").unwrap();

        let tool = EditFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({
                "path": file_path.to_str().unwrap(),
                "old_text": "nonexistent",
                "new_text": "replacement"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "Expected 'not found' in: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_edit_file_ambiguous() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("ambiguous.txt");
        std::fs::write(&file_path, "aaa aaa").unwrap();

        let tool = EditFileTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({
                "path": file_path.to_str().unwrap(),
                "old_text": "aaa",
                "new_text": "bbb"
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("2 times"),
            "Expected mention of multiple matches in: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_list_dir() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("file.txt"), "content").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();

        let tool = ListDirTool::new(tmp.path().to_path_buf(), None);
        let result = tool
            .execute(json!({"path": tmp.path().to_str().unwrap()}))
            .await
            .unwrap();

        assert!(result.contains("file.txt"), "Expected 'file.txt' in: {}", result);
        assert!(result.contains("subdir"), "Expected 'subdir' in: {}", result);
        // Verify emoji prefixes
        assert!(result.contains("\u{1F4C1}"), "Expected folder emoji in: {}", result);
        assert!(result.contains("\u{1F4C4}"), "Expected file emoji in: {}", result);
    }

    #[tokio::test]
    async fn test_path_escape_blocked() {
        let tmp = TempDir::new().unwrap();

        let tool = ReadFileTool::new(
            tmp.path().to_path_buf(),
            Some(tmp.path().to_path_buf()),
        );
        let result = tool
            .execute(json!({"path": "/etc/passwd"}))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("outside the allowed directory"),
            "Expected 'outside the allowed directory' in: {}",
            err
        );
    }
}
