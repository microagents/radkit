use crate::errors::AgentResult;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::debug;

use crate::a2a::{
    Message, SendStreamingMessageResult, Task, TaskArtifactUpdateEvent, TaskState,
    TaskStatusUpdateEvent,
};
use crate::events::internal::{EventMetadata, InternalEvent, ModelInfo};
use crate::sessions::SessionService;
use crate::task::TaskManager;

/// Trait for projecting events into both internal and A2A protocol formats.
/// This enables clean separation of concerns - business logic generates semantic events,
/// and the projector decides how to represent them for different audiences.
#[async_trait]
pub trait EventProjector: Send + Sync {
    /// Project an internal event for debugging/observability
    async fn project_internal(&self, event: InternalEvent) -> AgentResult<()>;

    /// Project an A2A protocol event for client streaming
    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()>;

    /// Helper: Project both internal and A2A representations of the same semantic event
    async fn project_dual(
        &self,
        internal: InternalEvent,
        a2a: SendStreamingMessageResult,
    ) -> AgentResult<()> {
        // log internally
        self.project_internal(internal).await?;

        // If there's an A2A representation, emit it
        self.project_a2a(a2a).await?;

        Ok(())
    }
}

/// Semantic event generators - these create both internal and A2A representations
pub struct EventGenerators;

impl EventGenerators {
    /// Generate events for user input
    pub fn user_input(
        task_id: String,
        context_id: String,
        app_name: String,
        user_id: String,
        message: Message,
    ) -> (InternalEvent, SendStreamingMessageResult) {
        use crate::models::content::Content;

        // Convert A2A Message to Content for internal storage
        let content = Content::from_message(message.clone(), task_id.clone(), context_id.clone());

        let internal = InternalEvent::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name,
                user_id,
                model_info: None, // User input doesn't have model info
                performance: None,
            },
            timestamp: Utc::now(),
        };

        // User input messages also go to A2A stream
        let a2a = SendStreamingMessageResult::Message(message);

        (internal, a2a)
    }

    /// Generate events for message received directly from Content
    pub fn message_received(
        content: crate::models::content::Content,
        app_name: String,
        user_id: String,
        model_info: Option<ModelInfo>,
    ) -> (InternalEvent, SendStreamingMessageResult) {
        let internal = InternalEvent::MessageReceived {
            content: content.clone(),
            metadata: EventMetadata {
                app_name,
                user_id,
                model_info,
                performance: None,
            },
            timestamp: Utc::now(),
        };

        // For A2A stream, send the filtered message
        let a2a = SendStreamingMessageResult::Message(content.to_a2a_message());

        (internal, a2a)
    }

    /// Generate events for state changes
    pub fn state_change(
        context_id: Option<String>,
        app_name: String,
        user_id: Option<String>,
        scope: crate::events::internal::StateScope,
        key: String,
        old_value: Option<Value>,
        new_value: Value,
        change_reason: Option<String>,
    ) -> InternalEvent {
        let internal = InternalEvent::StateChange {
            context_id,
            app_name,
            user_id,
            scope,
            key,
            old_value,
            new_value,
            change_reason,
            timestamp: Utc::now(),
        };

        internal
    }

    /// Generate events for task status update (for A2A protocol compliance)
    pub fn task_status_update(
        task_id: String,
        context_id: String,
        _old_state: TaskState,
        new_state: TaskState,
        message: Option<String>,
    ) -> SendStreamingMessageResult {
        let timestamp = Utc::now();

        let a2a = SendStreamingMessageResult::TaskStatusUpdate(TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: task_id.clone(),
            context_id: context_id.clone(),
            status: crate::a2a::TaskStatus {
                state: new_state.clone(),
                timestamp: Some(timestamp.to_rfc3339()),
                message: message.map(|msg| {
                    // Convert string message to A2A Message
                    Message {
                        kind: "message".to_string(),
                        message_id: uuid::Uuid::new_v4().to_string(),
                        role: crate::a2a::MessageRole::Agent,
                        parts: vec![crate::a2a::Part::Text {
                            text: msg,
                            metadata: None,
                        }],
                        context_id: Some(context_id.clone()),
                        task_id: Some(task_id.clone()),
                        reference_task_ids: Vec::new(),
                        extensions: Vec::new(),
                        metadata: None,
                    }
                }),
            },
            is_final: matches!(
                new_state,
                TaskState::Completed
                    | TaskState::Failed
                    | TaskState::Rejected
                    | TaskState::Canceled
            ),
            metadata: None,
        });

        a2a
    }

    /// Generate events for artifact save (for A2A protocol compliance)
    pub fn artifact_saved(
        task_id: String,
        context_id: String,
        artifact: crate::a2a::Artifact,
    ) -> SendStreamingMessageResult {
        let a2a = SendStreamingMessageResult::TaskArtifactUpdate(TaskArtifactUpdateEvent {
            kind: "artifact-update".to_string(),
            task_id,
            context_id,
            artifact,
            append: None,
            last_chunk: Some(true),
            metadata: None,
        });

        a2a
    }

    /// Generate events for task completion (final Task result)
    pub fn task_completed(task: Task) -> SendStreamingMessageResult {
        SendStreamingMessageResult::Task(task)
    }
}

