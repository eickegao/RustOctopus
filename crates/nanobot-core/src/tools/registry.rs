use std::collections::HashMap;

use super::traits::Tool;
use crate::providers::traits::{FunctionDef, ToolDefinition};

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                def_type: "function".to_string(),
                function: FunctionDef {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters(),
                },
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, params: serde_json::Value) -> String {
        let hint = "\n\n[Analyze the error above and try a different approach.]";
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => {
                let names: Vec<_> = self.tools.keys().map(|k| k.as_str()).collect();
                return format!(
                    "Error: Tool '{}' not found. Available: {}{}",
                    name,
                    names.join(", "),
                    hint
                );
            }
        };
        match tool.execute(params).await {
            Ok(result) => {
                if result.starts_with("Error") {
                    format!("{}{}", result, hint)
                } else {
                    result
                }
            }
            Err(e) => format!("Error executing {}: {}{}", name, e, hint),
        }
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
