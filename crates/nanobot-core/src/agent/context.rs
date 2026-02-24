//! Context builder for assembling agent prompts.
//!
//! The [`ContextBuilder`] assembles the system prompt from identity information,
//! bootstrap files, memory, and skills, then builds the complete message list
//! for an LLM call.

use std::fs;
use std::path::PathBuf;

use crate::agent::memory::MemoryStore;
use crate::agent::skills::SkillsLoader;
use crate::providers::traits::{ChatMessage, Role, ToolCallMessage};

/// Files loaded from the workspace root to bootstrap the agent's context.
const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

/// Builds the context (system prompt + messages) for the agent.
///
/// Assembles bootstrap files, memory, skills, and conversation history
/// into a coherent prompt for the LLM.
pub struct ContextBuilder {
    workspace: PathBuf,
    memory: MemoryStore,
    skills: SkillsLoader,
}

impl ContextBuilder {
    /// Create a new ContextBuilder for the given workspace.
    pub fn new(workspace: PathBuf) -> Self {
        let memory = MemoryStore::new(workspace.clone());
        let skills = SkillsLoader::new(workspace.clone(), None);
        Self {
            workspace,
            memory,
            skills,
        }
    }

    /// Create a new ContextBuilder with explicit skills loader.
    pub fn with_skills(workspace: PathBuf, builtin_skills: Option<PathBuf>) -> Self {
        let memory = MemoryStore::new(workspace.clone());
        let skills = SkillsLoader::new(workspace.clone(), builtin_skills);
        Self {
            workspace,
            memory,
            skills,
        }
    }

    /// Build the complete system prompt from identity, bootstrap files, memory, and skills.
    ///
    /// Parts are joined with `"\n\n---\n\n"`.
    pub fn build_system_prompt(&self) -> String {
        self.build_system_prompt_with_skills(None)
    }

    /// Build the system prompt, optionally including specific skills.
    pub fn build_system_prompt_with_skills(&self, _skill_names: Option<&[String]>) -> String {
        let mut parts = Vec::new();

        // Core identity
        parts.push(self.get_identity());

        // Bootstrap files
        let bootstrap = self.load_bootstrap_files();
        if !bootstrap.is_empty() {
            parts.push(bootstrap);
        }

        // Memory context
        let memory = self.memory.get_memory_context();
        if !memory.is_empty() {
            parts.push(format!("# Memory\n\n{}", memory));
        }

        // Skills - progressive loading
        // 1. Always-loaded skills: include full content
        let always_skills = self.skills.get_always_skills();
        if !always_skills.is_empty() {
            let always_content = self.skills.load_skills_for_context(&always_skills);
            if !always_content.is_empty() {
                parts.push(format!("# Active Skills\n\n{}", always_content));
            }
        }

        // 2. Available skills: only show summary (agent uses read_file to load)
        let skills_summary = self.skills.build_skills_summary();
        if !skills_summary.is_empty() {
            parts.push(format!(
                "# Skills\n\n\
                 The following skills extend your capabilities. \
                 To use a skill, read its SKILL.md file using the read_file tool.\n\
                 Skills with available=\"false\" need dependencies installed first \
                 - you can try installing them with apt/brew.\n\n\
                 {}",
                skills_summary
            ));
        }

        parts.join("\n\n---\n\n")
    }

