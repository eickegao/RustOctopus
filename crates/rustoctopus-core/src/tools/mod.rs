pub mod cron_tool;
pub mod filesystem;
pub mod message;
pub mod registry;
pub mod shell;
pub mod spawn;
pub mod traits;
pub mod web;

pub use cron_tool::CronTool;
pub use message::MessageTool;
pub use registry::ToolRegistry;
pub use spawn::SpawnTool;
pub use traits::{Tool, ToolError};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes input"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            })
        }
        async fn execute(&self, params: serde_json::Value) -> Result<String, ToolError> {
            Ok(params["text"].as_str().unwrap_or("").to_string())
        }
    }

    struct FailTool;

    #[async_trait]
    impl Tool for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<String, ToolError> {
            Err(ToolError::ExecutionFailed("something broke".to_string()))
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let result = registry
            .execute("echo", serde_json::json!({"text": "hello"}))
            .await;
        assert_eq!(result, "hello");
    }

    #[tokio::test]
    async fn test_not_found() {
        let registry = ToolRegistry::new();
        let result = registry
            .execute("nonexistent", serde_json::json!({}))
            .await;
        assert!(result.contains("not found"), "Expected 'not found' in: {}", result);
    }

    #[tokio::test]
    async fn test_get_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        let defs = registry.get_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "echo");
        assert_eq!(defs[0].function.description, "Echoes input");
        assert_eq!(defs[0].def_type, "function");
    }

    #[tokio::test]
    async fn test_has_and_unregister() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool));
        assert!(registry.has("echo"));
        registry.unregister("echo");
        assert!(!registry.has("echo"));
    }

    #[tokio::test]
    async fn test_execute_error_appends_hint() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(FailTool));
        let result = registry.execute("fail", serde_json::json!({})).await;
        assert!(
            result.contains("something broke"),
            "Expected error message in: {}",
            result
        );
        assert!(
            result.contains("[Analyze the error above and try a different approach.]"),
            "Expected hint in: {}",
            result
        );
    }

    #[tokio::test]
    async fn test_len_and_is_empty() {
        let mut registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        registry.register(Box::new(EchoTool));
        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
    }
}
