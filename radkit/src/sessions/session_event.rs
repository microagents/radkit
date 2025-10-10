//! Unified Session Event Types
//!
//! This module defines the unified SessionEvent system that will replace the dual
//! InternalEvent + A2A event architecture. This is Phase 1 of the migration plan.

use crate::models::content::Content;
use a2a_types::{Artifact, Task, TaskState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Unified session event type - single event stream for all session activities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// Unique event identifier
    pub id: String,
    /// When this event occurred
    pub timestamp: DateTime<Utc>,
    /// Session this event belongs to (maps to A2A contextId)
    pub session_id: String,
    /// Task this event belongs to
    pub task_id: String,
    /// The actual event data
    pub event_type: SessionEventType,
}

/// Different types of events that can occur in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventType {
    /// User sent a message (includes potential function responses)
    UserMessage { content: Content },

    /// Agent sent a message (includes potential function calls)
    AgentMessage { content: Content },

    /// Task status changed (submitted -> working -> completed/failed, etc.)
    TaskStatusUpdate {
        new_state: TaskState,
        message: Option<String>,
        task: Option<Task>,
    },

    /// Artifact was saved
    TaskArtifactUpdate { artifact: Artifact },

    /// State was changed at app/user/session level
    StateChanged {
        scope: StateScope,
        key: String,
        old_value: Option<Value>,
        new_value: Value,
    },
}

/// Scope of state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateScope {
    /// Application-level state
    App,
    /// User-level state within an app
    User,
    /// Session-level state
    Session,
}

impl SessionEvent {
    /// Create a new session event with auto-generated ID
    pub fn new(session_id: String, task_id: String, event_type: SessionEventType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            session_id,
            task_id,
            event_type,
        }
    }

    /// Check if this event is A2A protocol compatible (should be streamed)
    pub fn is_a2a_compatible(&self) -> bool {
        matches!(
            self.event_type,
            SessionEventType::AgentMessage { .. }
                | SessionEventType::TaskStatusUpdate { .. }
                | SessionEventType::TaskArtifactUpdate { .. }
        )
    }

    /// Convert to A2A streaming message result (if compatible)
    pub fn to_a2a_streaming_result(&self) -> Option<a2a_types::SendStreamingMessageResult> {
        use a2a_types::{
            Message, MessageRole, Part, SendStreamingMessageResult, TaskArtifactUpdateEvent,
            TaskStatus, TaskStatusUpdateEvent,
        };

        match &self.event_type {
            SessionEventType::AgentMessage { content } => {
                // Function calls filtered out by to_a2a_message()
                let message = content.to_a2a_message();
                if message.is_none() {
                    return None;
                }
                Some(SendStreamingMessageResult::Message(message.unwrap()))
            }

            SessionEventType::TaskStatusUpdate {
                new_state, message, ..
            } => {
                let task_status = TaskStatus {
                    state: new_state.clone(),
                    timestamp: Some(self.timestamp.to_rfc3339()),
                    message: message.as_ref().map(|msg| Message {
                        kind: "message".to_string(),
                        message_id: uuid::Uuid::new_v4().to_string(),
                        role: MessageRole::Agent,
                        parts: vec![Part::Text {
                            text: msg.clone(),
                            metadata: None,
                        }],
                        context_id: Some(self.session_id.clone()),
                        task_id: Some(self.task_id.clone()),
                        reference_task_ids: Vec::new(),
                        extensions: Vec::new(),
                        metadata: None,
                    }),
                };

                Some(SendStreamingMessageResult::TaskStatusUpdate(
                    TaskStatusUpdateEvent {
                        kind: "status-update".to_string(),
                        task_id: self.task_id.clone(),
                        context_id: self.session_id.clone(),
                        status: task_status,
                        is_final: matches!(
                            new_state,
                            TaskState::Completed
                                | TaskState::Failed
                                | TaskState::Rejected
                                | TaskState::Canceled
                        ),
                        metadata: None,
                    },
                ))
            }

            SessionEventType::TaskArtifactUpdate { artifact } => Some(
                SendStreamingMessageResult::TaskArtifactUpdate(TaskArtifactUpdateEvent {
                    kind: "artifact-update".to_string(),
                    task_id: self.task_id.clone(),
                    context_id: self.session_id.clone(),
                    artifact: artifact.clone(),
                    append: None,
                    last_chunk: Some(true),
                    metadata: None,
                }),
            ),

            // Other events don't map to A2A streaming results
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::content::{Content, ContentPart};
    use a2a_types::MessageRole;

    #[test]
    fn test_session_event_creation() {
        let content = Content::new(
            "task1".to_string(),
            "session1".to_string(),
            "msg1".to_string(),
            MessageRole::User,
        );

        let event = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::AgentMessage { content },
        );

        assert_eq!(event.session_id, "session1");
        assert_eq!(event.task_id, "task1");
        assert!(!event.id.is_empty());
        assert!(matches!(
            event.event_type,
            SessionEventType::AgentMessage { .. }
        ));
    }

    #[test]
    fn test_a2a_compatibility() {
        let user_message = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::AgentMessage {
                content: Content::new(
                    "task1".to_string(),
                    "session1".to_string(),
                    "msg1".to_string(),
                    MessageRole::Agent,
                ),
            },
        );

        let state_change = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::StateChanged {
                scope: StateScope::Session,
                key: "theme".to_string(),
                old_value: None,
                new_value: serde_json::json!("dark"),
            },
        );

        assert!(user_message.is_a2a_compatible());
        assert!(!state_change.is_a2a_compatible());
    }

    #[test]
    fn test_a2a_streaming_conversion() {
        let mut content = Content::new(
            "task1".to_string(),
            "session1".to_string(),
            "msg1".to_string(),
            MessageRole::Agent,
        );
        content.parts = vec![ContentPart::Text {
            text: "Agent message".to_string(),
            metadata: None,
        }];
        let user_message = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::AgentMessage { content },
        );

        let streaming_result = user_message.to_a2a_streaming_result();
        assert!(streaming_result.is_some());
        assert!(matches!(
            streaming_result.unwrap(),
            a2a_types::SendStreamingMessageResult::Message(_)
        ));

        let state_change = SessionEvent::new(
            "session1".to_string(),
            "task1".to_string(),
            SessionEventType::StateChanged {
                scope: StateScope::Session,
                key: "theme".to_string(),
                old_value: None,
                new_value: serde_json::json!("dark"),
            },
        );

        let streaming_result = state_change.to_a2a_streaming_result();
        assert!(streaming_result.is_none());
    }
}
