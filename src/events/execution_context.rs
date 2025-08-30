use crate::errors::AgentResult;
use std::sync::Arc;

use crate::a2a::{MessageSendParams, TaskState};
use crate::events::{EventGenerators, EventProjector};

/// Execution context that uses the new event projection system
pub struct ProjectedExecutionContext {
    pub context_id: String,
    pub task_id: String,
    pub app_name: String,
    pub user_id: String,
    pub current_params: MessageSendParams,

    // Event projection
    event_projector: Arc<dyn EventProjector>,
}

impl ProjectedExecutionContext {
    pub fn new(
        context_id: String,
        task_id: String,
        app_name: String,
        user_id: String,
        params: MessageSendParams,
        event_projector: Arc<dyn EventProjector>,
    ) -> Self {
        Self {
            context_id,
            task_id,
            app_name,
            user_id,
            current_params: params,
            event_projector,
        }
    }

    /// Get the event projector for this execution context
    pub fn event_projector(&self) -> &Arc<dyn EventProjector> {
        &self.event_projector
    }

    /// Emit user input event
    pub async fn emit_user_input(&self, message: crate::a2a::Message) -> AgentResult<()> {
        let (internal, a2a) = EventGenerators::user_input(
            self.task_id.clone(),
            self.context_id.clone(),
            self.app_name.clone(),
            self.user_id.clone(),
            message,
        );

        self.event_projector.project_dual(internal, a2a).await
    }

    /// Emit message received event directly from Content
    pub async fn emit_message(
        &self,
        content: crate::models::content::Content,
        model_info: Option<crate::events::internal::ModelInfo>,
    ) -> AgentResult<()> {
        let (internal, a2a) = EventGenerators::message_received(
            content,
            self.app_name.clone(),
            self.user_id.clone(),
            model_info,
        );
        self.event_projector.project_dual(internal, a2a).await
    }

    /// Emit task status update event
    pub async fn emit_task_status_update(
        &self,
        old_state: TaskState,
        new_state: TaskState,
        message: Option<String>,
    ) -> AgentResult<()> {
        let a2a = EventGenerators::task_status_update(
            self.task_id.clone(),
            self.context_id.clone(),
            old_state,
            new_state,
            message,
        );

        self.event_projector.project_a2a(a2a).await
    }

    /// Emit state change event
    pub async fn emit_state_change(
        &self,
        scope: crate::events::internal::StateScope,
        key: String,
        old_value: Option<serde_json::Value>,
        new_value: serde_json::Value,
    ) -> AgentResult<()> {
        let internal = EventGenerators::state_change(
            Some(self.context_id.clone()),
            self.app_name.clone(),
            Some(self.user_id.clone()),
            scope,
            key,
            old_value,
            new_value,
        );

        self.event_projector.project_internal(internal).await
    }

    /// Emit artifact saved event
    pub async fn emit_artifact_saved(&self, artifact: crate::a2a::Artifact) -> AgentResult<()> {
        let a2a = EventGenerators::artifact_saved(
            self.task_id.clone(),
            self.context_id.clone(),
            artifact,
        );

        self.event_projector.project_a2a(a2a).await
    }

    /// Emit task completion event
    pub async fn emit_task_completed(&self, task: crate::a2a::Task) -> AgentResult<()> {
        let a2a = EventGenerators::task_completed(task);

        self.event_projector.project_a2a(a2a).await
    }