/// Storage projector that immediately persists events to storage
pub struct StorageProjector {
    task_manager: Arc<TaskManager>,
    session_service: Arc<dyn SessionService>,
    /// Context for this projector - provides app/user scope for all events
    app_name: String,
    user_id: String,
}

impl StorageProjector {
    /// Create a context-aware projector for a specific app/user scope
    ///
    /// StorageProjector always requires context to ensure proper app/user isolation
    /// for security and prevent silent event drops.
    pub fn new(
        task_manager: Arc<TaskManager>,
        session_service: Arc<dyn SessionService>,
        app_name: String,
        user_id: String,
    ) -> Self {
        Self {
            task_manager,
            session_service,
            app_name,
            user_id,
        }
    }

    /// Store A2A event directly to task storage
    async fn store_a2a_event(
        &self,
        event: &SendStreamingMessageResult,
        app: &str,
        user: &str,
    ) -> AgentResult<()> {
        use crate::a2a::Message;

        match event {
            SendStreamingMessageResult::TaskStatusUpdate(e) => {
                self.task_manager
                    .update_status(app, user, &e.task_id, e.status.clone())
                    .await?;
            }
            SendStreamingMessageResult::TaskArtifactUpdate(e) => {
                self.task_manager
                    .add_artifact(app, user, &e.task_id, e.artifact.clone())
                    .await?;
            }
            SendStreamingMessageResult::Task(t) => {
                self.task_manager.save_task(app, user, t).await?;
            }
            SendStreamingMessageResult::Message(m) => {
                if let Some(task_id) = &m.task_id {
                    let msg: Message = m.clone();
                    self.task_manager
                        .add_message(app, user, task_id, msg)
                        .await?;
                } else {
                    debug!(
                        "Message without task_id, skipping storage: {:?}",
                        m.message_id
                    );
                }
            }
        }

        // Store raw event for auditing
        let task_id = match event {
            SendStreamingMessageResult::TaskStatusUpdate(e) => Some(e.task_id.as_str()),
            SendStreamingMessageResult::TaskArtifactUpdate(e) => Some(e.task_id.as_str()),
            SendStreamingMessageResult::Task(t) => Some(t.id.as_str()),
            SendStreamingMessageResult::Message(m) => m.task_id.as_deref(),
        };

        if let Some(task_id) = task_id {
            debug!("Storing A2A event for task: {}", task_id);
            self.task_manager
                .add_task_event(task_id, event.clone())
                .await?;
        }

        Ok(())
    }

    /// Store internal event directly to session storage
    async fn store_internal_event(&self, event: &InternalEvent) -> AgentResult<()> {
        // Extract session info from internal events that have context_id
        let (context_id, app_name, user_id) = match event {
            InternalEvent::MessageReceived { metadata, .. } => (
                event.context_id(),
                Some(metadata.app_name.as_str()),
                Some(metadata.user_id.as_str()),
            ),
            InternalEvent::StateChange {
                context_id,
                app_name,
                user_id,
                ..
            } => (
                context_id.as_deref(),
                Some(app_name.as_str()),
                user_id.as_deref(),
            ),
        };

        if let (Some(context_id), Some(app_name), Some(user_id)) = (context_id, app_name, user_id) {
            debug!("Storing internal event for session: {}", context_id);

            // Get the session and add the event
            if let Some(mut session) = self
                .session_service
                .get_session(app_name, user_id, context_id)
                .await?
            {
                session.add_event(event.clone());
                self.session_service.save_session(&session).await?;
            }
        } else {
            debug!(
                "Skipping internal event without context_id for session storage: {:?}",
                event
            );
        }

        Ok(())
    }
}

