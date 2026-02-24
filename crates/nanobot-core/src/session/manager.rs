//! Session management for conversation history.
//!
//! Sessions are stored as JSONL files: a metadata line followed by one JSON line per message.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde_json::{json, Value};
use tracing::{info, warn};

/// A conversation session.
///
/// Stores messages in JSONL format for easy reading and persistence.
/// Messages are append-only for LLM cache efficiency. The consolidation
/// process writes summaries to MEMORY.md/HISTORY.md but does NOT modify
/// the messages list or `get_history()` output.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session key (usually channel:chat_id).
    pub key: String,
    /// List of message objects (each has role, content, timestamp, optional tool_calls/tool_call_id/name).
    pub messages: Vec<Value>,
    /// When the session was created.
    pub created_at: DateTime<Local>,
    /// When the session was last updated.
    pub updated_at: DateTime<Local>,
    /// Arbitrary metadata associated with the session.
    pub metadata: Value,
    /// Number of messages already consolidated to files.
    pub last_consolidated: usize,
}

impl Session {
    /// Create a new empty session with the given key.
    pub fn new(key: &str) -> Self {
        let now = Local::now();
        Self {
            key: key.to_string(),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: json!({}),
            last_consolidated: 0,
        }
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.add_message_with_extras(role, content, None);
    }

    /// Add a message to the session with optional extra fields (tool_calls, tool_call_id, name).
    pub fn add_message_with_extras(
        &mut self,
        role: &str,
        content: &str,
        extras: Option<Value>,
    ) {
        let mut msg = json!({
            "role": role,
            "content": content,
            "timestamp": Local::now().to_rfc3339(),
        });

        if let Some(extras) = extras {
            if let (Some(msg_obj), Some(extras_obj)) = (msg.as_object_mut(), extras.as_object()) {
                for (k, v) in extras_obj {
                    msg_obj.insert(k.clone(), v.clone());
                }
            }
        }

        self.messages.push(msg);
        self.updated_at = Local::now();
    }

    /// Return unconsolidated messages for LLM input, aligned to a user turn.
    ///
    /// Skips already-consolidated messages, trims to `max_messages`, and drops
    /// leading non-user messages to avoid orphaned tool_result blocks.
    pub fn get_history(&self, max_messages: usize) -> Vec<Value> {
        let unconsolidated = &self.messages[self.last_consolidated..];

        // Take at most max_messages from the end
        let start = unconsolidated.len().saturating_sub(max_messages);
        let sliced = &unconsolidated[start..];

        // Find the first user message to avoid orphaned tool_result blocks
        let trim_start = sliced
            .iter()
            .position(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
            .unwrap_or(sliced.len());

        let trimmed = &sliced[trim_start..];

        // Build output with only LLM-relevant fields
        trimmed
            .iter()
            .map(|m| {
                let mut entry = json!({
                    "role": m.get("role").and_then(|r| r.as_str()).unwrap_or(""),
                    "content": m.get("content").and_then(|c| c.as_str()).unwrap_or(""),
                });
                for key in &["tool_calls", "tool_call_id", "name"] {
                    if let Some(val) = m.get(*key) {
                        entry
                            .as_object_mut()
                            .unwrap()
                            .insert(key.to_string(), val.clone());
                    }
                }
                entry
            })
            .collect()
    }

    /// Clear all messages and reset session to initial state.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.last_consolidated = 0;
        self.updated_at = Local::now();
    }
}

