use crate::errors::AgentResult;
use crate::events::EventBus;
use crate::sessions::{SessionEvent, SessionEventType, SessionService};
use std::sync::Arc;

/// EventProcessor handles the business logic of event processing
/// while keeping SessionService as a pure persistence layer.
///
/// This provides clean separation of concerns:
/// - SessionService: Pure persistence (store/retrieve events)
/// - EventProcessor: Business logic (indexing, side effects, streaming)
pub struct EventProcessor {
    /// Pure persistence layer - can be InMemory, Database, etc.
    session_service: Arc<dyn SessionService>,

    /// Event streaming for real-time subscriptions
    event_bus: Arc<dyn EventBus>,
}

impl EventProcessor {
    pub fn new(session_service: Arc<dyn SessionService>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            session_service,
            event_bus,
        }
    }

    /// Get access to the event bus for subscriptions
    pub fn event_bus(&self) -> &Arc<dyn EventBus> {
        &self.event_bus
    }

    /// Process an event with full business logic
    /// This replaces the complex logic that was in InMemorySessionService
    pub async fn process_event(&self, event: SessionEvent) -> AgentResult<()> {
        // 1. Store event in persistence layer (includes indexing)
        self.session_service.store_event(&event).await?;

        // 2. Apply side effects (state changes)
        self.apply_event_side_effects(&event).await?;

        // 3. Stream to subscribers via EventBus
        self.event_bus.publish(event).await?;

        Ok(())
    }

    /// Apply side effects like updating app/user/session state
    async fn apply_event_side_effects(&self, event: &SessionEvent) -> AgentResult<()> {
        match &event.event_type {
            SessionEventType::StateChanged { scope, .. } => {
                // Delegate state changes to SessionService
                match scope {
                    crate::sessions::StateScope::App => {
                        // Extract app_name from session (this requires a session lookup)
                        // For now, we'll let the SessionService handle this internally
                        self.session_service.apply_state_change(event).await?;
                    }
                    crate::sessions::StateScope::User => {
                        self.session_service.apply_state_change(event).await?;
                    }
                    crate::sessions::StateScope::Session => {
                        self.session_service.apply_state_change(event).await?;
                    }
                }
            }
            _ => {
                // Other events don't have side effects
            }
        }
        Ok(())
    }

    // ===== Business Logic Methods =====
    // These methods handle task reconstruction from events - moved from SessionService

    /// Get task by ID using event sourcing (A2A Protocol: tasks/get)
    pub async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<crate::a2a::Task>> {
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
    ) -> AgentResult<Vec<crate::a2a::Task>> {
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
    pub async fn get_llm_conversation(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<Vec<crate::models::content::Content>> {
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

    /// Rebuild a Task object from events (business logic)
    async fn rebuild_task_from_events(
        &self,
        events: &[SessionEvent],
        task_id: &str,
    ) -> AgentResult<Option<crate::a2a::Task>> {
        let mut current_task: Option<crate::a2a::Task> = None;

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
