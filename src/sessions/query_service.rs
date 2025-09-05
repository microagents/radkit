//! QueryService - Read-only service for querying tasks, state, and conversations
//!
//! This service provides a clean interface for all read operations, handling business logic
//! for task reconstruction from events and state queries from sessions.

use crate::a2a::Task;
use crate::errors::AgentResult;
use crate::models::content::Content;
use crate::sessions::{SessionEvent, SessionEventType, SessionService, StateScope};
use serde_json::Value;
use std::sync::Arc;

/// Service for all read/query operations on sessions, tasks, and state
pub struct QueryService {
    session_service: Arc<dyn SessionService>,
}

impl QueryService {
    /// Create a new QueryService with a SessionService
    pub fn new(session_service: Arc<dyn SessionService>) -> Self {
        Self { session_service }
    }

    // ===== Task Query Methods =====

    /// Get task by ID using event sourcing (A2A Protocol: tasks/get)
    pub async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<Task>> {
        // Find which session contains this task (could be optimized with task_id -> session_id mapping)
        let sessions = self
            .session_service
            .list_sessions(app_name, user_id)
            .await?;

        for session in sessions {
            let events = self
                .session_service
                .get_session_events(app_name, user_id, &session.id)
                .await?;
            if let Some(task) = self.rebuild_task_from_events(&events, task_id).await? {
                return Ok(Some(task));
            }
        }

        Ok(None)
    }

    /// List tasks for app/user using event sourcing (A2A Protocol: tasks/list)
    pub async fn list_tasks(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<Task>> {
        let sessions = if let Some(ctx_id) = context_id {
            // Single session
            vec![
                self.session_service
                    .get_session(app_name, user_id, ctx_id)
                    .await?
                    .ok_or_else(|| crate::errors::AgentError::SessionNotFound {
                        session_id: ctx_id.to_string(),
                        app_name: app_name.to_string(),
                        user_id: user_id.to_string(),
                    })?,
            ]
        } else {
            // All sessions for this user
            self.session_service
                .list_sessions(app_name, user_id)
                .await?
        };

        let mut all_tasks = Vec::new();
        for session in sessions {
            let events = self
                .session_service
                .get_session_events(app_name, user_id, &session.id)
                .await?;
            let task_ids = self.extract_task_ids_from_events(&events);

            for task_id in task_ids {
                if let Some(task) = self.rebuild_task_from_events(&events, &task_id).await? {
                    all_tasks.push(task);
                }
            }
        }

        // Sort by creation time
        all_tasks.sort_by(|a, b| a.status.timestamp.cmp(&b.status.timestamp));
        Ok(all_tasks)
    }

    /// Get LLM conversation history from session events
    pub async fn get_conversation(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<Vec<Content>> {
        let events = self
            .session_service
            .get_session_events(app_name, user_id, session_id)
            .await?;

        let mut content_messages = Vec::new();
        for event in &events {
            match &event.event_type {
                SessionEventType::UserMessage { content }
                | SessionEventType::AgentMessage { content } => {
                    content_messages.push(content.clone());
                }
                _ => {} // Other events don't contribute to conversation history
            }
        }
        Ok(content_messages)
    }

    // ===== State Query Methods =====

    /// Get state value from session with specified scope
    pub async fn get_state(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
        scope: StateScope,
        key: &str,
    ) -> AgentResult<Option<Value>> {
        // Get the session which includes merged app/user/session state
        let session = self
            .session_service
            .get_session(app_name, user_id, session_id)
            .await?;

        if let Some(session) = session {
            let prefixed_key = match scope {
                StateScope::App => format!("app:{key}"),
                StateScope::User => format!("user:{key}"),
                StateScope::Session => key.to_string(),
            };
            Ok(session.get_state(&prefixed_key).cloned())
        } else {
            Ok(None)
        }
    }

    /// Get app-level state (convenience method)
    pub async fn get_app_state(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
        key: &str,
    ) -> AgentResult<Option<Value>> {
        self.get_state(app_name, user_id, session_id, StateScope::App, key)
            .await
    }

    /// Get user-level state (convenience method)
    pub async fn get_user_state(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
        key: &str,
    ) -> AgentResult<Option<Value>> {
        self.get_state(app_name, user_id, session_id, StateScope::User, key)
            .await
    }

    /// Get session-level state (convenience method)
    pub async fn get_session_state(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
        key: &str,
    ) -> AgentResult<Option<Value>> {
        self.get_state(app_name, user_id, session_id, StateScope::Session, key)
            .await
    }

    // ===== Private Helper Methods =====

    /// Rebuild a Task object from events (business logic)
    async fn rebuild_task_from_events(
        &self,
        events: &[SessionEvent],
        task_id: &str,
    ) -> AgentResult<Option<Task>> {
        let mut current_task: Option<Task> = None;

        for event in events {
            if event.task_id != task_id {
                continue;
            }

            match &event.event_type {
                SessionEventType::TaskCreated { task } => {
                    current_task = Some(task.clone());
                }
                SessionEventType::TaskStatusChanged { new_state, .. } => {
                    if let Some(ref mut task) = current_task {
                        task.status.state = new_state.clone();
                        task.status.timestamp = Some(event.timestamp.to_rfc3339());
                    }
                }
                SessionEventType::UserMessage { content }
                | SessionEventType::AgentMessage { content } => {
                    if let Some(ref mut task) = current_task {
                        // Function calls filtered out by to_a2a_message()
                        task.history.push(content.to_a2a_message());
                    }
                }
                SessionEventType::ArtifactSaved { artifact } => {
                    if let Some(ref mut task) = current_task {
                        task.artifacts.push(artifact.clone());
                    }
                }
                SessionEventType::TaskCompleted { task }
                | SessionEventType::TaskFailed { task, .. } => {
                    current_task = Some(task.clone());
                }
                _ => {} // Other events don't affect task state
            }
        }

        Ok(current_task)
    }

    /// Extract unique task IDs from session events
    fn extract_task_ids_from_events(&self, events: &[SessionEvent]) -> Vec<String> {
        let mut task_ids = std::collections::HashSet::new();
        for event in events {
            if let SessionEventType::TaskCreated { .. } = &event.event_type {
                task_ids.insert(event.task_id.clone());
            }
        }
        task_ids.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::sessions::InMemorySessionService;

    use std::sync::Arc;

    async fn create_test_query_service() -> QueryService {
        let session_service = Arc::new(InMemorySessionService::new());
        QueryService::new(session_service)
    }

    #[tokio::test]
    async fn test_query_service_empty_state() {
        let query_service = create_test_query_service().await;

        // Test querying non-existent state
        let result = query_service
            .get_state(
                "test_app",
                "test_user",
                "test_session",
                StateScope::User,
                "key",
            )
            .await
            .unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_query_service_no_tasks() {
        let query_service = create_test_query_service().await;

        // Test querying non-existent task
        let result = query_service
            .get_task("test_app", "test_user", "test_task")
            .await
            .unwrap();

        assert_eq!(result, None);

        // Test listing tasks with no sessions
        let tasks = query_service
            .list_tasks("test_app", "test_user", None)
            .await
            .unwrap();

        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_query_service_conversation_empty() {
        let query_service = create_test_query_service().await;

        // Test getting conversation from non-existent session should return error
        let result = query_service
            .get_conversation("test_app", "test_user", "test_session")
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::errors::AgentError::SessionNotFound { .. }
        ));
    }
}
