//! Dual-layer memory system: MEMORY.md (long-term facts) + HISTORY.md (grep-searchable log).
//!
//! The memory store lives in a `memory/` subdirectory of the workspace and provides
//! two persistence layers:
//!
//! - **MEMORY.md** -- long-term facts, overwritten by the LLM during consolidation
//! - **HISTORY.md** -- append-only timestamped log, designed for grep search

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tracing::{info, warn};

use crate::providers::traits::{
    ChatMessage, ChatParams, FunctionDef, LlmProvider, Role, ToolDefinition,
};
use crate::session::Session;

/// Build the save_memory tool definition used during consolidation.
fn save_memory_tool() -> ToolDefinition {
    ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDef {
            name: "save_memory".to_string(),
            description: "Save the memory consolidation result to persistent storage.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "history_entry": {
                        "type": "string",
                        "description": "A paragraph (2-5 sentences) summarizing key events/decisions/topics. Start with [YYYY-MM-DD HH:MM]. Include detail useful for grep search."
                    },
                    "memory_update": {
                        "type": "string",
                        "description": "Full updated long-term memory as markdown. Include all existing facts plus new ones. Return unchanged if nothing new."
                    }
                },
                "required": ["history_entry", "memory_update"]
            }),
        },
    }
}

/// Two-layer memory: MEMORY.md (long-term facts) + HISTORY.md (grep-searchable log).
pub struct MemoryStore {
    memory_file: PathBuf,
    history_file_path: PathBuf,
}

impl MemoryStore {
    /// Create a new MemoryStore rooted at `workspace/memory/`.
    ///
    /// Creates the `memory/` directory if it does not exist.
    pub fn new(workspace: PathBuf) -> Self {
        let memory_dir = workspace.join("memory");
        if let Err(e) = fs::create_dir_all(&memory_dir) {
            warn!("Failed to create memory directory {:?}: {}", memory_dir, e);
        }
        Self {
            memory_file: memory_dir.join("MEMORY.md"),
            history_file_path: memory_dir.join("HISTORY.md"),
        }
    }

    /// Read MEMORY.md contents. Returns empty string if the file does not exist.
    pub fn read_long_term(&self) -> String {
        fs::read_to_string(&self.memory_file).unwrap_or_default()
    }

    /// Overwrite MEMORY.md with `content`.
    pub fn write_long_term(&self, content: &str) {
        if let Err(e) = fs::write(&self.memory_file, content) {
            warn!("Failed to write MEMORY.md: {}", e);
        }
    }

    /// Append an entry to HISTORY.md (followed by two newlines).
    pub fn append_history(&self, entry: &str) {
        use std::fs::OpenOptions;
        use std::io::Write;

        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_file_path)
            .and_then(|mut f| write!(f, "{}\n\n", entry.trim_end()));

