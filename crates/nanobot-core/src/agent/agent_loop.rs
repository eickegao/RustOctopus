//! Agent loop — the core processing engine.
//!
//! Receives messages from the bus, calls the LLM, executes tools, and returns
//! responses.  Named `agent_loop` rather than `loop` because `loop` is a Rust
//! reserved word.

use std::path::PathBuf;

use regex::Regex;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::agent::context::ContextBuilder;
use crate::bus::events::{InboundMessage, OutboundMessage};
use crate::bus::queue::MessageBus;
use crate::config::factory::resolve_workspace_path;
use crate::config::schema::Config;
use crate::providers::traits::{
    ChatMessage, ChatParams, LlmProvider, Role, ToolCallFunction, ToolCallMessage, ToolDefinition,
};
use crate::session::manager::{Session, SessionManager};
use crate::tools::filesystem::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
use crate::tools::registry::ToolRegistry;
use crate::tools::shell::ExecTool;
use crate::tools::web::{WebFetchTool, WebSearchTool};

/// Maximum number of characters for a tool result saved to session history.
const MAX_TOOL_RESULT_LEN: usize = 500;

/// The core processing engine.
///
/// Receives [`InboundMessage`]s, builds context, calls the LLM, iterates
/// through tool calls, and sends [`OutboundMessage`]s back through the bus.
pub struct AgentLoop {
    bus: MessageBus,
    provider: Box<dyn LlmProvider>,
    #[allow(dead_code)]
    workspace: PathBuf,
    model: String,
    pub(crate) max_iterations: usize,
    pub(crate) temperature: f64,
    pub(crate) max_tokens: u32,
    pub(crate) memory_window: usize,
    context: ContextBuilder,
    sessions: SessionManager,
    pub(crate) tools: ToolRegistry,
    inbound_rx: mpsc::UnboundedReceiver<InboundMessage>,
    _running: bool,
}

impl AgentLoop {
    /// Create a new `AgentLoop`.
    ///
    /// Registers the default tool set: filesystem tools (read, write, edit,
    /// list_dir), shell (exec), and web tools (web_search, web_fetch).
    pub fn new(
        bus: MessageBus,
        provider: Box<dyn LlmProvider>,
        workspace: PathBuf,
        inbound_rx: mpsc::UnboundedReceiver<InboundMessage>,
    ) -> Self {
        let model = provider.default_model().to_string();
        let context = ContextBuilder::new(workspace.clone());
        let sessions_dir = workspace.join("sessions");
        let sessions = SessionManager::new(sessions_dir);

        let mut tools = ToolRegistry::new();

        // Register default tools
        tools.register(Box::new(ReadFileTool::new(workspace.clone(), None)));
        tools.register(Box::new(WriteFileTool::new(workspace.clone(), None)));
        tools.register(Box::new(EditFileTool::new(workspace.clone(), None)));
        tools.register(Box::new(ListDirTool::new(workspace.clone(), None)));
        tools.register(Box::new(ExecTool::new(
            workspace.to_string_lossy().as_ref(),
            120,
            false,
        )));
        tools.register(Box::new(WebSearchTool::new(None)));
        tools.register(Box::new(WebFetchTool::new()));

        Self {
            bus,
            provider,
            workspace,
            model,
            max_iterations: 40,
            temperature: 0.1,
            max_tokens: 4096,
            memory_window: 100,
            context,
            sessions,
            tools,
            inbound_rx,
            _running: false,
        }
    }

    /// Create a new `AgentLoop` from a [`Config`].
    ///
    /// Resolves workspace path (tilde expansion), applies agent defaults
    /// (max_iterations, temperature, max_tokens, memory_window) from config.
    pub fn from_config(
        config: Config,
        bus: MessageBus,
        provider: Box<dyn LlmProvider>,
        inbound_rx: mpsc::UnboundedReceiver<InboundMessage>,
    ) -> Self {
        let workspace = resolve_workspace_path(&config.agents.defaults.workspace);
        let mut agent = Self::new(bus, provider, workspace, inbound_rx);
        agent.max_iterations = config.agents.defaults.max_tool_iterations as usize;
        agent.temperature = config.agents.defaults.temperature;
        agent.max_tokens = config.agents.defaults.max_tokens;
        agent.memory_window = config.agents.defaults.memory_window as usize;
        agent
    }

