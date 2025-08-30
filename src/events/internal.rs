use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Import Content type for rich event data
use crate::models::content::Content;

/// Internal events for debugging, observability, and logging.
/// These events are NOT sent to A2A clients - they exist purely for internal tracking.
/// Now uses Content type for rich message representation including function calls/responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InternalEvent {
    /// Message event - unified representation of all conversation interactions
    /// Captures user inputs, model responses, function calls, and function results
    MessageReceived {
        /// The full content including text, function calls, and function responses
        content: Content,
        /// Additional metadata for the event
        metadata: EventMetadata,
        /// When this event occurred
        timestamp: DateTime<Utc>,
    },

    /// State was modified (app/user/session level)
    StateChange {
        context_id: Option<String>, // None for app-level changes
        app_name: String,
        user_id: Option<String>, // None for app-level changes
        scope: StateScope,       // App, User, or Session level
        key: String,
        old_value: Option<Value>,
        new_value: Value,
        timestamp: DateTime<Utc>,
    },
}

/// Metadata for conversation content events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Application name for multi-tenancy
    pub app_name: String,
    /// User ID
    pub user_id: String,
    /// Model-specific metadata (for model responses)
    pub model_info: Option<ModelInfo>,
    /// Performance metrics
    pub performance: Option<PerformanceMetrics>,
}

/// Model information for model-generated content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_name: String,
    pub prompt_tokens: Option<usize>,
    pub response_tokens: Option<usize>,
    pub cost_estimate: Option<f64>,
}

/// Performance metrics for any operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub duration_ms: u64,
    pub cache_hit: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateScope {
    App,
    User,
    Session,
}

impl InternalEvent {
    /// Get the context_id if this event is associated with a session
    /// This method is used in production code for event projection
    pub fn context_id(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { content, .. } => Some(&content.context_id),
            Self::StateChange { context_id, .. } => context_id.as_deref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::MessageRole;
    use crate::models::content::Content;
    use serde_json::json;

    #[test]
    fn test_context_id_extraction() {
        // Test MessageReceived event
        let content = Content::new(
            "task_123".to_string(),
            "ctx_456".to_string(),
            "msg_789".to_string(),
            MessageRole::User,
        );

        let event = InternalEvent::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name: "test_app".to_string(),
                user_id: "user_123".to_string(),
                model_info: None,
                performance: None,
            },
            timestamp: Utc::now(),
        };

        assert_eq!(event.context_id(), Some("ctx_456"));

        // Test StateChange event
        let state_event = InternalEvent::StateChange {
            context_id: Some("ctx_123".to_string()),
            app_name: "test_app".to_string(),
            user_id: Some("user_456".to_string()),
            scope: StateScope::Session,
            key: "preferred_language".to_string(),
            old_value: Some(json!("en")),
            new_value: json!("es"),
            timestamp: Utc::now(),
        };

        assert_eq!(state_event.context_id(), Some("ctx_123"));

        // Test StateChange event without context_id
        let app_state_event = InternalEvent::StateChange {
            context_id: None,
            app_name: "test_app".to_string(),
            user_id: None,
            scope: StateScope::App,
            key: "global_config".to_string(),
            old_value: None,
            new_value: json!("value"),
            timestamp: Utc::now(),
        };

        assert_eq!(app_state_event.context_id(), None);
    }
}