    /// Build the complete message list for an LLM call.
    ///
    /// Returns a `Vec<ChatMessage>` containing:
    /// 1. System message with the assembled system prompt
    /// 2. History messages
    /// 3. Current user message
    pub fn build_messages(
        &self,
        history: &[ChatMessage],
        current_message: &str,
        skill_names: Option<&[String]>,
        channel: Option<&str>,
        chat_id: Option<&str>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // System prompt
        let mut system_prompt = self.build_system_prompt_with_skills(skill_names);
        if let (Some(ch), Some(cid)) = (channel, chat_id) {
            system_prompt.push_str(&format!(
                "\n\n## Current Session\nChannel: {}\nChat ID: {}",
                ch, cid
            ));
        }
        messages.push(ChatMessage {
            role: Role::System,
            content: Some(serde_json::Value::String(system_prompt)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        // History
        messages.extend(history.iter().cloned());

        // Current user message
        messages.push(ChatMessage {
            role: Role::User,
            content: Some(serde_json::Value::String(current_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        messages
    }

    /// Append a tool result message to the message list.
    pub fn add_tool_result(
        messages: &mut Vec<ChatMessage>,
        tool_call_id: &str,
        tool_name: &str,
        result: &str,
    ) {
        messages.push(ChatMessage {
            role: Role::Tool,
            content: Some(serde_json::Value::String(result.to_string())),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            name: Some(tool_name.to_string()),
        });
    }

    /// Append an assistant message to the message list.
    pub fn add_assistant_message(
        messages: &mut Vec<ChatMessage>,
        content: Option<&str>,
        tool_calls: Option<Vec<ToolCallMessage>>,
    ) {
        messages.push(ChatMessage {
            role: Role::Assistant,
            content: content.map(|c| serde_json::Value::String(c.to_string())),
            tool_calls,
            tool_call_id: None,
            name: None,
        });
    }

    /// Get the core identity section for the system prompt.
    fn get_identity(&self) -> String {
        let now = chrono::Local::now();
        let time_str = now.format("%Y-%m-%d %H:%M (%A)").to_string();
        let tz = now.format("%Z").to_string();
        let workspace_path = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone())
            .display()
            .to_string();

        let system = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        let os_display = match system {
            "macos" => "macOS",
            other => other,
        };
        let runtime = format!("{} {}, Rust", os_display, arch);

        format!(
            "# nanobot\n\n\
             You are nanobot, a helpful AI assistant.\n\n\
             ## Current Time\n\
             {} ({})\n\n\
             ## Runtime\n\
             {}\n\n\
             ## Workspace\n\
             Your workspace is at: {}\n\
             - Long-term memory: {}/memory/MEMORY.md\n\
             - History log: {}/memory/HISTORY.md (grep-searchable)\n\
             - Custom skills: {}/skills/{{skill-name}}/SKILL.md\n\n\
             Reply directly with text for conversations. \
             Only use the 'message' tool to send to a specific chat channel.\n\n\
             ## Tool Call Guidelines\n\
             - Before calling tools, you may briefly state your intent \
             (e.g. \"Let me check that\"), but NEVER predict or describe the expected result before receiving it.\n\
             - Before modifying a file, read it first to confirm its current content.\n\
             - Do not assume a file or directory exists -- use list_dir or read_file to verify.\n\
             - After writing or editing a file, re-read it if accuracy matters.\n\
             - If a tool call fails, analyze the error before retrying with a different approach.\n\n\
             ## Memory\n\
             - Remember important facts: write to {}/memory/MEMORY.md\n\
             - Recall past events: grep {}/memory/HISTORY.md",
            time_str,
            tz,
            runtime,
            workspace_path,
            workspace_path,
            workspace_path,
            workspace_path,
            workspace_path,
            workspace_path,
        )
    }

    /// Load all bootstrap files from the workspace root.
    fn load_bootstrap_files(&self) -> String {
        let mut parts = Vec::new();

        for filename in BOOTSTRAP_FILES {
            let file_path = self.workspace.join(filename);
            if file_path.exists() {
                if let Ok(content) = fs::read_to_string(&file_path) {
                    parts.push(format!("## {}\n\n{}", filename, content));
                }
            }
        }

        parts.join("\n\n")
    }

    /// Get a reference to the memory store.
    pub fn memory(&self) -> &MemoryStore {
        &self.memory
    }

    /// Get a mutable reference to the memory store.
    pub fn memory_mut(&mut self) -> &mut MemoryStore {
        &mut self.memory
    }

    /// Get a reference to the skills loader.
    pub fn skills(&self) -> &SkillsLoader {
        &self.skills
    }

    /// Get the workspace path.
    pub fn workspace(&self) -> &PathBuf {
        &self.workspace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_build_system_prompt_includes_identity() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("nanobot"));
        assert!(prompt.contains("Workspace"));
    }

    #[test]
    fn test_build_messages_structure() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let messages = ctx.build_messages(&[], "hello", None, Some("cli"), Some("direct"));
        assert_eq!(messages.len(), 2); // system + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn test_bootstrap_files_loaded() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("SOUL.md"), "Be kind").unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Be kind"));
    }

    #[test]
    fn test_build_messages_with_history() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let history = vec![
            ChatMessage {
                role: Role::User,
                content: Some(serde_json::Value::String("previous question".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: Role::Assistant,
                content: Some(serde_json::Value::String("previous answer".to_string())),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let messages = ctx.build_messages(&history, "new question", None, None, None);
        assert_eq!(messages.len(), 4); // system + 2 history + user
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
        assert_eq!(messages[3].role, Role::User);
    }

    #[test]
    fn test_build_messages_session_info() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let messages = ctx.build_messages(&[], "hi", None, Some("telegram"), Some("123"));
        let system_content = messages[0]
            .content
            .as_ref()
            .unwrap()
            .as_str()
            .unwrap();
        assert!(system_content.contains("Channel: telegram"));
        assert!(system_content.contains("Chat ID: 123"));
    }

    #[test]
    fn test_build_messages_no_session_info() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let messages = ctx.build_messages(&[], "hi", None, None, None);
        let system_content = messages[0]
            .content
            .as_ref()
            .unwrap()
            .as_str()
            .unwrap();
        assert!(!system_content.contains("Current Session"));
    }

    #[test]
    fn test_add_tool_result() {
        let mut messages = Vec::new();
        ContextBuilder::add_tool_result(&mut messages, "call_1", "read_file", "file contents");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::Tool);
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(messages[0].name.as_deref(), Some("read_file"));
    }

    #[test]
    fn test_add_assistant_message() {
        let mut messages = Vec::new();
        ContextBuilder::add_assistant_message(&mut messages, Some("hello"), None);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::Assistant);
        assert_eq!(
            messages[0].content.as_ref().unwrap().as_str().unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_add_assistant_message_with_tool_calls() {
        let mut messages = Vec::new();
        let tool_calls = vec![ToolCallMessage {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: crate::providers::traits::ToolCallFunction {
                name: "read_file".to_string(),
                arguments: r#"{"path": "/tmp/test"}"#.to_string(),
            },
        }];
        ContextBuilder::add_assistant_message(&mut messages, None, Some(tool_calls));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, Role::Assistant);
        assert!(messages[0].tool_calls.is_some());
    }

    #[test]
    fn test_multiple_bootstrap_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("SOUL.md"), "Be kind and helpful").unwrap();
        std::fs::write(dir.path().join("USER.md"), "User prefers Rust").unwrap();
        std::fs::write(dir.path().join("TOOLS.md"), "Use tools wisely").unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Be kind and helpful"));
        assert!(prompt.contains("User prefers Rust"));
        assert!(prompt.contains("Use tools wisely"));
    }

    #[test]
    fn test_memory_included_in_prompt() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        ctx.memory().write_long_term("User likes dark mode");
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("User likes dark mode"));
        assert!(prompt.contains("Memory"));
    }

    #[test]
    fn test_identity_contains_time() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Current Time"));
    }

    #[test]
    fn test_identity_contains_tool_guidelines() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Tool Call Guidelines"));
    }

    #[test]
    fn test_user_message_content() {
        let dir = TempDir::new().unwrap();
        let ctx = ContextBuilder::new(dir.path().to_path_buf());
        let messages = ctx.build_messages(&[], "What is Rust?", None, None, None);
        let user_content = messages[1]
            .content
            .as_ref()
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(user_content, "What is Rust?");
    }
}