    /// Main loop: receive messages from the bus, process them, and send responses.
    ///
    /// Uses `tokio::select!` with a 1-second sleep timer so the loop can be
    /// stopped externally by dropping the inbound sender.
    pub async fn run(&mut self) {
        self._running = true;
        info!("AgentLoop started");

        loop {
            tokio::select! {
                msg = self.inbound_rx.recv() => {
                    match msg {
                        Some(inbound) => {
                            let response = self.process_message(&inbound).await;
                            self.bus.publish_outbound(response).await;
                        }
                        None => {
                            info!("Inbound channel closed, stopping AgentLoop");
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    // Tick — allows cooperative shutdown
                }
            }
        }

        self._running = false;
        info!("AgentLoop stopped");
    }

    /// Process a single inbound message and return an outbound response.
    async fn process_message(&mut self, msg: &InboundMessage) -> OutboundMessage {
        let session_key = msg.session_key();
        let content = msg.content.trim();

        // Handle /new command — clear session
        if content == "/new" {
            let mut session = self.sessions.get_or_create(&session_key);
            session.clear();
            let _ = self.sessions.save(&session);
            return OutboundMessage::new(&msg.channel, &msg.chat_id, "New session started.");
        }

        // Handle /help command
        if content == "/help" {
            let help = self.build_help_text();
            return OutboundMessage::new(&msg.channel, &msg.chat_id, &help);
        }

        let mut session = self.sessions.get_or_create(&session_key);

        // Trigger memory consolidation if unconsolidated messages exceed window
        let unconsolidated = session.messages.len() - session.last_consolidated;
        if unconsolidated > self.memory_window {
            let model = self.model.clone();
            self.context
                .memory_mut()
                .consolidate(
                    &mut session,
                    self.provider.as_ref(),
                    &model,
                    false,
                    self.memory_window,
                )
                .await;
            let _ = self.sessions.save(&session);
        }

        // Build context with history
        let history = session.get_history(self.memory_window);
        let history_messages = self.history_to_chat_messages(&history);

        let messages = self.context.build_messages(
            &history_messages,
            content,
            None,
            Some(&msg.channel),
            Some(&msg.chat_id),
        );

        // Run the agent loop
        let (final_content, _tools_used) = self.run_agent_loop(messages).await;

        // Save turn to session
        let response_text = final_content.unwrap_or_else(|| "(no response)".to_string());

        // Save user message + assistant response
        session.add_message("user", content);
        session.add_message("assistant", &response_text);
        let _ = self.sessions.save(&session);

        OutboundMessage::new(&msg.channel, &msg.chat_id, &response_text)
    }

    /// Iterative agent loop: call LLM, execute tool calls, repeat.
    ///
    /// Returns `(final_content, tools_used)` where `tools_used` is a list of
    /// tool names that were invoked during the loop.
    async fn run_agent_loop(
        &self,
        initial_messages: Vec<ChatMessage>,
    ) -> (Option<String>, Vec<String>) {
        let mut messages = initial_messages;
        let mut tools_used = Vec::new();
        let tool_defs = self.tools.get_definitions();
        let params = ChatParams {
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        };

        for iteration in 0..self.max_iterations {
            let tools_param: Option<&[ToolDefinition]> = if tool_defs.is_empty() {
                None
            } else {
                Some(&tool_defs)
            };

            let response = match self
                .provider
                .chat(&messages, tools_param, &self.model, &params)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!("LLM call failed on iteration {}: {}", iteration, e);
                    return (
                        Some(format!("Error: LLM call failed: {}", e)),
                        tools_used,
                    );
                }
            };

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

                // Execute each tool call and add results
                for tc in &response.tool_calls {
                    info!("Executing tool: {} (id: {})", tc.name, tc.id);
                    tools_used.push(tc.name.clone());
                    let result = self.tools.execute(&tc.name, tc.arguments.clone()).await;
                    ContextBuilder::add_tool_result(&mut messages, &tc.id, &tc.name, &result);
                }
            } else {
                // Text response — we're done
                let content = response.content.map(|c| Self::strip_think(&c));
                return (content, tools_used);
            }
        }

