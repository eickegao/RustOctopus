pub mod manager;

pub use manager::{Session, SessionManager};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_add_message() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "hello");
        session.add_message("assistant", "hi there");
        assert_eq!(session.messages.len(), 2);
    }

    #[test]
    fn test_session_get_history_trims_leading_non_user() {
        let mut session = Session::new("test:chat");
        session.add_message("assistant", "orphan");
        session.add_message("user", "hello");
        session.add_message("assistant", "hi");
        let history = session.get_history(100);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["role"], "user");
    }

    #[test]
    fn test_manager_save_and_load() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let mut session = manager.get_or_create("test:chat");
        session.add_message("user", "hello");
        manager.save(&session).unwrap();

        // Create new manager (simulates restart)
        let manager2 = SessionManager::new(dir.path().to_path_buf());
        let loaded = manager2.get_or_create("test:chat");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0]["content"], "hello");
    }

    #[test]
    fn test_session_clear() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "hello");
        session.clear();
        assert_eq!(session.messages.len(), 0);
        assert_eq!(session.last_consolidated, 0);
    }

    #[test]
    fn test_session_add_message_with_extras() {
        let mut session = Session::new("test:chat");
        let extras = serde_json::json!({
            "tool_calls": [{"id": "call_1", "function": {"name": "read_file"}}],
        });
        session.add_message_with_extras("assistant", "Let me read that.", Some(extras));
        assert_eq!(session.messages.len(), 1);
        assert!(session.messages[0].get("tool_calls").is_some());
    }

    #[test]
    fn test_get_history_respects_last_consolidated() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "msg1");
        session.add_message("assistant", "reply1");
        session.add_message("user", "msg2");
        session.add_message("assistant", "reply2");
        session.last_consolidated = 2;
        let history = session.get_history(100);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["content"], "msg2");
    }

    #[test]
    fn test_get_history_respects_max_messages() {
        let mut session = Session::new("test:chat");
        for i in 0..10 {
            session.add_message("user", &format!("msg{}", i));
            session.add_message("assistant", &format!("reply{}", i));
        }
        let history = session.get_history(4);
        assert_eq!(history.len(), 4);
    }

    #[test]
    fn test_get_history_strips_timestamp() {
        let mut session = Session::new("test:chat");
        session.add_message("user", "hello");
        let history = session.get_history(100);
        assert!(history[0].get("timestamp").is_none());
    }

    #[test]
    fn test_manager_caching() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let mut session = manager.get_or_create("test:chat");
        session.add_message("user", "hello");
        manager.save(&session).unwrap();

        // get_or_create should return the cached version
        let cached = manager.get_or_create("test:chat");
        assert_eq!(cached.messages.len(), 1);
    }

    #[test]
    fn test_manager_invalidate_cache() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let session = manager.get_or_create("test:chat");
        manager.save(&session).unwrap();

        manager.invalidate("test:chat");
        // After invalidation, should reload from disk
        let reloaded = manager.get_or_create("test:chat");
        assert_eq!(reloaded.key, "test:chat");
    }

    #[test]
    fn test_manager_list_sessions() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());

        let session1 = Session::new("telegram:123");
        manager.save(&session1).unwrap();

        let session2 = Session::new("discord:456");
        manager.save(&session2).unwrap();

        let sessions = manager.list_sessions();
        assert_eq!(sessions.len(), 2);

        let keys: Vec<&str> = sessions
            .iter()
            .filter_map(|s| s.get("key").and_then(|k| k.as_str()))
            .collect();
        assert!(keys.contains(&"telegram:123"));
        assert!(keys.contains(&"discord:456"));
    }

    #[test]
    fn test_session_file_naming() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let session = Session::new("telegram:123");
        manager.save(&session).unwrap();

        let expected_path = dir.path().join("telegram_123.jsonl");
        assert!(expected_path.exists());
    }

    #[test]
    fn test_jsonl_format() {
        let dir = TempDir::new().unwrap();
        let manager = SessionManager::new(dir.path().to_path_buf());
        let mut session = Session::new("test:fmt");
        session.add_message("user", "hello");
        session.add_message("assistant", "hi");
        manager.save(&session).unwrap();

        let content = std::fs::read_to_string(dir.path().join("test_fmt.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3); // metadata + 2 messages

        let meta: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(meta["_type"], "metadata");
        assert_eq!(meta["key"], "test:fmt");

        let msg1: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(msg1["role"], "user");
        assert_eq!(msg1["content"], "hello");

        let msg2: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(msg2["role"], "assistant");
        assert_eq!(msg2["content"], "hi");
    }
}