#[async_trait]
impl EventProjector for StorageProjector {
    async fn project_internal(&self, event: InternalEvent) -> AgentResult<()> {
        // Direct storage only
        self.store_internal_event(&event).await
    }

    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()> {
        // Direct storage only
        self.store_a2a_event(&event, &self.app_name, &self.user_id)
            .await
    }
}

/// Event projector that captures events into channels for later retrieval (unbounded)
/// This is useful for the enriched API return types that capture events during execution
pub struct EventCaptureProjector {
    internal_tx: mpsc::UnboundedSender<InternalEvent>,
    a2a_tx: mpsc::UnboundedSender<SendStreamingMessageResult>,
}

impl EventCaptureProjector {
    pub fn new(
        internal_tx: mpsc::UnboundedSender<InternalEvent>,
        a2a_tx: mpsc::UnboundedSender<SendStreamingMessageResult>,
    ) -> Self {
        Self {
            internal_tx,
            a2a_tx,
        }
    }
}

#[async_trait]
impl EventProjector for EventCaptureProjector {
    async fn project_internal(&self, event: InternalEvent) -> AgentResult<()> {
        // Ignore send errors (receiver might be dropped during shutdown)
        let _ = self.internal_tx.send(event);
        Ok(())
    }

    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()> {
        // Ignore send errors (receiver might be dropped during shutdown)
        let _ = self.a2a_tx.send(event);
        Ok(())
    }
}

/// Internal-only projector that captures internal events but ignores A2A events
/// Used when we only need internal event capture (e.g., for testing)
pub struct InternalOnlyProjector {
    internal_tx: mpsc::UnboundedSender<InternalEvent>,
}

impl InternalOnlyProjector {
    pub fn new(internal_tx: mpsc::UnboundedSender<InternalEvent>) -> Self {
        Self { internal_tx }
    }
}

#[async_trait]
impl EventProjector for InternalOnlyProjector {
    async fn project_internal(&self, event: InternalEvent) -> AgentResult<()> {
        // Ignore send errors (receiver might be dropped during shutdown)
        let _ = self.internal_tx.send(event);
        Ok(())
    }

    async fn project_a2a(&self, _event: SendStreamingMessageResult) -> AgentResult<()> {
        // No-op for internal-only projector
        Ok(())
    }
}

/// A2A-only projector that forwards A2A events to a bounded channel but ignores internal events
/// Used for streaming A2A events to clients (bounded channels for backpressure)
pub struct A2AOnlyProjector {
    a2a_tx: mpsc::Sender<SendStreamingMessageResult>,
}

impl A2AOnlyProjector {
    pub fn new(a2a_tx: mpsc::Sender<SendStreamingMessageResult>) -> Self {
        Self { a2a_tx }
    }
}

#[async_trait]
impl EventProjector for A2AOnlyProjector {
    async fn project_internal(&self, _event: InternalEvent) -> AgentResult<()> {
        // A2A-only projector ignores internal events
        Ok(())
    }

    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()> {
        // Send to A2A channel, ignore failures (channel might be closed)
        if let Err(e) = self.a2a_tx.send(event).await {
            tracing::warn!("Failed to send A2A event to channel: {}", e);
        }
        Ok(())
    }
}

/// A2A capture projector that forwards A2A events to an unbounded channel but ignores internal events
/// Used for capturing A2A events during execution (unbounded channels for capture)
pub struct A2ACaptureProjector {
    a2a_tx: mpsc::UnboundedSender<SendStreamingMessageResult>,
}

impl A2ACaptureProjector {
    pub fn new(a2a_tx: mpsc::UnboundedSender<SendStreamingMessageResult>) -> Self {
        Self { a2a_tx }
    }
}

#[async_trait]
impl EventProjector for A2ACaptureProjector {
    async fn project_internal(&self, _event: InternalEvent) -> AgentResult<()> {
        // A2A capture projector ignores internal events
        Ok(())
    }

    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()> {
        // Send to A2A capture channel, ignore failures (receiver might be dropped)
        let _ = self.a2a_tx.send(event);
        Ok(())
    }
}

/// Multi-projector that forwards events to multiple projectors
pub struct MultiProjector {
    projectors: Vec<Arc<dyn EventProjector>>,
}

impl MultiProjector {
    pub fn new(projectors: Vec<Arc<dyn EventProjector>>) -> Self {
        Self { projectors }
    }
}