/// Manages conversation sessions with JSONL persistence and in-memory caching.
///
/// Sessions are stored as JSONL files in a directory. The first line is a
/// metadata object, followed by one JSON line per message.
pub struct SessionManager {
    sessions_dir: PathBuf,
    cache: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    /// Create a new SessionManager that stores sessions in the given directory.
    ///
    /// The directory is created if it does not exist.
    pub fn new(sessions_dir: PathBuf) -> Self {
        if let Err(e) = fs::create_dir_all(&sessions_dir) {
            warn!("Failed to create sessions directory {:?}: {}", sessions_dir, e);
        }
        Self {
            sessions_dir,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the file path for a session key.
    fn get_session_path(&self, key: &str) -> PathBuf {
        let safe_key = key.replace(':', "_");
        self.sessions_dir.join(format!("{}.jsonl", safe_key))
    }

    /// Get an existing session or create a new one.
    pub fn get_or_create(&self, key: &str) -> Session {
        {
            let cache = self.cache.lock().unwrap();
            if let Some(session) = cache.get(key) {
                return session.clone();
            }
        }

        let session = self.load(key).unwrap_or_else(|| Session::new(key));

        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(key.to_string(), session.clone());
        }

        session
    }

    /// Load a session from disk. Returns None if the file doesn't exist or is unreadable.
    fn load(&self, key: &str) -> Option<Session> {
        let path = self.get_session_path(key);
        if !path.exists() {
            return None;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read session file {:?}: {}", path, e);
                return None;
            }
        };

        let mut messages = Vec::new();
        let mut metadata = json!({});
        let mut created_at: Option<DateTime<Local>> = None;
        let mut last_consolidated: usize = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match serde_json::from_str::<Value>(line) {
                Ok(data) => {
                    if data.get("_type").and_then(|t| t.as_str()) == Some("metadata") {
                        metadata = data.get("metadata").cloned().unwrap_or(json!({}));
                        created_at = data
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Local));
                        last_consolidated = data
                            .get("last_consolidated")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                    } else {
                        messages.push(data);
                    }
                }
                Err(e) => {
                    warn!("Skipping invalid JSON line in {:?}: {}", path, e);
                }
            }
        }

        info!("Loaded session {} with {} messages", key, messages.len());

        Some(Session {
            key: key.to_string(),
            messages,
            created_at: created_at.unwrap_or_else(Local::now),
            updated_at: Local::now(),
            metadata,
            last_consolidated,
        })
    }

    /// Save a session to disk in JSONL format.
    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.get_session_path(&session.key);

        let mut lines = Vec::new();

        // Metadata line
        let metadata_line = json!({
            "_type": "metadata",
            "key": session.key,
            "created_at": session.created_at.to_rfc3339(),
            "updated_at": session.updated_at.to_rfc3339(),
            "metadata": session.metadata,
            "last_consolidated": session.last_consolidated,
        });
        lines.push(serde_json::to_string(&metadata_line)?);

        // Message lines
        for msg in &session.messages {
            lines.push(serde_json::to_string(msg)?);
        }

        let content = lines.join("\n") + "\n";
        fs::write(&path, content)
            .with_context(|| format!("Failed to write session file {:?}", path))?;

        // Update cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(session.key.clone(), session.clone());
        }

        Ok(())
    }

    /// Remove a session from the in-memory cache.
    pub fn invalidate(&self, key: &str) {
        let mut cache = self.cache.lock().unwrap();
        cache.remove(key);
    }

    /// List all sessions by reading just the metadata line from each JSONL file.
    pub fn list_sessions(&self) -> Vec<Value> {
        let mut sessions = Vec::new();

        let entries = match fs::read_dir(&self.sessions_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read sessions directory: {}", e);
                return sessions;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            match fs::read_to_string(&path) {
                Ok(content) => {
                    if let Some(first_line) = content.lines().next() {
                        let first_line = first_line.trim();
                        if !first_line.is_empty() {
                            if let Ok(data) = serde_json::from_str::<Value>(first_line) {
                                if data.get("_type").and_then(|t| t.as_str()) == Some("metadata") {
                                    let key = data
                                        .get("key")
                                        .and_then(|k| k.as_str())
                                        .unwrap_or_else(|| {
                                            path.file_stem()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or("")
                                        });
                                    sessions.push(json!({
                                        "key": key,
                                        "created_at": data.get("created_at"),
                                        "updated_at": data.get("updated_at"),
                                        "path": path.to_string_lossy(),
                                    }));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read session file {:?}: {}", path, e);
                }
            }
        }

        // Sort by updated_at descending
        sessions.sort_by(|a, b| {
            let a_updated = a
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let b_updated = b
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            b_updated.cmp(a_updated)
        });

        sessions
    }
}
