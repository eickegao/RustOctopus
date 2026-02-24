//! Subagent manager for background task execution.
//!
//! The [`SubagentManager`] spawns isolated agent loops that run in the
//! background with a limited number of iterations and announce their results
//! via the message bus.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};
use uuid::Uuid;

use crate::agent::context::ContextBuilder;
use crate::bus::events::InboundMessage;
use crate::bus::queue::MessageBus;
use crate::providers::traits::{
    ChatMessage, ChatParams, LlmProvider, Role, ToolCallFunction, ToolCallMessage, ToolDefinition,
};
use crate::tools::filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
use crate::tools::registry::ToolRegistry;
use crate::tools::shell::ExecTool;
use crate::tools::web::{WebFetchTool, WebSearchTool};

/// Maximum number of tool-call iterations for a subagent.
const MAX_SUBAGENT_ITERATIONS: usize = 15;

/// Manages background subagent execution.
///
/// Subagents are lightweight agent instances that run in the background to
/// handle specific tasks.  They share the same LLM provider but have isolated
/// context and a focused system prompt.
pub struct SubagentManager {
    provider: Option<Arc<dyn LlmProvider>>,
    workspace: PathBuf,
    bus: Option<MessageBus>,
    model: String,
    temperature: f64,
    max_tokens: u32,
    running_tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl SubagentManager {
    /// Create a new `SubagentManager`.
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        workspace: PathBuf,
        bus: MessageBus,
        model: String,
        temperature: f64,
        max_tokens: u32,
    ) -> Self {
        Self {
            provider: Some(provider),
            workspace,
            bus: Some(bus),
            model,
            temperature,
            max_tokens,
            running_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Simplified constructor for testing.
    ///
    /// Does not require a real provider or bus -- only suitable for testing
    /// methods that do not perform LLM calls (e.g. [`build_subagent_prompt`]).
    pub fn new_for_test(workspace: PathBuf) -> Self {
        Self {
            provider: None,
            workspace,
            bus: None,
            model: String::new(),
            temperature: 0.7,
            max_tokens: 4096,
            running_tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build a focused system prompt for a subagent.
    pub fn build_subagent_prompt(&self, _task: &str) -> String {
        let now = chrono::Local::now();
        let time_str = now.format("%Y-%m-%d %H:%M (%A)").to_string();
        let tz = now.format("%Z").to_string();
        let workspace = self.workspace.display();

        format!(
            "# Subagent\n\n\
             ## Current Time\n\
             {} ({})\n\n\
             You are a subagent spawned by the main agent to complete a specific task.\n\n\
             ## Rules\n\
             1. Stay focused - complete only the assigned task, nothing else\n\
             2. Your final response will be reported back to the main agent\n\
             3. Do not initiate conversations or take on side tasks\n\
             4. Be concise but informative in your findings\n\n\
             ## What You Can Do\n\
             - Read and write files in the workspace\n\
             - Execute shell commands\n\
             - Search the web and fetch web pages\n\
             - Complete the task thoroughly\n\n\
             ## What You Cannot Do\n\
             - Send messages directly to users (no message tool available)\n\
             - Spawn other subagents\n\
             - Access the main agent's conversation history\n\n\
             ## Workspace\n\
             Your workspace is at: {}\n\
             Skills are available at: {}/skills/ (read SKILL.md files as needed)\n\n\
             When you have completed the task, provide a clear summary of your findings or actions.",
            time_str, tz, workspace, workspace
        )
    }

    /// Spawn a subagent to execute a task in the background.
    ///
    /// Returns a status message indicating the subagent was started.
    pub async fn spawn(
        &self,
        task: &str,
        label: Option<&str>,
        origin_channel: &str,
        origin_chat_id: &str,
    ) -> String {
        let task_id = Uuid::new_v4().to_string()[..8].to_string();
        let display_label = match label {
            Some(l) => l.to_string(),
            None => {
                if task.len() > 30 {
                    format!("{}...", &task[..30])
                } else {
                    task.to_string()
                }
            }
        };

        let origin_channel = origin_channel.to_string();
        let origin_chat_id = origin_chat_id.to_string();

        // Clone data for the spawned task
        let provider = self.provider.clone().expect("provider required for spawn");
        let bus = self.bus.clone().expect("bus required for spawn");
        let workspace = self.workspace.clone();
        let model = self.model.clone();
        let temperature = self.temperature;
        let max_tokens = self.max_tokens;
        let task_str = task.to_string();
        let label_clone = display_label.clone();
        let task_id_clone = task_id.clone();
        let running_tasks = Arc::clone(&self.running_tasks);
        let running_tasks_cleanup = Arc::clone(&self.running_tasks);
        let task_id_cleanup = task_id.clone();

        let handle = tokio::spawn(async move {
            run_subagent(
                task_id_clone,
                task_str,
                label_clone,
                origin_channel,
                origin_chat_id,
                provider,
                bus,
                workspace,
                model,
                temperature,
                max_tokens,
            )
            .await;

            // Cleanup on completion
            let mut tasks = running_tasks_cleanup.lock().await;
            tasks.remove(&task_id_cleanup);
        });

        {
            let mut tasks = running_tasks.lock().await;
            tasks.insert(task_id.clone(), handle);
        }

        info!("Spawned subagent [{}]: {}", task_id, display_label);
        format!(
            "Subagent [{}] started (id: {}). I'll notify you when it completes.",
            display_label, task_id
        )
    }

    /// Return the number of currently running subagents.
    pub async fn get_running_count(&self) -> usize {
        let tasks = self.running_tasks.lock().await;
        tasks.len()
    }
}

/// Build an isolated tool registry for a subagent.
///
/// Includes filesystem, shell, and web tools but explicitly excludes
/// message and spawn tools.
fn build_subagent_tools(workspace: &Path) -> ToolRegistry {
    let mut tools = ToolRegistry::new();

    tools.register(Box::new(ReadFileTool::new(workspace.to_path_buf(), None)));
    tools.register(Box::new(WriteFileTool::new(workspace.to_path_buf(), None)));
    tools.register(Box::new(EditFileTool::new(workspace.to_path_buf(), None)));
    tools.register(Box::new(ListDirTool::new(workspace.to_path_buf(), None)));
    tools.register(Box::new(ExecTool::new(
        workspace.to_string_lossy().as_ref(),
        120,
        false,
    )));
    tools.register(Box::new(WebSearchTool::new(None)));
    tools.register(Box::new(WebFetchTool::new()));

    tools
}

/// Execute the subagent task and announce the result.
///
/// This is a free function (rather than a method) so it can be moved into a
/// `tokio::spawn` closure without lifetime issues.
#[allow(clippy::too_many_arguments)]
async fn run_subagent(
    task_id: String,
    task: String,
    label: String,
    origin_channel: String,
    origin_chat_id: String,
    provider: Arc<dyn LlmProvider>,
    bus: MessageBus,
    workspace: PathBuf,
    model: String,
    temperature: f64,
    max_tokens: u32,
) {
    info!("Subagent [{}] starting task: {}", task_id, label);

    let result = run_subagent_inner(
        &task_id, &task, &workspace, provider.as_ref(), &model, temperature, max_tokens,
    )
    .await;

    let (result_text, status) = match result {
        Ok(text) => {
            info!("Subagent [{}] completed successfully", task_id);
            (text, "ok")
        }
        Err(e) => {
            warn!("Subagent [{}] failed: {}", task_id, e);
            (format!("Error: {}", e), "error")
        }
    };

    announce_result(
        &task_id,
        &label,
        &task,
        &result_text,
        &origin_channel,
        &origin_chat_id,
        status,
        &bus,
    )
    .await;
}

/// Inner logic for running the subagent tool loop.
async fn run_subagent_inner(
    task_id: &str,
    task: &str,
    workspace: &Path,
    provider: &dyn LlmProvider,
    model: &str,
    temperature: f64,
    max_tokens: u32,
) -> anyhow::Result<String> {
    let tools = build_subagent_tools(workspace);
    let tool_defs = tools.get_definitions();

    // Build initial messages
    let now = chrono::Local::now();
    let time_str = now.format("%Y-%m-%d %H:%M (%A)").to_string();
    let tz = now.format("%Z").to_string();
    let ws = workspace.display();

    let system_prompt = format!(
        "# Subagent\n\n\
         ## Current Time\n\
         {} ({})\n\n\
         You are a subagent spawned by the main agent to complete a specific task.\n\n\
         ## Rules\n\
         1. Stay focused - complete only the assigned task, nothing else\n\
         2. Your final response will be reported back to the main agent\n\
         3. Do not initiate conversations or take on side tasks\n\
         4. Be concise but informative in your findings\n\n\
         ## What You Can Do\n\
         - Read and write files in the workspace\n\
         - Execute shell commands\n\
         - Search the web and fetch web pages\n\
         - Complete the task thoroughly\n\n\
         ## What You Cannot Do\n\
         - Send messages directly to users (no message tool available)\n\
         - Spawn other subagents\n\
         - Access the main agent's conversation history\n\n\
         ## Workspace\n\
         Your workspace is at: {}\n\
         Skills are available at: {}/skills/ (read SKILL.md files as needed)\n\n\
         When you have completed the task, provide a clear summary of your findings or actions.",
        time_str, tz, ws, ws
    );

    let mut messages = vec![
        ChatMessage {
            role: Role::System,
            content: Some(serde_json::Value::String(system_prompt)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        ChatMessage {
            role: Role::User,
            content: Some(serde_json::Value::String(task.to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    let params = ChatParams {
        max_tokens,
        temperature,
    };

    for iteration in 0..MAX_SUBAGENT_ITERATIONS {
        let tools_param: Option<&[ToolDefinition]> = if tool_defs.is_empty() {
            None
        } else {
            Some(&tool_defs)
        };

        let response = provider.chat(&messages, tools_param, model, &params).await?;

        if response.has_tool_calls() {
            // Add assistant message with tool calls
            let tool_call_messages: Vec<ToolCallMessage> = response
                .tool_calls
                .iter()
                .map(|tc| ToolCallMessage {
                    id: tc.id.clone(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction {
                        name: tc.name.clone(),
                        arguments: tc.arguments.to_string(),
                    },
                })
                .collect();

            ContextBuilder::add_assistant_message(
                &mut messages,
                response.content.as_deref(),
                Some(tool_call_messages),
            );

            // Execute each tool call
            for tc in &response.tool_calls {
                info!(
                    "Subagent [{}] executing tool: {} (iteration {})",
                    task_id, tc.name, iteration
                );
                let result = tools.execute(&tc.name, tc.arguments.clone()).await;
                ContextBuilder::add_tool_result(&mut messages, &tc.id, &tc.name, &result);
            }
        } else {
            // Text response -- we're done
            let content = response
                .content
                .unwrap_or_else(|| "Task completed but no final response was generated.".into());
            return Ok(content);
        }
    }

    Ok("Task completed but no final response was generated.".to_string())
}

/// Announce the subagent result to the main agent via the message bus.
#[allow(clippy::too_many_arguments)]
async fn announce_result(
    task_id: &str,
    label: &str,
    task: &str,
    result: &str,
    origin_channel: &str,
    origin_chat_id: &str,
    status: &str,
    bus: &MessageBus,
) {
    let status_text = if status == "ok" {
        "completed successfully"
    } else {
        "failed"
    };

    let announce_content = format!(
        "[Subagent '{}' {}]\n\n\
         Task: {}\n\n\
         Result:\n\
         {}\n\n\
         Summarize this naturally for the user. Keep it brief (1-2 sentences). \
         Do not mention technical details like \"subagent\" or task IDs.",
        label, status_text, task, result
    );

    let msg = InboundMessage::new(
        "system",
        "subagent",
        &format!("{}:{}", origin_channel, origin_chat_id),
        &announce_content,
    );

    bus.publish_inbound(msg).await;
    info!(
        "Subagent [{}] announced result to {}:{}",
        task_id, origin_channel, origin_chat_id
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_prompt_contains_workspace() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        let prompt = manager.build_subagent_prompt("test task");
        assert!(prompt.contains("/tmp/workspace"));
        assert!(prompt.contains("Subagent"));
    }

    #[test]
    fn test_subagent_prompt_contains_time() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        let prompt = manager.build_subagent_prompt("test task");
        assert!(prompt.contains("Current Time"));
    }

    #[test]
    fn test_subagent_prompt_contains_rules() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        let prompt = manager.build_subagent_prompt("test task");
        assert!(prompt.contains("Stay focused"));
        assert!(prompt.contains("Cannot Do"));
    }

    #[test]
    fn test_subagent_prompt_workspace_skills_path() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        let prompt = manager.build_subagent_prompt("test task");
        assert!(prompt.contains("/tmp/workspace/skills/"));
    }

    #[tokio::test]
    async fn test_get_running_count_initially_zero() {
        let manager = SubagentManager::new_for_test("/tmp/workspace".into());
        assert_eq!(manager.get_running_count().await, 0);
    }

    #[test]
    fn test_build_subagent_tools() {
        let workspace = PathBuf::from("/tmp/workspace");
        let tools = build_subagent_tools(&workspace);
        assert!(tools.has("read_file"));
        assert!(tools.has("write_file"));
        assert!(tools.has("edit_file"));
        assert!(tools.has("list_dir"));
        assert!(tools.has("exec"));
        assert!(tools.has("web_search"));
        assert!(tools.has("web_fetch"));
        assert_eq!(tools.len(), 7);
    }

    #[tokio::test]
    async fn test_spawn_returns_status_message() {
        use crate::providers::traits::*;

        struct MockProvider;

        #[async_trait::async_trait]
        impl LlmProvider for MockProvider {
            async fn chat(
                &self,
                _messages: &[ChatMessage],
                _tools: Option<&[ToolDefinition]>,
                _model: &str,
                _params: &ChatParams,
            ) -> anyhow::Result<LlmResponse> {
                Ok(LlmResponse {
                    content: Some("Done.".to_string()),
                    tool_calls: vec![],
                    finish_reason: FinishReason::Stop,
                    usage: TokenUsage::default(),
                    reasoning_content: None,
                })
            }
            fn default_model(&self) -> &str {
                "mock"
            }
        }

        let (bus, _inbound_rx, _outbound_rx) = MessageBus::new();
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider);
        let manager = SubagentManager::new(
            provider,
            PathBuf::from("/tmp/workspace"),
            bus,
            "mock".to_string(),
            0.7,
            4096,
        );

        let result = manager
            .spawn("test task", Some("test-label"), "cli", "direct")
            .await;
        assert!(result.contains("test-label"));
        assert!(result.contains("started"));

        // Give the spawned task a moment to register and complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[test]
    fn test_spawn_label_truncation() {
        // Verify that long tasks get truncated in the label
        let long_task = "a".repeat(50);
        let label: Option<&str> = None;
        let display_label = match label {
            Some(l) => l.to_string(),
            None => {
                if long_task.len() > 30 {
                    format!("{}...", &long_task[..30])
                } else {
                    long_task.to_string()
                }
            }
        };
        assert_eq!(display_label.len(), 33); // 30 chars + "..."
        assert!(display_label.ends_with("..."));
    }
}