        warn!("Agent loop hit max iterations ({})", self.max_iterations);
        (
            Some("I've reached the maximum number of iterations. Please try rephrasing your request.".to_string()),
            tools_used,
        )
    }

    /// Process a message directly without the bus (for CLI / cron use).
    ///
    /// Creates an [`InboundMessage`] internally and calls [`process_message`].
    pub async fn process_direct(
        &mut self,
        content: &str,
        session_key: &str,
    ) -> anyhow::Result<String> {
        let parts: Vec<&str> = session_key.splitn(2, ':').collect();
        let (channel, chat_id) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            ("cli", session_key)
        };

        let mut msg = InboundMessage::new(channel, "user", chat_id, content);
        msg.session_key_override = Some(session_key.to_string());

        let response = self.process_message(&msg).await;
        Ok(response.content)
    }

    /// Save turn messages to a session, truncating tool results longer than
    /// [`MAX_TOOL_RESULT_LEN`] characters.
    #[allow(dead_code)]
    fn save_turn(session: &mut Session, messages: &[ChatMessage], skip: usize) {
        for msg in messages.iter().skip(skip) {
            let role_str = match msg.role {
                Role::System => continue, // don't save system messages
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };

            let content_str = msg
                .content
                .as_ref()
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Truncate tool results
            let content_str = if msg.role == Role::Tool && content_str.len() > MAX_TOOL_RESULT_LEN {
                format!("{}...(truncated)", &content_str[..MAX_TOOL_RESULT_LEN])
            } else {
                content_str
            };

            let mut extras = json!({});
            if let Some(ref tool_calls) = msg.tool_calls {
                extras["tool_calls"] = serde_json::to_value(tool_calls).unwrap_or_default();
            }
            if let Some(ref tool_call_id) = msg.tool_call_id {
                extras["tool_call_id"] = json!(tool_call_id);
            }
            if let Some(ref name) = msg.name {
                extras["name"] = json!(name);
            }

            let extras = if extras.as_object().is_none_or(|o| o.is_empty()) {
                None
            } else {
                Some(extras)
            };

            session.add_message_with_extras(role_str, &content_str, extras);
        }
    }

    /// Remove `<think>...</think>` blocks from LLM responses.
    fn strip_think(text: &str) -> String {
        let re = Regex::new(r"(?s)<think>.*?</think>").unwrap();
        re.replace_all(text, "").trim().to_string()
    }

    /// Convert session history (serde_json::Value items) to ChatMessage format.
    fn history_to_chat_messages(&self, history: &[serde_json::Value]) -> Vec<ChatMessage> {
        history
            .iter()
            .filter_map(|m| {
                let role_str = m.get("role")?.as_str()?;
                let role = match role_str {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "tool" => Role::Tool,
                    "system" => Role::System,
                    _ => return None,
                };

                let content = m.get("content").cloned();

                let tool_calls = m
                    .get("tool_calls")
                    .and_then(|tc| serde_json::from_value::<Vec<ToolCallMessage>>(tc.clone()).ok());

                let tool_call_id = m
                    .get("tool_call_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let name = m.get("name").and_then(|v| v.as_str()).map(String::from);

                Some(ChatMessage {
                    role,
                    content,
                    tool_calls,
                    tool_call_id,
                    name,
                })
            })
            .collect()
    }

    /// Build help text listing available commands and tools.
    fn build_help_text(&self) -> String {
        let tool_names = self.tools.tool_names();
        let tools_list = if tool_names.is_empty() {
            "  (none)".to_string()
        } else {
            tool_names
                .iter()
                .map(|n| format!("  - {}", n))
                .collect::<Vec<_>>()
                .join("\n")
        };

        format!(
            "nanobot help\n\n\
             Commands:\n\
             /new   - Start a new session\n\
             /help  - Show this help\n\n\
             Available tools:\n\
             {}",
            tools_list
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::queue::MessageBus;
    use crate::config::schema::Config;
    use crate::providers::traits::*;
    use tempfile::TempDir;

    struct MockProvider {
        response: String,
    }

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
                content: Some(self.response.clone()),
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

    #[tokio::test]
    async fn test_from_config() {
        let dir = TempDir::new().unwrap();
        let mut config = Config::default();
        config.agents.defaults.max_tool_iterations = 10;
        config.agents.defaults.temperature = 0.5;
        config.agents.defaults.max_tokens = 2048;
        config.agents.defaults.memory_window = 50;
        config.agents.defaults.workspace = dir.path().to_string_lossy().to_string();

        let provider = Box::new(MockProvider {
            response: "ok".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let agent = AgentLoop::from_config(config, bus, provider, inbound_rx);
        assert_eq!(agent.max_iterations, 10);
        assert!((agent.temperature - 0.5).abs() < f64::EPSILON);
        assert_eq!(agent.max_tokens, 2048);
        assert_eq!(agent.memory_window, 50);
    }

    #[tokio::test]
    async fn test_process_direct_simple() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider {
            response: "Hello!".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        let result = agent.process_direct("hi", "test:chat").await.unwrap();
        assert_eq!(result, "Hello!");
    }

    #[tokio::test]
    async fn test_slash_new() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider {
            response: "done".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        agent.process_direct("hello", "test:chat").await.unwrap();
        let result = agent.process_direct("/new", "test:chat").await.unwrap();
        assert!(result.contains("New session") || result.contains("new session"));
    }

    #[tokio::test]
    async fn test_slash_help() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider {
            response: "unused".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        let result = agent.process_direct("/help", "test:chat").await.unwrap();
        assert!(result.contains("nanobot help"));
        assert!(result.contains("/new"));
    }

    #[tokio::test]
    async fn test_strip_think() {
        let input = "<think>internal reasoning</think>Hello world";
        let output = AgentLoop::strip_think(input);
        assert_eq!(output, "Hello world");
    }

    #[tokio::test]
    async fn test_strip_think_multiline() {
        let input = "<think>\nstep 1\nstep 2\n</think>\nFinal answer";
        let output = AgentLoop::strip_think(input);
        assert_eq!(output, "Final answer");
    }

    #[tokio::test]
    async fn test_strip_think_no_tags() {
        let input = "Just a normal response";
        let output = AgentLoop::strip_think(input);
        assert_eq!(output, "Just a normal response");
    }

    #[tokio::test]
    async fn test_save_turn_truncates_tool_results() {
        let mut session = Session::new("test:chat");
        let long_result = "x".repeat(1000);
        let messages = vec![
            ChatMessage {
                role: Role::Tool,
                content: Some(serde_json::Value::String(long_result)),
                tool_calls: None,
                tool_call_id: Some("call_1".into()),
                name: Some("read_file".into()),
            },
        ];
        AgentLoop::save_turn(&mut session, &messages, 0);
        assert_eq!(session.messages.len(), 1);
        let saved_content = session.messages[0]["content"].as_str().unwrap();
        assert!(saved_content.len() < 600);
        assert!(saved_content.contains("...(truncated)"));
    }

    #[tokio::test]
    async fn test_tool_registration() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider {
            response: "test".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        assert!(agent.tools.has("read_file"));
        assert!(agent.tools.has("write_file"));
        assert!(agent.tools.has("edit_file"));
        assert!(agent.tools.has("list_dir"));
        assert!(agent.tools.has("exec"));
        assert!(agent.tools.has("web_search"));
        assert!(agent.tools.has("web_fetch"));
        assert_eq!(agent.tools.len(), 7);
    }

    #[tokio::test]
    async fn test_process_direct_session_persistence() {
        let dir = TempDir::new().unwrap();
        let provider = Box::new(MockProvider {
            response: "I remember".into(),
        });
        let (bus, inbound_rx, _outbound_rx) = MessageBus::new();

        let mut agent = AgentLoop::new(bus, provider, dir.path().to_path_buf(), inbound_rx);
        agent
            .process_direct("remember this", "test:chat")
            .await
            .unwrap();

        // Session should have messages saved
        let session = agent.sessions.get_or_create("test:chat");
        assert!(session.messages.len() >= 2); // user + assistant
    }
}
