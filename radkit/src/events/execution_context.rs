use crate::errors::AgentResult;
use std::sync::Arc;

use crate::events::EventProcessor;
use crate::sessions::{QueryService, SessionEvent, SessionEventType};
use a2a_types::{Task, TaskState};

/// Execution context that handles event processing and delegates reads to QueryService
pub struct ExecutionContext {
    pub context_id: String,
    pub task_id: String,
    pub app_name: String,
    pub user_id: String,

    // Event processing for state changes and streaming
    event_processor: Arc<EventProcessor>,
    // Query service for all read operations
    query_service: Arc<QueryService>,
}

impl ExecutionContext {
    pub fn new(
        context_id: String,
        task_id: String,
        app_name: String,
        user_id: String,
        event_processor: Arc<EventProcessor>,
        query_service: Arc<QueryService>,
    ) -> Self {
        Self {
            context_id,
            task_id,
            app_name,
            user_id,
            event_processor,
            query_service,
        }
    }

    /// Emit user input event using unified SessionEvent
    pub async fn emit_user_input(
        &self,
        content: crate::models::content::Content,
    ) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::UserMessage { content },
        );
        self.event_processor.process_event(event).await
    }

    /// Emit message received event using unified SessionEvent
    pub async fn emit_message(&self, content: crate::models::content::Content) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::AgentMessage { content },
        );

        self.event_processor.process_event(event).await
    }

    /// Emit task status update event using unified SessionEvent
    pub async fn emit_task_status_update(
        &self,
        new_state: TaskState,
        message: Option<String>,
        task: Option<Task>,
    ) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::TaskStatusUpdate {
                new_state,
                message,
                task,
            },
        );
        self.event_processor.process_event(event).await
    }

    /// Emit state change event using unified SessionEvent
    pub async fn emit_state_change(
        &self,
        scope: crate::sessions::StateScope,
        key: String,
        old_value: Option<serde_json::Value>,
        new_value: serde_json::Value,
    ) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::StateChanged {
                scope,
                key,
                old_value,
                new_value,
            },
        );
        self.event_processor.process_event(event).await
    }

    /// Emit artifact saved event using unified SessionEvent
    pub async fn emit_artifact_update(&self, artifact: a2a_types::Artifact) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::TaskArtifactUpdate { artifact },
        );
        self.event_processor.process_event(event).await
    }

    // ===== Business Logic Methods (delegate to QueryService) =====

    /// Get task by ID using event sourcing (A2A Protocol: tasks/get)
    pub async fn get_task(&self) -> AgentResult<Option<Task>> {
        self.query_service
            .get_task(&self.app_name, &self.user_id, &self.task_id)
            .await
    }

    /// List tasks for app/user using event sourcing (A2A Protocol: tasks/list)
    pub async fn list_tasks(&self, context_id: Option<&str>) -> AgentResult<Vec<Task>> {
        self.query_service
            .list_tasks(&self.app_name, &self.user_id, context_id)
            .await
    }

    /// Get LLM conversation history from session events
    pub async fn get_llm_conversation(&self) -> AgentResult<Vec<crate::models::content::Content>> {
        self.query_service
            .get_conversation(&self.app_name, &self.user_id, &self.context_id)
            .await
    }

    /// Get state value from session (for ToolContext state access)
    pub async fn get_state(
        &self,
        scope: crate::sessions::StateScope,
        key: &str,
    ) -> AgentResult<Option<serde_json::Value>> {
        self.query_service
            .get_state(&self.app_name, &self.user_id, &self.context_id, scope, key)
            .await
    }

    /// Get access to the event processor (for event emission)
    pub fn event_processor(&self) -> &Arc<EventProcessor> {
        &self.event_processor
    }

    /// Get access to the query service (for ToolContext)
    pub fn query_service(&self) -> &Arc<QueryService> {
        &self.query_service
    }
}