    /// Direct A2A event emission for tools (compatible with old ToolContext.emit_a2a)
    pub async fn emit_a2a(&self, event: crate::a2a::SendStreamingMessageResult) -> AgentResult<()> {
        self.event_projector.project_a2a(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::{Message, MessageRole, MessageSendParams, Part, Task, TaskState, TaskStatus};
    use crate::events::StorageProjector;
    use crate::sessions::{InMemorySessionService, SessionService};
    use crate::task::{InMemoryTaskStore, TaskManager};
    use chrono::Utc;
    use std::sync::Arc;

    async fn create_test_context() -> ProjectedExecutionContext {
        let task_manager = Arc::new(TaskManager::new(Arc::new(InMemoryTaskStore::new())));
        let session_service = Arc::new(InMemorySessionService::new());

        // Create the required session and task for the tests
        let _session = session_service
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .expect("Failed to create test session");

        let task = task_manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .expect("Failed to create test task");

        let task_id = task.id.clone();

        let projector = Arc::new(StorageProjector::new(
            task_manager,
            session_service,
            "app1".to_string(),
            "user1".to_string(),
        ));

        let params = MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: "msg1".to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: "Test message".to_string(),
                    metadata: None,
                }],
                context_id: Some("ctx1".to_string()),
                task_id: Some(task_id.clone()),
                reference_task_ids: vec![],
                extensions: vec![],
                metadata: None,
            },
            configuration: None,
            metadata: None,
        };

        ProjectedExecutionContext::new(
            "ctx1".to_string(),
            task_id,
            "app1".to_string(),
            "user1".to_string(),
            params,
            projector,
        )
    }

    #[tokio::test]
    async fn test_context_creation() {
        let context = create_test_context().await;

        assert_eq!(context.context_id, "ctx1");
        assert!(!context.task_id.is_empty(), "Task ID should be generated");
        assert_eq!(context.app_name, "app1");
        assert_eq!(context.user_id, "user1");
    }

    #[tokio::test]
    async fn test_emit_user_input() {
        let context = create_test_context().await;

        let message = Message {
            kind: "message".to_string(),
            message_id: "user_input_1".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "Hello, this is user input".to_string(),
                metadata: None,
            }],
            context_id: Some(context.context_id.clone()),
            task_id: Some(context.task_id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        let result = context.emit_user_input(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_message_received() {
        let context = create_test_context().await;

        // Create Content directly
        use crate::models::content::{Content, ContentPart};
        let content = Content {
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            message_id: "response1".to_string(),
            role: MessageRole::Agent,
            parts: vec![ContentPart::Text {
                text: "Response text".to_string(),
                metadata: None,
            }],
            metadata: None,
        };

        let model_info = Some(crate::events::internal::ModelInfo {
            model_name: "test-model".to_string(),
            prompt_tokens: Some(100),
            response_tokens: Some(50),
            cost_estimate: Some(0.01),
        });

        let result = context.emit_message(content, model_info).await;
        if let Err(e) = &result {
            println!("DEBUG: emit_message_received failed with error: {}", e);
        }
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_message_with_function_calls() {
        let context = create_test_context().await;

        // Create Content with function call and response
        use crate::models::content::{Content, ContentPart};
        let content = Content {
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            message_id: "func_call_msg".to_string(),
            role: MessageRole::Agent,
            parts: vec![
                ContentPart::FunctionCall {
                    name: "test_tool".to_string(),
                    arguments: serde_json::json!({"key": "value"}),
                    tool_use_id: Some("tool_use_123".to_string()),
                    metadata: None,
                },
                ContentPart::FunctionResponse {
                    name: "test_tool".to_string(),
                    success: true,
                    result: serde_json::json!({"success": true, "result": "completed"}),
                    error_message: None,
                    tool_use_id: Some("tool_use_123".to_string()),
                    duration_ms: Some(100),
                    metadata: None,
                },
            ],
            metadata: None,
        };

        let result = context.emit_message(content, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_message_with_failed_function_response() {
        let context = create_test_context().await;

        // Create Content with failed function response
        use crate::models::content::{Content, ContentPart};
        let content = Content {
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            message_id: "failed_func_msg".to_string(),
            role: MessageRole::Agent,
            parts: vec![ContentPart::FunctionResponse {
                name: "failing_tool".to_string(),
                success: false,
                result: serde_json::json!({}),
                error_message: Some("Tool failed with specific error".to_string()),
                tool_use_id: Some("tool_use_456".to_string()),
                duration_ms: Some(50),
                metadata: None,
            }],
            metadata: None,
        };

        let result = context.emit_message(content, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_task_status_update() {
        let context = create_test_context().await;

        let result = context
            .emit_task_status_update(TaskState::Working, TaskState::Completed, None)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_task_status_update_with_message() {
        let context = create_test_context().await;

        let result = context
            .emit_task_status_update(
                TaskState::Working,
                TaskState::Completed,
                Some("Task completed successfully".to_string()),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_task_completed() {
        let context = create_test_context().await;

        let task = Task {
            kind: "task".to_string(),
            id: "task1".to_string(),
            context_id: "ctx1".to_string(),
            status: TaskStatus {
                state: TaskState::Completed,
                timestamp: Some(Utc::now().to_rfc3339()),
                message: None,
            },
            history: vec![],
            artifacts: vec![],
            metadata: None,
        };

        let result = context.emit_task_completed(task).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_context_with_metadata() {
        let params = MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: "msg1".to_string(),
                role: MessageRole::User,
                parts: vec![],
                context_id: None,
                task_id: None,
                reference_task_ids: vec![],
                extensions: vec![],
                metadata: None,
            },
            configuration: None,
            metadata: Some({
                let mut map = std::collections::HashMap::new();
                map.insert("key".to_string(), serde_json::json!("value"));
                map
            }),
        };

        let task_manager = Arc::new(TaskManager::new(Arc::new(InMemoryTaskStore::new())));
        let session_service = Arc::new(InMemorySessionService::new());
        let projector = Arc::new(StorageProjector::new(
            task_manager,
            session_service,
            "test_app".to_string(),
            "test_user".to_string(),
        ));

        let context = ProjectedExecutionContext::new(
            "ctx2".to_string(),
            "task2".to_string(),
            "app2".to_string(),
            "user2".to_string(),
            params,
            projector,
        );

        assert!(context.current_params.metadata.is_some());
        let metadata = context.current_params.metadata.as_ref().unwrap();
        assert_eq!(metadata.get("key"), Some(&serde_json::json!("value")));
    }

    #[tokio::test]
    async fn test_emit_state_change() {
        let context = create_test_context().await;

        let result = context
            .emit_state_change(
                crate::events::internal::StateScope::Session,
                "user_preference".to_string(),
                Some(serde_json::json!("old_value")),
                serde_json::json!("new_value"),
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_event_emissions() {
        let context = create_test_context().await;

        // Emit user input
        let user_message = Message {
            kind: "message".to_string(),
            message_id: "user_msg".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "User input".to_string(),
                metadata: None,
            }],
            context_id: Some(context.context_id.clone()),
            task_id: Some(context.task_id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };
        assert!(context.emit_user_input(user_message).await.is_ok());

        // Emit model response using new Content approach
        use crate::models::content::{Content, ContentPart};
        let model_content = Content {
            task_id: context.task_id.clone(),
            context_id: context.context_id.clone(),
            message_id: "model_msg".to_string(),
            role: MessageRole::Agent,
            parts: vec![
                ContentPart::Text {
                    text: "Model response".to_string(),
                    metadata: None,
                },
                ContentPart::FunctionCall {
                    name: "analyze".to_string(),
                    arguments: serde_json::json!({"data": "test_data"}),
                    tool_use_id: Some("tool_123".to_string()),
                    metadata: None,
                },
                ContentPart::FunctionResponse {
                    name: "analyze".to_string(),
                    success: true,
                    result: serde_json::json!({"analysis": "complete"}),
                    error_message: None,
                    tool_use_id: Some("tool_123".to_string()),
                    duration_ms: Some(250),
                    metadata: None,
                },
            ],
            metadata: None,
        };

        let model_info = Some(crate::events::internal::ModelInfo {
            model_name: "test-model".to_string(),
            prompt_tokens: Some(120),
            response_tokens: Some(80),
            cost_estimate: Some(0.015),
        });

        assert!(
            context
                .emit_message(model_content, model_info)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_context_fields_immutable() {
        let context = create_test_context().await;

        // Verify fields remain constant after multiple operations
        let original_task_id = context.task_id.clone();
        // Test that we can emit a user input event
        let test_message = Message {
            kind: "message".to_string(),
            message_id: "test_msg".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "Test message".to_string(),
                metadata: None,
            }],
            context_id: Some(context.context_id.clone()),
            task_id: Some(context.task_id.clone()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };
        assert!(context.emit_user_input(test_message).await.is_ok());
        assert_eq!(context.context_id, "ctx1");
        assert_eq!(context.task_id, original_task_id);
        assert_eq!(context.app_name, "app1");
        assert_eq!(context.user_id, "user1");
    }
}