        if let Err(e) = result {
            warn!("Failed to append to HISTORY.md: {}", e);
        }
    }

    /// Return formatted memory context for the system prompt.
    ///
    /// Returns an empty string if there is no long-term memory, otherwise
    /// returns `"## Long-term Memory\n{content}"`.
    pub fn get_memory_context(&self) -> String {
        let long_term = self.read_long_term();
        if long_term.is_empty() {
            String::new()
        } else {
            format!("## Long-term Memory\n{}", long_term)
        }
    }

    /// Return the path to HISTORY.md (useful for tests and external tools).
    pub fn history_file(&self) -> &Path {
        &self.history_file_path
    }

    /// Consolidate old messages into MEMORY.md + HISTORY.md via an LLM tool call.
    ///
    /// When `archive_all` is true, all messages are processed and `keep_count` is 0.
    /// Otherwise, keep the last `memory_window / 2` messages and consolidate earlier ones.
    ///
    /// Returns `true` on success (including no-op when there is nothing to consolidate),
    /// `false` on failure.
    pub async fn consolidate(
        &self,
        session: &mut Session,
        provider: &dyn LlmProvider,
        model: &str,
        archive_all: bool,
        memory_window: usize,
    ) -> bool {
        let keep_count;
        let old_messages: Vec<serde_json::Value>;

        if archive_all {
            old_messages = session.messages.clone();
            keep_count = 0;
            info!(
                "Memory consolidation (archive_all): {} messages",
                session.messages.len()
            );
        } else {
            keep_count = memory_window / 2;
            if session.messages.len() <= keep_count {
                return true;
            }
            let last_consolidated = session.last_consolidated;
            let end = session.messages.len().saturating_sub(keep_count);
            if end <= last_consolidated {
                return true;
            }
            old_messages = session.messages[last_consolidated..end].to_vec();
            if old_messages.is_empty() {
                return true;
            }
            info!(
                "Memory consolidation: {} to consolidate, {} keep",
                old_messages.len(),
                keep_count
            );
        }

        // Format messages for the consolidation prompt
        let mut lines = Vec::new();
        for m in &old_messages {
            let content = m
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            if content.is_empty() {
                continue;
            }
            let timestamp = m
                .get("timestamp")
                .and_then(|t| t.as_str())
                .unwrap_or("?");
            // Truncate timestamp to 16 chars (YYYY-MM-DDTHH:MM)
            let ts_short = &timestamp[..timestamp.len().min(16)];
            let role = m
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_uppercase();
            let tools_str = if let Some(tools) = m.get("tools_used").and_then(|t| t.as_array()) {
                let names: Vec<&str> = tools.iter().filter_map(|t| t.as_str()).collect();
                if names.is_empty() {
                    String::new()
                } else {
                    format!(" [tools: {}]", names.join(", "))
                }
            } else {
                String::new()
            };
            lines.push(format!("[{}] {}{}: {}", ts_short, role, tools_str, content));
        }

        let current_memory = self.read_long_term();
        let memory_display = if current_memory.is_empty() {
            "(empty)".to_string()
        } else {
            current_memory.clone()
        };

        let prompt = format!(
            "Process this conversation and call the save_memory tool with your consolidation.\n\n\
             ## Current Long-term Memory\n\
             {}\n\n\
             ## Conversation to Process\n\
             {}",
            memory_display,
            lines.join("\n")
        );

        let system_msg = ChatMessage {
            role: Role::System,
            content: Some(serde_json::Value::String(
                "You are a memory consolidation agent. Call the save_memory tool with your consolidation of the conversation.".to_string(),
            )),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };

        let user_msg = ChatMessage {
            role: Role::User,
            content: Some(serde_json::Value::String(prompt)),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };

        let messages = vec![system_msg, user_msg];
        let tools = vec![save_memory_tool()];
        let params = ChatParams {
            max_tokens: 4096,
            temperature: 0.0,
        };

        match provider.chat(&messages, Some(&tools), model, &params).await {
            Ok(response) => {
                if !response.has_tool_calls() {
                    warn!("Memory consolidation: LLM did not call save_memory, skipping");
                    return false;
                }

                let args = &response.tool_calls[0].arguments;

                if let Some(entry) = args.get("history_entry") {
                    let entry_str = match entry.as_str() {
                        Some(s) => s.to_string(),
                        None => serde_json::to_string(entry).unwrap_or_default(),
                    };
                    if !entry_str.is_empty() {
                        self.append_history(&entry_str);
                    }
                }

                if let Some(update) = args.get("memory_update") {
                    let update_str = match update.as_str() {
                        Some(s) => s.to_string(),
                        None => serde_json::to_string(update).unwrap_or_default(),
                    };
                    if !update_str.is_empty() && update_str != current_memory {
                        self.write_long_term(&update_str);
                    }
                }

                session.last_consolidated = if archive_all {
                    0
                } else {
                    session.messages.len() - keep_count
                };

                info!(
                    "Memory consolidation done: {} messages, last_consolidated={}",
                    session.messages.len(),
                    session.last_consolidated
                );
                true
            }
            Err(e) => {
                warn!("Memory consolidation failed: {}", e);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_write_long_term() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        assert_eq!(store.read_long_term(), "");
        store.write_long_term("User prefers dark mode");
        assert_eq!(store.read_long_term(), "User prefers dark mode");
    }

    #[test]
    fn test_append_history() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        store.append_history("[2026-02-24] User asked about Rust");
        store.append_history("[2026-02-24] Discussed architecture");
        let content = std::fs::read_to_string(store.history_file()).unwrap();
        assert!(content.contains("Rust"));
        assert!(content.contains("architecture"));
    }

    #[test]
    fn test_memory_context() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        assert_eq!(store.get_memory_context(), "");
        store.write_long_term("Some facts");
        assert!(store.get_memory_context().contains("Long-term Memory"));
    }

    #[test]
    fn test_memory_dir_created() {
        let dir = TempDir::new().unwrap();
        let _store = MemoryStore::new(dir.path().to_path_buf());
        assert!(dir.path().join("memory").is_dir());
    }

    #[test]
    fn test_history_file_path() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        assert_eq!(store.history_file(), dir.path().join("memory").join("HISTORY.md"));
    }

    #[test]
    fn test_append_history_multiple_entries() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        store.append_history("First entry");
        store.append_history("Second entry");
        store.append_history("Third entry");
        let content = std::fs::read_to_string(store.history_file()).unwrap();
        // Each entry should be separated by double newlines
        assert!(content.contains("First entry\n\n"));
        assert!(content.contains("Second entry\n\n"));
        assert!(content.contains("Third entry\n\n"));
    }

    #[test]
    fn test_write_long_term_overwrites() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        store.write_long_term("Version 1");
        assert_eq!(store.read_long_term(), "Version 1");
        store.write_long_term("Version 2");
        assert_eq!(store.read_long_term(), "Version 2");
    }

    #[test]
    fn test_memory_context_format() {
        let dir = TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf());
        store.write_long_term("User likes Rust");
        let ctx = store.get_memory_context();
        assert_eq!(ctx, "## Long-term Memory\nUser likes Rust");
    }

    #[test]
    fn test_save_memory_tool_definition() {
        let tool = save_memory_tool();
        assert_eq!(tool.def_type, "function");
        assert_eq!(tool.function.name, "save_memory");
        let params = &tool.function.parameters;
        let required = params.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 2);
    }
}
