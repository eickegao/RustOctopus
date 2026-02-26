use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::traits::{Tool, ToolError};
use crate::mcp::manager::McpManager;

/// A bridge that exposes an MCP server tool as a native [`Tool`] instance.
///
/// When the LLM selects an MCP-backed tool, `McpTool::execute()` delegates to
/// [`McpManager::call_tool()`] which forwards the call to the appropriate child
/// process over the Model Context Protocol.
pub struct McpTool {
    /// Namespaced name shown to the LLM, e.g. `mcp_filesystem_read_file`.
    tool_name: String,
    /// Human-readable description of the tool.
    tool_description: String,
    /// JSON Schema describing the tool's input parameters.
    tool_parameters: Value,
    /// Key of the MCP server in the config map, e.g. `"filesystem"`.
    server_name: String,
    /// Original tool name as reported by the MCP server, e.g. `"read_file"`.
    original_name: String,
    /// Shared reference to the manager that owns the running server processes.
    manager: Arc<Mutex<McpManager>>,
    /// Whether this tool may be executed without user confirmation.
    auto_approved: bool,
}

impl McpTool {
    pub fn new(
        tool_name: String,
        tool_description: String,
        tool_parameters: Value,
        server_name: String,
        original_name: String,
        manager: Arc<Mutex<McpManager>>,
        auto_approved: bool,
    ) -> Self {
        let tool_parameters = sanitize_schema(tool_parameters);
        Self {
            tool_name,
            tool_description,
            tool_parameters,
            server_name,
            original_name,
            manager,
            auto_approved,
        }
    }

    /// Whether this tool may be executed without user confirmation.
    pub fn is_auto_approved(&self) -> bool {
        self.auto_approved
    }
}

/// Sanitize a JSON Schema so it is accepted by the OpenAI function-calling API.
///
/// MCP servers may return schemas that use features not supported by OpenAI,
/// such as array-style type unions (`[{"type":"number"},{"type":"number"}]`).
/// This function recursively rewrites such constructs into compatible forms.
fn sanitize_schema(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            // If a schema node has a "type" field that is an array, convert it.
            // e.g. [{"type":"number"},{"type":"string"}] → {"type":"string"}
            // e.g. ["number","string"] → {"type":"string"}
            if let Some(type_val) = map.get("type") {
                if type_val.is_array() {
                    let first_type = type_val
                        .as_array()
                        .unwrap()
                        .iter()
                        .find_map(|v| {
                            v.as_str().map(String::from).or_else(|| {
                                v.get("type").and_then(|t| t.as_str()).map(String::from)
                            })
                        })
                        .unwrap_or_else(|| "string".to_string());
                    map.insert("type".to_string(), Value::String(first_type));
                }
            }
            // If "items" is a tuple-validation array (e.g. [{"type":"number"},{"type":"number"}]),
            // collapse it to the first element. OpenAI only accepts a single schema for "items".
            if let Some(items_val) = map.get("items") {
                if items_val.is_array() {
                    let first = items_val.as_array().unwrap().first().cloned()
                        .unwrap_or(Value::Object(Default::default()));
                    map.insert("items".to_string(), first);
                }
            }
            // Recurse into all values.
            for val in map.values_mut() {
                *val = sanitize_schema(val.take());
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                *item = sanitize_schema(item.take());
            }
        }
        _ => {}
    }
    value
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters(&self) -> Value {
        self.tool_parameters.clone()
    }

    async fn execute(&self, params: Value) -> Result<String, ToolError> {
        let mgr = self.manager.lock().await;
        mgr.call_tool(&self.server_name, &self.original_name, params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_tool() -> McpTool {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        McpTool::new(
            "mcp_filesystem_read_file".into(),
            "Read a file from the filesystem".into(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
            "filesystem".into(),
            "read_file".into(),
            manager,
            false,
        )
    }

    #[test]
    fn test_tool_name() {
        let tool = make_test_tool();
        assert_eq!(tool.name(), "mcp_filesystem_read_file");
    }

    #[test]
    fn test_tool_description() {
        let tool = make_test_tool();
        assert_eq!(tool.description(), "Read a file from the filesystem");
    }

    #[test]
    fn test_tool_parameters() {
        let tool = make_test_tool();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["path"].is_object());
    }

    #[test]
    fn test_auto_approved_false() {
        let tool = make_test_tool();
        assert!(!tool.is_auto_approved());
    }

    #[test]
    fn test_auto_approved_true() {
        let manager = Arc::new(Mutex::new(McpManager::new()));
        let tool = McpTool::new(
            "mcp_test_foo".into(),
            "test".into(),
            serde_json::json!({}),
            "test".into(),
            "foo".into(),
            manager,
            true,
        );
        assert!(tool.is_auto_approved());
    }

    #[test]
    fn test_sanitize_schema_array_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "pos": {
                    "type": [{"type": "number"}, {"type": "number"}],
                    "description": "Position"
                },
                "name": {"type": "string"}
            }
        });
        let sanitized = super::sanitize_schema(schema);
        assert_eq!(sanitized["properties"]["pos"]["type"], "number");
        assert_eq!(sanitized["properties"]["name"]["type"], "string");
    }

    #[test]
    fn test_sanitize_schema_string_array_type() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "value": {"type": ["number", "string"]}
            }
        });
        let sanitized = super::sanitize_schema(schema);
        assert_eq!(sanitized["properties"]["value"]["type"], "number");
    }

    #[test]
    fn test_sanitize_schema_tuple_items() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "view_range": {
                    "type": "array",
                    "items": [{"type": "number"}, {"type": "number"}],
                    "minItems": 2,
                    "maxItems": 2
                }
            }
        });
        let sanitized = super::sanitize_schema(schema);
        // items should be collapsed from array to single schema
        assert_eq!(sanitized["properties"]["view_range"]["items"]["type"], "number");
        assert!(sanitized["properties"]["view_range"]["items"].is_object());
    }

    #[tokio::test]
    async fn test_execute_server_not_found() {
        let tool = make_test_tool();
        let result = tool
            .execute(serde_json::json!({"path": "/tmp/test"}))
            .await;
        assert!(result.is_err());
        // Should fail because no server "filesystem" is running in the manager.
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not running"),
            "Expected 'not running' in error: {err}"
        );
    }
}
