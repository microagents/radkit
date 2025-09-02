use crate::errors::AgentResult;
use std::sync::Arc;

use crate::a2a::{MessageSendParams, TaskState};
use crate::events::EventProcessor;
use crate::sessions::{SessionEvent, SessionEventType};

/// Execution context that uses EventProcessor for clean separation
pub struct ExecutionContext {
    pub context_id: String,
    pub task_id: String,
    pub app_name: String,
    pub user_id: String,
    pub current_params: MessageSendParams,

    // Clean event processing with proper separation of concerns
    event_processor: Arc<EventProcessor>,
}

impl ExecutionContext {
    pub fn new(
        context_id: String,
        task_id: String,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
        event_processor: Arc<EventProcessor>,
    ) -> Self {
        Self {
            context_id,
            task_id,
            app_name,
            user_id,
            current_params: params,
            event_processor,
        }
    }

    /// Emit user input event using unified SessionEvent
    pub async fn emit_user_input(&self, message: crate::a2a::Message) -> AgentResult<()> {
        let content = crate::models::content::Content::from_message(
            message,
            self.task_id.clone(),
            self.context_id.clone(),
        );
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
        old_state: TaskState,
        new_state: TaskState,
        message: Option<String>,
    ) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::TaskStatusChanged {
                old_state,
                new_state,
                message,
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
    pub async fn emit_artifact_save(&self, artifact: crate::a2a::Artifact) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::ArtifactSaved { artifact },
        );
        self.event_processor.process_event(event).await
    }

    /// Emit task created event using unified SessionEvent
    pub async fn emit_task_created(&self, task: crate::a2a::Task) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::TaskCreated { task },
        );
        self.event_processor.process_event(event).await
    }

    /// Emit task completed event using unified SessionEvent
    /// This will automatically send the final task through A2A streaming
    pub async fn emit_task_completed(&self, task: crate::a2a::Task) -> AgentResult<()> {
        let event = SessionEvent::new(
            self.context_id.clone(),
            self.task_id.clone(),
            SessionEventType::TaskCompleted { task },
        );
        self.event_processor.process_event(event).await
    }

    /// Get task from EventProcessor (business logic)
    pub async fn get_task(&self) -> AgentResult<Option<crate::a2a::Task>> {
        self.event_processor
            .get_task(&self.app_name, &self.user_id, &self.task_id)
            .await
    }

    /// Get LLM conversation history from EventProcessor (business logic)
    pub async fn get_llm_conversation(&self) -> AgentResult<Vec<crate::models::content::Content>> {
        self.event_processor
            .get_llm_conversation(&self.app_name, &self.user_id, &self.context_id)
            .await
    }
}
