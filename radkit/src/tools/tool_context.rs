//! ToolContext - Safe, limited execution context for tools
//!
//! This module provides a restricted execution context that tools receive,
//! which limits their capabilities to only safe operations while maintaining
//! access to essential functionality.

use crate::errors::AgentResult;
use crate::events::ExecutionContext;
use crate::sessions::StateScope;
use a2a_types::{Artifact, Task, TaskState};
use async_trait::async_trait;
use serde_json::Value;

/// Capability trait for tools to access and modify state
/// Tools can only modify user and session state, not app-level state
#[async_trait]
pub trait ToolStateAccess {
    /// Get app-level state (read-only for tools)
    async fn get_app_state(&self, key: &str) -> AgentResult<Option<Value>>;

    /// Get user-level state
    async fn get_user_state(&self, key: &str) -> AgentResult<Option<Value>>;

    /// Get session-level state
    async fn get_session_state(&self, key: &str) -> AgentResult<Option<Value>>;

    /// Set user-level state (automatically fetches old value for event)
    async fn set_user_state(&self, key: String, value: Value) -> AgentResult<()>;

    /// Set session-level state (automatically fetches old value for event)
    async fn set_session_state(&self, key: String, value: Value) -> AgentResult<()>;
}

/// Capability trait for tools to access task information and update status
#[async_trait]
pub trait ToolTaskAccess {
    /// Get current task information
    async fn get_current_task(&self) -> AgentResult<Option<Task>>;

    /// Update task status (automatically fetches old state for event)
    async fn update_task_status(
        &self,
        new_state: TaskState,
        message: Option<String>,
    ) -> AgentResult<()>;
}

/// Capability trait for tools to save artifacts
#[async_trait]
pub trait ToolArtifactAccess {
    /// Save an artifact from tool execution
    async fn save_artifact(&self, artifact: Artifact) -> AgentResult<()>;
}

/// Capability trait for tools to inject user input (for interactive tools)
#[async_trait]
pub trait ToolUserInteraction {
    /// Add user input content (for tools that handle user interactions)
    async fn add_user_input(&self, content: crate::models::content::Content) -> AgentResult<()>;
}

/// Safe, limited execution context for tools
/// Tools receive this instead of the full ExecutionContext to maintain security boundaries
pub struct ToolContext<'a> {
    execution_context: &'a ExecutionContext,
}

impl<'a> ToolContext<'a> {
    /// Create a ToolContext from an ExecutionContext
    pub fn from_execution_context(ctx: &'a ExecutionContext) -> Self {
        Self {
            execution_context: ctx,
        }
    }

    // Safe direct access to context properties
    pub fn context_id(&self) -> &str {
        &self.execution_context.context_id
    }

    pub fn task_id(&self) -> &str {
        &self.execution_context.task_id
    }

    pub fn app_name(&self) -> &str {
        &self.execution_context.app_name
    }

    pub fn user_id(&self) -> &str {
        &self.execution_context.user_id
    }
}

#[async_trait]
impl<'a> ToolStateAccess for ToolContext<'a> {
    async fn get_app_state(&self, key: &str) -> AgentResult<Option<Value>> {
        self.execution_context.get_state(StateScope::App, key).await
    }

    async fn get_user_state(&self, key: &str) -> AgentResult<Option<Value>> {
        self.execution_context
            .get_state(StateScope::User, key)
            .await
    }

    async fn get_session_state(&self, key: &str) -> AgentResult<Option<Value>> {
        self.execution_context
            .get_state(StateScope::Session, key)
            .await
    }

    async fn set_user_state(&self, key: String, value: Value) -> AgentResult<()> {
        // Get current value for event
        let old_value = self.get_user_state(&key).await?;

        // Emit state change event
        self.execution_context
            .emit_state_change(StateScope::User, key, old_value, value)
            .await
    }

    async fn set_session_state(&self, key: String, value: Value) -> AgentResult<()> {
        // Get current value for event
        let old_value = self.get_session_state(&key).await?;

        // Emit state change event
        self.execution_context
            .emit_state_change(StateScope::Session, key, old_value, value)
            .await
    }
}

#[async_trait]
impl<'a> ToolTaskAccess for ToolContext<'a> {
    async fn get_current_task(&self) -> AgentResult<Option<Task>> {
        self.execution_context.get_task().await
    }

