use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use super::session_event::SessionEvent;

/// Represents a user session for managing conversation state, context, and events.
/// Maps to A2A contextId for grouping related interactions.
/// Tasks are now managed separately via TaskManager.
#[derive(Debug, Clone)]
pub struct Session {
    /// Unique session identifier (maps to A2A contextId)
    pub id: String,
    /// Application name for multi-tenancy support
    pub app_name: String,
    /// User identifier
    pub user_id: String,
    /// Session-level state (only session-specific data, app/user state is merged in)
    pub state: HashMap<String, Value>,
    /// Session events for the unified system
    pub events: Vec<SessionEvent>,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
}

impl Session {
    /// Create a new session with auto-generated ID
    pub fn new(app_name: String, user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            app_name,
            user_id,
            state: HashMap::new(),
            events: Vec::new(),
            created_at: now,
            last_activity: now,
        }
    }

    /// Get state value (only session-level state, use merged session for app/user state)
    pub fn get_state(&self, key: &str) -> Option<&Value> {
        self.state.get(key)
    }

    /// Set session-level state value (for session-specific data only)
    pub fn set_state(&mut self, key: String, value: Value) {
        self.state.insert(key, value);
        self.last_activity = Utc::now();
    }

    /// Legacy compatibility methods
    pub fn remove_session_state(&mut self, key: &str) -> Option<Value> {
        self.last_activity = Utc::now();
        self.state.remove(key)
    }

    /// Add a session event to this session
    pub fn add_session_event(&mut self, event: SessionEvent) {
        self.events.push(event);
        self.last_activity = Utc::now();
    }

    /// Get all session events
    pub fn get_session_events(&self) -> &[SessionEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_session_new() {
        let session = Session::new("test_app".to_string(), "user123".to_string());
        assert!(!session.id.is_empty());
        assert_eq!(session.app_name, "test_app");
        assert_eq!(session.user_id, "user123");
        assert!(session.state.is_empty());
        assert!(session.events.is_empty());
    }

    #[test]
    fn test_session_state_management() {
        let mut session = Session::new("test_app".to_string(), "user123".to_string());
        let initial_activity = session.last_activity;

        // Wait a moment to ensure the timestamp changes
        thread::sleep(Duration::from_millis(10));

        // Set state
        session.set_state("key1".to_string(), json!("value1"));
        assert_eq!(session.get_state("key1"), Some(&json!("value1")));
        assert!(session.last_activity > initial_activity);

        let activity_after_set = session.last_activity;
        thread::sleep(Duration::from_millis(10));

        // Remove state
        let removed = session.remove_session_state("key1");
        assert_eq!(removed, Some(json!("value1")));
        assert_eq!(session.get_state("key1"), None);
        assert!(session.last_activity > activity_after_set);
    }

    #[test]
    fn test_session_event_management() {
        let mut session = Session::new("test_app".to_string(), "user123".to_string());
        let initial_activity = session.last_activity;
        thread::sleep(Duration::from_millis(10));

        // Add session event
        use crate::models::content::{Content, ContentPart};
        let test_content = Content {
            task_id: "task1".to_string(),
            context_id: session.id.clone(),
            message_id: "test_msg".to_string(),
            role: a2a_types::MessageRole::User,
            parts: vec![ContentPart::Text {
                text: "Test message".to_string(),
                metadata: None,
            }],
            metadata: None,
        };

        let event = SessionEvent::new(
            session.id.clone(),
            "task1".to_string(),
            crate::sessions::SessionEventType::UserMessage {
                content: test_content,
            },
        );

        session.add_session_event(event);
        assert_eq!(session.get_session_events().len(), 1);
        assert!(session.last_activity > initial_activity);
    }
}