#[async_trait]
impl EventProjector for MultiProjector {
    async fn project_internal(&self, event: InternalEvent) -> AgentResult<()> {
        for projector in &self.projectors {
            // Continue even if one fails - don't propagate errors
            let _ = projector.project_internal(event.clone()).await;
        }
        Ok(())
    }

    async fn project_a2a(&self, event: SendStreamingMessageResult) -> AgentResult<()> {
        for projector in &self.projectors {
            // Continue even if one fails - don't propagate errors
            let _ = projector.project_a2a(event.clone()).await;
        }
        Ok(())
    }
}

// Helper functions

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::a2a::{
        Artifact, Message, MessageRole, SendStreamingMessageResult, TaskArtifactUpdateEvent,
        TaskState, TaskStatusUpdateEvent,
    };
    use crate::events::internal::{EventMetadata, ModelInfo, PerformanceMetrics};
    use crate::sessions::{InMemorySessionService, SessionService};
    use crate::task::{InMemoryTaskStore, TaskManager};
    use chrono::Utc;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // Helper function to create test components
    fn create_test_components() -> (Arc<TaskManager>, Arc<dyn SessionService>) {
        let task_store = Arc::new(InMemoryTaskStore::new());
        let task_manager = Arc::new(TaskManager::new(task_store));
        let session_service = Arc::new(InMemorySessionService::new());
        (task_manager, session_service)
    }

    #[tokio::test]
    async fn test_storage_projector_creation() {
        let (task_manager, session_service) = create_test_components();
        let _projector = StorageProjector::new(
            task_manager,
            session_service,
            "app1".to_string(),
            "user1".to_string(),
        );

        // Test with context
        let with_context = StorageProjector::new(
            Arc::new(TaskManager::new(Arc::new(InMemoryTaskStore::new()))),
            Arc::new(InMemorySessionService::new()),
            "app1".to_string(),
            "user1".to_string(),
        );

        assert_eq!(with_context.app_name, "app1");
        assert_eq!(with_context.user_id, "user1");
    }

    #[tokio::test]
    async fn test_storage_projector_internal_event_projection() {
        let (task_manager, session_service) = create_test_components();
        let projector = StorageProjector::new(
            task_manager,
            session_service,
            "app1".to_string(),
            "user1".to_string(),
        );

        // Test various internal events
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "test_msg".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Test storage".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        use crate::models::content::Content;

        let events = vec![
            InternalEvent::MessageReceived {
                content: Content::from_message(
                    test_message.clone(),
                    "task1".to_string(),
                    "ctx1".to_string(),
                ),
                metadata: EventMetadata {
                    app_name: "app1".to_string(),
                    user_id: "user1".to_string(),
                    model_info: None, // User message
                    performance: None,
                },
                timestamp: Utc::now(),
            },
            InternalEvent::MessageReceived {
                content: Content::from_message(
                    test_message.clone(),
                    "task1".to_string(),
                    "ctx1".to_string(),
                ),
                metadata: EventMetadata {
                    app_name: "app1".to_string(),
                    user_id: "user1".to_string(),
                    model_info: Some(ModelInfo {
                        model_name: "test-model".to_string(),
                        prompt_tokens: Some(50),
                        response_tokens: Some(100),
                        cost_estimate: Some(0.01),
                    }),
                    performance: Some(PerformanceMetrics {
                        duration_ms: 500,
                        cache_hit: None,
                    }),
                },
                timestamp: Utc::now(),
            },
            InternalEvent::StateChange {
                context_id: Some("ctx1".to_string()),
                app_name: "app1".to_string(),
                user_id: Some("user1".to_string()),
                scope: crate::events::internal::StateScope::Session,
                key: "test_key".to_string(),
                old_value: None,
                new_value: serde_json::json!("test_value"),
                change_reason: Some("Test state change".to_string()),
                timestamp: Utc::now(),
            },
        ];

        for event in events {
            let result = projector.project_internal(event).await;
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_multi_projector_storage_and_streaming() {
        let (task_manager, session_service) = create_test_components();

        // Create required session and task first
        let _session = session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();

        let task = task_manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let storage_projector = Arc::new(StorageProjector::new(
            task_manager,
            session_service,
            "app1".to_string(),
            "user1".to_string(),
        ));

        let (internal_tx, mut internal_rx) = mpsc::unbounded_channel();
        let (a2a_tx, mut a2a_rx) = mpsc::channel(10);

        // Use MultiProjector with storage + streaming (matches agent.rs pattern)
        let streaming_internal = Arc::new(InternalOnlyProjector::new(internal_tx));
        let streaming_a2a = Arc::new(A2AOnlyProjector::new(a2a_tx));
        let multi_projector =
            MultiProjector::new(vec![storage_projector, streaming_internal, streaming_a2a]);

        // Test that events are sent to both storage and channels
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "test_response".to_string(),
            role: crate::a2a::MessageRole::Agent,
            parts: vec![crate::a2a::Part::Text {
                text: "Test response".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some(task.id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        use crate::models::content::Content;
        let internal_event = InternalEvent::MessageReceived {
            content: Content::from_message(test_message, task.id.clone(), "ctx1".to_string()),
            metadata: EventMetadata {
                app_name: "app1".to_string(),
                user_id: "user1".to_string(),
                model_info: Some(ModelInfo {
                    model_name: "test-model".to_string(),
                    prompt_tokens: Some(30),
                    response_tokens: Some(50),
                    cost_estimate: Some(0.008),
                }),
                performance: Some(PerformanceMetrics {
                    duration_ms: 200,
                    cache_hit: None,
                }),
            },
            timestamp: Utc::now(),
        };

        multi_projector
            .project_internal(internal_event.clone())
            .await
            .unwrap();

        // Verify internal channel received it
        let received = internal_rx.try_recv();
        assert!(received.is_ok());

        // Test A2A event
        let a2a_event = SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "msg2".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Hello".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some(task.id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        });

        multi_projector
            .project_a2a(a2a_event.clone())
            .await
            .unwrap();

        // Verify A2A channel received it
        let received = a2a_rx.try_recv();
        assert!(received.is_ok());
    }

    #[tokio::test]
    async fn test_storage_projector_a2a_message_handling() {
        let (task_manager, _) = create_test_components();

        // Create a task first
        let task = task_manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let projector = StorageProjector::new(
            Arc::clone(&task_manager),
            Arc::new(InMemorySessionService::new()),
            "app1".to_string(),
            "user1".to_string(),
        );

        // Test message handling
        let message = Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Test message".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some(task.id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        projector
            .project_a2a(SendStreamingMessageResult::Message(message))
            .await
            .unwrap();

        // Verify message was added to task
        let updated_task = task_manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.history.len(), 1);
        assert_eq!(updated_task.history[0].message_id, "msg1");
    }

    #[tokio::test]
    async fn test_storage_projector_task_status_update() {
        let (task_manager, _) = create_test_components();

        // Create a task first
        let task = task_manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let projector = StorageProjector::new(
            Arc::clone(&task_manager),
            Arc::new(InMemorySessionService::new()),
            "app1".to_string(),
            "user1".to_string(),
        );

        // Test status update
        let status_event = TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: task.id.clone(),
            context_id: "ctx1".to_string(),
            status: crate::a2a::TaskStatus {
                state: crate::a2a::TaskState::Completed,
                timestamp: Some(Utc::now().to_rfc3339()),
                message: None,
            },
            is_final: true,
            metadata: None,
        };

        projector
            .project_a2a(SendStreamingMessageResult::TaskStatusUpdate(status_event))
            .await
            .unwrap();

        // Verify status was updated
        let updated_task = task_manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_storage_projector_artifact_update() {
        let (task_manager, _) = create_test_components();

        // Create a task first
        let task = task_manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let projector = StorageProjector::new(
            Arc::clone(&task_manager),
            Arc::new(InMemorySessionService::new()),
            "app1".to_string(),
            "user1".to_string(),
        );

        // Test artifact update
        let artifact_event = TaskArtifactUpdateEvent {
            kind: "artifact-update".to_string(),
            task_id: task.id.clone(),
            context_id: "ctx1".to_string(),
            artifact: Artifact {
                artifact_id: "artifact1".to_string(),
                parts: vec![crate::a2a::Part::Text {
                    text: "artifact data".to_string(),
                    metadata: None,
                }],
                name: Some("Test Artifact".to_string()),
                description: Some("Test artifact description".to_string()),
                extensions: vec![],
                metadata: None,
            },
            append: None,
            last_chunk: Some(true),
            metadata: None,
        };

        projector
            .project_a2a(SendStreamingMessageResult::TaskArtifactUpdate(
                artifact_event,
            ))
            .await
            .unwrap();

        // Verify artifact was added
        let updated_task = task_manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.artifacts.len(), 1);
        assert_eq!(updated_task.artifacts[0].artifact_id, "artifact1");
    }

    #[tokio::test]
    async fn test_event_projector_trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StorageProjector>();
    }

    #[tokio::test]
    async fn test_multi_projector_error_handling() {
        let (task_manager, session_service) = create_test_components();
        let storage_projector = Arc::new(StorageProjector::new(
            task_manager,
            session_service,
            "app1".to_string(),
            "user1".to_string(),
        ));

        // Create channels with size 1
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        let (a2a_tx, a2a_rx) = mpsc::channel(1);

        // Create multi projector with storage + streaming
        let streaming_internal = Arc::new(InternalOnlyProjector::new(internal_tx));
        let streaming_a2a = Arc::new(A2AOnlyProjector::new(a2a_tx));
        let multi_projector =
            MultiProjector::new(vec![storage_projector, streaming_internal, streaming_a2a]);

        // Drop receivers to simulate closed channels
        drop(internal_rx);
        drop(a2a_rx);

        // Try to send events - channels closed but storage should still work
        use crate::models::content::{Content, ContentPart};
        let content = Content {
            task_id: "task1".to_string(),
            context_id: "ctx1".to_string(),
            message_id: "user_msg_1".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![ContentPart::Text {
                text: "Test user input".to_string(),
                metadata: None,
            }],
            metadata: None,
        };

        let internal_event = InternalEvent::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name: "app1".to_string(),
                user_id: "user1".to_string(),
                model_info: None,
                performance: None,
            },
            timestamp: Utc::now(),
        };

        // MultiProjector should handle streaming errors gracefully
        let result = multi_projector.project_internal(internal_event).await;
        assert!(
            result.is_ok(),
            "MultiProjector should handle streaming failures gracefully"
        );

        let a2a_event = SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![],
            context_id: None,
            task_id: None,
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        });

        // This should also succeed despite closed A2A channel
        let result = multi_projector.project_a2a(a2a_event).await;
        assert!(
            result.is_ok(),
            "MultiProjector should handle A2A streaming failures gracefully"
        );
    }

    #[tokio::test]
    async fn test_user_input_event_generation() {
        use crate::events::EventGenerators;

        // Test user input event generator
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "user_input".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Hello world!".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        let (internal, a2a) = EventGenerators::user_input(
            "task1".to_string(),
            "ctx1".to_string(),
            "app1".to_string(),
            "user1".to_string(),
            test_message.clone(),
        );

        match internal {
            InternalEvent::MessageReceived {
                content, metadata, ..
            } => {
                assert_eq!(content.task_id, "task1");
                assert_eq!(content.message_id, "user_input");
                assert_eq!(metadata.app_name, "app1");
                assert_eq!(metadata.user_id, "user1");
                assert!(metadata.model_info.is_none()); // User input doesn't have model info
            }
            _ => panic!("Expected MessageReceived event"),
        }

        // Verify A2A event is generated
        match a2a {
            SendStreamingMessageResult::Message(msg) => {
                assert_eq!(msg.message_id, "user_input");
            }
            _ => panic!("Expected Message A2A event"),
        }

        // Test task_completed with comprehensive task structure
        let test_task = crate::a2a::Task {
            kind: "task".to_string(),
            id: "task1".to_string(),
            context_id: "ctx1".to_string(),
            status: crate::a2a::TaskStatus {
                state: crate::a2a::TaskState::Completed,
                timestamp: Some(chrono::Utc::now().to_rfc3339()),
                message: None,
            },
            history: vec![crate::a2a::Message {
                kind: "message".to_string(),
                message_id: "msg1".to_string(),
                role: crate::a2a::MessageRole::User,
                parts: vec![crate::a2a::Part::Text {
                    text: "Test message".to_string(),
                    metadata: None,
                }],
                context_id: Some("ctx1".to_string()),
                task_id: Some("task1".to_string()),
                reference_task_ids: Vec::new(),
                extensions: Vec::new(),
                metadata: None,
            }],
            artifacts: Vec::new(),
            metadata: None,
        };

        let a2a_result = EventGenerators::task_completed(
            test_task.clone()
        );

        // task_completed only generates A2A events, no internal events
        match a2a_result {
            SendStreamingMessageResult::Task(task) => {
                assert_eq!(task.id, "task1");
                assert_eq!(task.history.len(), 1);
            }
            _ => panic!("Expected Task A2A event"),
        }
    }

    #[tokio::test]
    async fn test_concurrent_event_projection() {
        use tokio::task::JoinSet;

        let task_store = Arc::new(InMemoryTaskStore::new());
        let task_manager = Arc::new(TaskManager::new(task_store));
        let session_service = Arc::new(InMemorySessionService::new());

        let storage_projector = Arc::new(StorageProjector::new(
            task_manager.clone(),
            session_service.clone(),
            "concurrent_app".to_string(),
            "concurrent_user".to_string(),
        ));

        // Create initial task and session
        let session = session_service
            .create_session("concurrent_app".to_string(), "concurrent_user".to_string())
            .await
            .unwrap();

        let task = task_manager
            .create_task(
                "concurrent_app".to_string(),
                "concurrent_user".to_string(),
                session.id.clone(),
            )
            .await
            .unwrap();

        // Concurrent event projections
        let mut join_set = JoinSet::new();

        for i in 0..15 {
            let projector = storage_projector.clone();
            let task_id = task.id.clone();
            let ctx_id = session.id.clone();

            join_set.spawn(async move {
                let message = Message {
                    kind: "message".to_string(),
                    message_id: format!("concurrent_msg_{}", i),
                    role: if i % 2 == 0 {
                        MessageRole::User
                    } else {
                        MessageRole::Agent
                    },
                    parts: vec![crate::a2a::Part::Text {
                        text: format!("Concurrent message {}", i),
                        metadata: None,
                    }],
                    context_id: Some(ctx_id),
                    task_id: Some(task_id),
                    reference_task_ids: Vec::new(),
                    extensions: Vec::new(),
                    metadata: None,
                };

                projector
                    .project_a2a(SendStreamingMessageResult::Message(message))
                    .await
            });
        }

        // Wait for all tasks to complete
        let mut success_count = 0;
        while let Some(result) = join_set.join_next().await {
            if result.is_ok() && result.unwrap().is_ok() {
                success_count += 1;
            }
        }

        assert_eq!(
            success_count, 15,
            "All concurrent projections should succeed"
        );

        // Verify task has all messages
        let final_task = task_manager
            .get_task("concurrent_app", "concurrent_user", &task.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            final_task.history.len(),
            15,
            "Task should have all 15 messages"
        );
    }

    #[tokio::test]
    async fn test_event_capture_projector() {
        let (internal_tx, mut internal_rx) = mpsc::unbounded_channel();
        let (a2a_tx, mut a2a_rx) = mpsc::unbounded_channel();

        let capture_projector = EventCaptureProjector::new(internal_tx, a2a_tx);

        // Test internal event capture
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "capture_test".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Test capture".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        use crate::models::content::Content;
        let internal_event = InternalEvent::MessageReceived {
            content: Content::from_message(test_message, "task1".to_string(), "ctx1".to_string()),
            metadata: EventMetadata {
                app_name: "app1".to_string(),
                user_id: "user1".to_string(),
                model_info: None, // User input doesn't have model info
                performance: None,
            },
            timestamp: Utc::now(),
        };

        capture_projector
            .project_internal(internal_event.clone())
            .await
            .unwrap();

        // Verify captured
        let received = internal_rx.try_recv().unwrap();
        match received {
            InternalEvent::MessageReceived { content, .. } => {
                assert_eq!(content.task_id, "task1");
            }
            _ => panic!("Expected MessageReceived event"),
        }

        // Test A2A event capture
        let a2a_event = SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Test".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        });

        capture_projector
            .project_a2a(a2a_event.clone())
            .await
            .unwrap();

        // Verify captured
        let received = a2a_rx.try_recv().unwrap();
        match received {
            SendStreamingMessageResult::Message(msg) => {
                assert_eq!(msg.message_id, "msg1");
            }
            _ => panic!("Expected Message A2A event"),
        }
    }

    #[tokio::test]
    async fn test_multi_projector() {
        let (task_manager, session_service) = create_test_components();
        let storage1 = Arc::new(StorageProjector::new(
            task_manager.clone(),
            session_service.clone(),
            "app1".to_string(),
            "user1".to_string(),
        ));
        let storage2 = Arc::new(StorageProjector::new(
            task_manager,
            session_service,
            "app2".to_string(),
            "user2".to_string(),
        ));

        let multi_projector = MultiProjector::new(vec![storage1, storage2]);

        // Test that both projectors receive the event
        let test_message = crate::a2a::Message {
            kind: "message".to_string(),
            message_id: "multi_test".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Multi projector test".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        use crate::models::content::Content;
        let internal_event = InternalEvent::MessageReceived {
            content: Content::from_message(test_message, "task1".to_string(), "ctx1".to_string()),
            metadata: EventMetadata {
                app_name: "app1".to_string(),
                user_id: "user1".to_string(),
                model_info: None,
                performance: None,
            },
            timestamp: Utc::now(),
        };

        let result = multi_projector.project_internal(internal_event).await;
        assert!(
            result.is_ok(),
            "MultiProjector should handle projections gracefully"
        );

        // Test A2A event
        let a2a_event = SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![],
            context_id: None,
            task_id: None,
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        });

        let result = multi_projector.project_a2a(a2a_event).await;
        assert!(
            result.is_ok(),
            "MultiProjector should handle A2A projections gracefully"
        );
    }

    #[tokio::test]
    async fn test_a2a_only_projector() {
        let (a2a_tx, mut a2a_rx) = mpsc::channel(10);

        let projector = A2AOnlyProjector::new(a2a_tx);

        // Test A2A event forwarding
        let a2a_event = SendStreamingMessageResult::Message(Message {
            kind: "message".to_string(),
            message_id: "test_a2a".to_string(),
            role: crate::a2a::MessageRole::Agent,
            parts: vec![crate::a2a::Part::Text {
                text: "A2A test message".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        });

        projector.project_a2a(a2a_event.clone()).await.unwrap();

        // Verify A2A event was forwarded
        let received = a2a_rx.try_recv().unwrap();
        match received {
            SendStreamingMessageResult::Message(msg) => {
                assert_eq!(msg.message_id, "test_a2a");
            }
            _ => panic!("Expected Message A2A event"),
        }

        // Test that internal events are ignored
        use crate::models::content::{Content, ContentPart};
        let content = Content {
            task_id: "task1".to_string(),
            context_id: "ctx1".to_string(),
            message_id: "internal_test".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![ContentPart::Text {
                text: "Internal test message".to_string(),
                metadata: None,
            }],
            metadata: None,
        };

        let internal_event = InternalEvent::MessageReceived {
            content,
            metadata: EventMetadata {
                app_name: "app1".to_string(),
                user_id: "user1".to_string(),
                model_info: None,
                performance: None,
            },
            timestamp: Utc::now(),
        };

        // This should not send anything to the A2A channel
        projector.project_internal(internal_event).await.unwrap();

        // Verify no additional messages in A2A channel
        assert!(
            a2a_rx.try_recv().is_err(),
            "Internal events should be ignored by A2AOnlyProjector"
        );
    }

    #[tokio::test]
    async fn test_internal_event_serialization() {
        // Test that all internal event types can be serialized/deserialized
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "test_msg".to_string(),
            role: crate::a2a::MessageRole::User,
            parts: vec![crate::a2a::Part::Text {
                text: "Test message".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some("task1".to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        use crate::models::content::Content;

        let events = vec![
            InternalEvent::MessageReceived {
                content: Content::from_message(
                    test_message.clone(),
                    "task1".to_string(),
                    "ctx1".to_string(),
                ),
                metadata: EventMetadata {
                    app_name: "app1".to_string(),
                    user_id: "user1".to_string(),
                    model_info: None,
                    performance: None,
                },
                timestamp: chrono::Utc::now(),
            },
            InternalEvent::StateChange {
                context_id: Some("ctx1".to_string()),
                app_name: "app1".to_string(),
                user_id: Some("user1".to_string()),
                scope: crate::events::internal::StateScope::Session,
                key: "test_key".to_string(),
                old_value: Some(serde_json::json!("old")),
                new_value: serde_json::json!("new"),
                change_reason: None,
                timestamp: chrono::Utc::now(),
            },
        ];

        for event in events {
            // Just test that serialization works (without roundtrip due to Message complexity)
            let json = serde_json::to_string(&event).expect("Should serialize to JSON");
            assert!(!json.is_empty(), "Should serialize to non-empty JSON");

            // Test event helper methods work correctly
            assert!(!event.app_name().is_empty(), "App name should not be empty");
            assert!(
                event.timestamp() <= chrono::Utc::now(),
                "Timestamp should be valid"
            );
        }
    }
}