    async fn update_task_status(
        &self,
        new_state: TaskState,
        message: Option<String>,
    ) -> AgentResult<()> {
        // Get current task to determine old state
        let current_task = self.get_current_task().await?;
        let old_state = current_task
            .map(|t| t.status.state)
            .unwrap_or(TaskState::Working); // Default assumption

        self.execution_context
            .emit_task_status_update(old_state, new_state, message)
            .await
    }
}

#[async_trait]
impl<'a> ToolArtifactAccess for ToolContext<'a> {
    async fn save_artifact(&self, artifact: Artifact) -> AgentResult<()> {
        self.execution_context.emit_artifact_save(artifact).await
    }
}

#[async_trait]
impl<'a> ToolUserInteraction for ToolContext<'a> {
    async fn add_user_input(&self, content: crate::models::content::Content) -> AgentResult<()> {
        self.execution_context.emit_user_input(content).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventProcessor, InMemoryEventBus};
    use crate::sessions::{InMemorySessionService, SessionService};
    use a2a_types::{Message, MessageRole, MessageSendParams, Part};
    use serde_json::json;
    use std::sync::Arc;

    // Helper function to create a test ExecutionContext with a real session
    async fn create_test_execution_context_with_session() -> ExecutionContext {
        let session_service = Arc::new(InMemorySessionService::new());

        // Create a real session first
        let session = session_service
            .create_session("test_app".to_string(), "test_user".to_string())
            .await
            .unwrap();

        let query_service = Arc::new(crate::sessions::QueryService::new(session_service.clone()));
        let event_bus = Arc::new(InMemoryEventBus::new());
        let event_processor = Arc::new(EventProcessor::new(session_service, event_bus));

        let params = MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: "test_msg".to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: "test".to_string(),
                    metadata: None,
                }],
                context_id: Some(session.id.clone()),
                task_id: Some("test_task".to_string()),
                reference_task_ids: vec![],
                extensions: vec![],
                metadata: None,
            },
            configuration: None,
            metadata: None,
        };

        ExecutionContext::new(
            session.id.clone(),
            "test_task".to_string(),
            "test_app".to_string(),
            "test_user".to_string(),
            params,
            event_processor,
            query_service,
        )
    }

    #[tokio::test]
    async fn test_tool_context_basic_access() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // With real session, the context_id will be the session UUID, not "test_ctx"
        assert!(!tool_ctx.context_id().is_empty());
        assert_eq!(tool_ctx.task_id(), "test_task");
        assert_eq!(tool_ctx.app_name(), "test_app");
        assert_eq!(tool_ctx.user_id(), "test_user");
    }

    #[tokio::test]
    async fn test_tool_state_access_empty() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Test getting non-existent state
        let app_state = tool_ctx.get_app_state("nonexistent").await.unwrap();
        assert_eq!(app_state, None);

        let user_state = tool_ctx.get_user_state("nonexistent").await.unwrap();
        assert_eq!(user_state, None);

        let session_state = tool_ctx.get_session_state("nonexistent").await.unwrap();
        assert_eq!(session_state, None);
    }

    #[tokio::test]
    async fn test_tool_user_state_management() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Set user state
        tool_ctx
            .set_user_state("preference".to_string(), json!("dark_mode"))
            .await
            .unwrap();

        // Get user state
        let retrieved = tool_ctx.get_user_state("preference").await.unwrap();
        assert_eq!(retrieved, Some(json!("dark_mode")));
    }

    #[tokio::test]
    async fn test_tool_session_state_management() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Set session state
        tool_ctx
            .set_session_state("temp_data".to_string(), json!({"key": "value"}))
            .await
            .unwrap();

        // Get session state
        let retrieved = tool_ctx.get_session_state("temp_data").await.unwrap();
        assert_eq!(retrieved, Some(json!({"key": "value"})));
    }

    #[tokio::test]
    async fn test_tool_task_access() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Initially no task should exist
        let task = tool_ctx.get_current_task().await.unwrap();
        assert_eq!(task, None);

        // Update task status should work even without existing task
        tool_ctx
            .update_task_status(TaskState::Working, Some("Processing...".to_string()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_tool_artifact_access() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        let artifact = Artifact {
            artifact_id: "test_artifact".to_string(),
            parts: vec![Part::Text {
                text: "This is a test artifact".to_string(),
                metadata: None,
            }],
            name: Some("Test Output".to_string()),
            description: Some("A test artifact".to_string()),
            extensions: Vec::new(),
            metadata: None,
        };

        // Should not panic and should complete successfully
        tool_ctx.save_artifact(artifact).await.unwrap();
    }

    #[tokio::test]
    async fn test_tool_user_interaction() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        let user_message = Message {
            kind: "message".to_string(),
            message_id: "user_msg_1".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "User interaction test".to_string(),
                metadata: None,
            }],
            context_id: Some(tool_ctx.context_id().to_string()),
            task_id: Some(tool_ctx.task_id().to_string()),
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        };

        // Convert message to content and add it
        let content = crate::models::content::Content::from_message(
            user_message,
            tool_ctx.task_id().to_string(),
            tool_ctx.context_id().to_string(),
        );

        // Should not panic and should complete successfully
        tool_ctx.add_user_input(content).await.unwrap();
    }

    #[tokio::test]
    async fn test_tool_context_state_isolation() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Set different values for different scopes
        tool_ctx
            .set_user_state("theme".to_string(), json!("dark"))
            .await
            .unwrap();
        tool_ctx
            .set_session_state("theme".to_string(), json!("light"))
            .await
            .unwrap();

        // Verify they are isolated
        let user_theme = tool_ctx.get_user_state("theme").await.unwrap();
        let session_theme = tool_ctx.get_session_state("theme").await.unwrap();

        assert_eq!(user_theme, Some(json!("dark")));
        assert_eq!(session_theme, Some(json!("light")));
    }

    #[tokio::test]
    async fn test_tool_context_state_updates_with_old_values() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Set initial value
        tool_ctx
            .set_user_state("counter".to_string(), json!(1))
            .await
            .unwrap();

        // Update value - this should fetch the old value (1) and set new value (2)
        tool_ctx
            .set_user_state("counter".to_string(), json!(2))
            .await
            .unwrap();

        // Verify final state
        let final_value = tool_ctx.get_user_state("counter").await.unwrap();
        assert_eq!(final_value, Some(json!(2)));
    }

    #[tokio::test]
    async fn test_tool_context_improved_api() {
        let exec_ctx = create_test_execution_context_with_session().await;
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);

        // Test the improved API with specific methods for each state scope

        // App state (read-only)
        let app_config = tool_ctx.get_app_state("config").await.unwrap();
        assert_eq!(app_config, None); // No app config in test

        // User state (read/write)
        let user_pref = tool_ctx.get_user_state("theme").await.unwrap();
        assert_eq!(user_pref, None); // No user preference initially

        tool_ctx
            .set_user_state("theme".to_string(), json!("dark"))
            .await
            .unwrap();
        let updated_pref = tool_ctx.get_user_state("theme").await.unwrap();
        assert_eq!(updated_pref, Some(json!("dark")));

        // Session state (read/write)
        let session_data = tool_ctx.get_session_state("temp_data").await.unwrap();
        assert_eq!(session_data, None); // No session data initially

        tool_ctx
            .set_session_state("temp_data".to_string(), json!("cached_value"))
            .await
            .unwrap();
        let updated_data = tool_ctx.get_session_state("temp_data").await.unwrap();
        assert_eq!(updated_data, Some(json!("cached_value")));
    }

    #[tokio::test]
    async fn test_tool_context_concurrent_access() {
        use tokio::task::JoinSet;

        let exec_ctx = Arc::new(create_test_execution_context_with_session().await);
        let mut join_set = JoinSet::new();

        // Spawn multiple concurrent state updates
        for i in 0..5 {
            let exec_ctx_clone = exec_ctx.clone();
            join_set.spawn(async move {
                let tool_ctx = ToolContext::from_execution_context(&exec_ctx_clone);
                tool_ctx
                    .set_user_state(format!("key_{}", i), json!(format!("value_{}", i)))
                    .await
            });
        }

        // Wait for all to complete
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }

        // All operations should succeed
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all values were set using a single tool context
        let tool_ctx = ToolContext::from_execution_context(&exec_ctx);
        for i in 0..5 {
            let value = tool_ctx
                .get_user_state(&format!("key_{}", i))
                .await
                .unwrap();
            assert_eq!(value, Some(json!(format!("value_{}", i))));
        }
    }
}
