use crate::errors::{AgentError, AgentResult};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

// Use A2A types directly
use super::task_store::TaskStore;
use crate::a2a::{
    Artifact, Message, SendStreamingMessageResult, Task, TaskArtifactUpdateEvent, TaskState,
    TaskStatus, TaskStatusUpdateEvent,
};

/// High-level task management operations.
///
/// TaskManager provides a clean API for task lifecycle operations while maintaining
/// A2A protocol compliance. It uses a TaskStore for persistence and generates proper
/// A2A events for protocol compliance.
///
/// Key responsibilities:
/// - Task CRUD operations with security isolation
/// - A2A Task lifecycle management
/// - A2A event generation (TaskStatusUpdate, TaskArtifactUpdate)
/// - Integration with built-in tools (update_status, save_artifact)
pub struct TaskManager {
    store: Arc<dyn TaskStore>,
}

impl TaskManager {
    /// Create a new TaskManager with the specified TaskStore backend
    pub fn new(store: Arc<dyn TaskStore>) -> Self {
        Self { store }
    }

    /// Create a new task within a specific context (session).
    ///
    /// Returns a new A2A Task with:
    /// - Auto-generated task ID
    /// - Submitted initial state
    /// - Empty history and artifacts
    /// - Proper timestamps
    pub async fn create_task(
        &self,
        app_name: String,
        user_id: String,
        context_id: String,
    ) -> AgentResult<Task> {
        let task_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        let task = Task {
            kind: "task".to_string(),
            id: task_id.clone(),
            context_id,
            status: TaskStatus {
                state: TaskState::Submitted,
                timestamp: Some(now.to_rfc3339()),
                message: None,
            },
            history: Vec::new(),
            artifacts: Vec::new(),
            metadata: None,
        };

        self.store.save_task(&app_name, &user_id, &task).await?;
        Ok(task)
    }

    /// Overwrite/save a full task to storage
    pub async fn save_task(&self, app_name: &str, user_id: &str, task: &Task) -> AgentResult<()> {
        self.store.save_task(app_name, user_id, task).await
    }

    /// Retrieve a task by ID.
    ///
    /// Returns the complete A2A Task with history (messages) and artifacts included.
    /// Returns None if task doesn't exist or access is denied.
    pub async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<Task>> {
        self.store.get_task(app_name, user_id, task_id).await
    }

    /// Add a message to a task's history.
    ///
    /// This operation is atomic and thread-safe, preventing lost updates in concurrent scenarios.
    /// Messages are appended to the task's history in chronological order.
    pub async fn add_message(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        message: Message,
    ) -> AgentResult<()> {
        // Use atomic append operation to prevent race conditions
        self.store
            .append_message(app_name, user_id, task_id, message)
            .await
    }

    /// Add an artifact to a task and generate A2A TaskArtifactUpdate event.
    ///
    /// This method:
    /// 1. Atomically adds the artifact to the task's artifacts collection
    /// 2. Returns an A2A-compliant TaskArtifactUpdateEvent for protocol compliance
    ///    Thread-safe: Uses atomic operations to prevent lost updates in concurrent scenarios.
    pub async fn add_artifact(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        artifact: Artifact,
    ) -> AgentResult<TaskArtifactUpdateEvent> {
        // Use atomic append operation to prevent race conditions
        self.store
            .append_artifact(app_name, user_id, task_id, artifact.clone())
            .await?;

        // Get the task to retrieve context_id for event generation
        let task = self
            .get_task(app_name, user_id, task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;

        // Generate A2A TaskArtifactUpdate event
        let event = TaskArtifactUpdateEvent {
            kind: "artifact-update".to_string(),
            task_id: task_id.to_string(),
            context_id: task.context_id.clone(),
            artifact,
            append: None,
            last_chunk: Some(true),
            metadata: None,
        };

        Ok(event)
    }

    /// Update a task's status and generate A2A TaskStatusUpdate event.
    ///
    /// This method:
    /// 1. Atomically updates the task's status
    /// 2. Returns an A2A-compliant TaskStatusUpdateEvent for protocol compliance
    ///    Thread-safe: Uses atomic operations to prevent lost updates in concurrent scenarios.
    pub async fn update_status(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        status: TaskStatus,
    ) -> AgentResult<TaskStatusUpdateEvent> {
        // Use atomic update operation to prevent race conditions
        self.store
            .update_task_status(app_name, user_id, task_id, status.clone())
            .await?;

        // Get the task to retrieve context_id for event generation
        let task = self
            .get_task(app_name, user_id, task_id)
            .await?
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;

        // Generate A2A TaskStatusUpdate event
        let is_final = matches!(
            status.state,
            TaskState::Completed | TaskState::Failed | TaskState::Rejected | TaskState::Canceled
        );

        let event = TaskStatusUpdateEvent {
            kind: "status-update".to_string(),
            task_id: task_id.to_string(),
            context_id: task.context_id.clone(),
            status,
            is_final,
            metadata: None,
        };

        Ok(event)
    }

    /// List tasks for a specific app/user, optionally filtered by context.
    ///
    /// Returns tasks ordered by creation time (newest first).
    /// If context_id is provided, only returns tasks from that context (session).
    pub async fn list_tasks(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<Task>> {
        self.store.list_tasks(app_name, user_id, context_id).await
    }

    /// Delete a task from storage.
    ///
    /// This is a permanent operation that removes the task and all its data.
    /// Succeeds silently if the task doesn't exist (idempotent).
    pub async fn delete_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<()> {
        self.store.delete_task(app_name, user_id, task_id).await
    }

    /// Check if a task exists without retrieving it.
    ///
    /// More efficient than get_task() when you only need existence verification.
    pub async fn task_exists(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<bool> {
        self.store.task_exists(app_name, user_id, task_id).await
    }

    /// Get a task with its A2A events
    /// Returns tuple of (Task, Vec<A2A Events>) or None if task doesn't exist
    pub async fn get_task_with_events(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<(Task, Vec<SendStreamingMessageResult>)>> {
        // Use trait method - no downcasting needed
        self.store
            .get_task_with_events(app_name, user_id, task_id)
            .await
    }

    /// Add an A2A event to a task's event history
    /// This is called by event handlers when A2A events are generated
    pub async fn add_task_event(
        &self,
        task_id: &str,
        event: SendStreamingMessageResult,
    ) -> AgentResult<()> {
        // Use trait method - no downcasting needed
        self.store.add_task_event(task_id, event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::{Message, MessageRole, Part};
    use crate::task::InMemoryTaskStore;
    use std::sync::Arc;
    use tokio::task::JoinSet;

    // Helper to create a TaskManager with an in-memory store for testing
    fn create_test_task_manager() -> TaskManager {
        let store = Arc::new(InMemoryTaskStore::new());
        TaskManager::new(store)
    }

    #[tokio::test]
    async fn test_create_task() {
        let manager = create_test_task_manager();
        let task = manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        assert_eq!(task.status.state, TaskState::Submitted);
        assert_eq!(task.context_id, "ctx1");

        let retrieved = manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.id, task.id);
    }

    #[tokio::test]
    async fn test_add_message() {
        let manager = create_test_task_manager();
        let task = manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let message = Message {
            kind: "message".to_string(),
            message_id: "msg1".to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "Hello".to_string(),
                metadata: None,
            }],
            context_id: Some("ctx1".to_string()),
            task_id: Some(task.id.clone()),
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        };

        manager
            .add_message("app1", "user1", &task.id, message)
            .await
            .unwrap();

        let updated_task = manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.history.len(), 1);
        assert_eq!(updated_task.history[0].message_id, "msg1");
    }

    #[tokio::test]
    async fn test_add_artifact() {
        let manager = create_test_task_manager();
        let task = manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let artifact = Artifact {
            artifact_id: "art1".to_string(),
            parts: vec![Part::Text {
                text: "Artifact data".to_string(),
                metadata: None,
            }],
            name: Some("Test Artifact".to_string()),
            description: Some("Test artifact description".to_string()),
            extensions: Vec::new(),
            metadata: None,
        };

        let event = manager
            .add_artifact("app1", "user1", &task.id, artifact)
            .await
            .unwrap();

        assert_eq!(event.task_id, task.id);
        assert_eq!(event.artifact.artifact_id, "art1");

        let updated_task = manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.artifacts.len(), 1);
        assert_eq!(updated_task.artifacts[0].artifact_id, "art1");
    }

    #[tokio::test]
    async fn test_update_status() {
        let manager = create_test_task_manager();
        let task = manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();

        let status = TaskStatus {
            state: TaskState::Completed,
            timestamp: Some(Utc::now().to_rfc3339()),
            message: None,
        };

        let event = manager
            .update_status("app1", "user1", &task.id, status)
            .await
            .unwrap();

        assert_eq!(event.task_id, task.id);
        assert_eq!(event.status.state, TaskState::Completed);
        assert!(event.is_final);

        let updated_task = manager
            .get_task("app1", "user1", &task.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.status.state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_task_not_found() {
        let manager = create_test_task_manager();
        let result = manager.get_task("app1", "user1", "nonexistent").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        let add_msg_err = manager
            .add_message(
                "app1",
                "user1",
                "nonexistent",
                Message {
                    kind: "message".to_string(),
                    message_id: "msg1".to_string(),
                    role: MessageRole::User,
                    parts: vec![Part::Text {
                        text: "Hello".to_string(),
                        metadata: None,
                    }],
                    context_id: Some("ctx1".to_string()),
                    task_id: Some("nonexistent".to_string()),
                    reference_task_ids: Vec::new(),
                    extensions: Vec::new(),
                    metadata: None,
                },
            )
            .await;

        // Should fail with TaskNotFound error
        assert!(matches!(
            add_msg_err,
            Err(crate::errors::AgentError::TaskNotFound { task_id })
                if task_id == "nonexistent"
        ));
    }

    #[tokio::test]
    async fn test_concurrency_no_lost_updates() {
        let manager = Arc::new(create_test_task_manager());
        let task = manager
            .create_task("app1".to_string(), "user1".to_string(), "ctx1".to_string())
            .await
            .unwrap();
        let task_id = task.id.clone();

        let mut join_set = JoinSet::new();

        let num_messages = 50;
        let num_artifacts = 50;

        // Spawn tasks to add messages
        for i in 0..num_messages {
            let manager_clone = Arc::clone(&manager);
            let task_id_clone = task_id.clone();
            join_set.spawn(async move {
                let message = Message {
                    message_id: format!("msg_{}", i),
                    kind: "message".to_string(),
                    role: MessageRole::User,
                    parts: Vec::new(),
                    context_id: None,
                    task_id: Some(task_id_clone.clone()),
                    reference_task_ids: Vec::new(),
                    extensions: Vec::new(),
                    metadata: None,
                };
                manager_clone
                    .add_message("app1", "user1", &task_id_clone, message)
                    .await
            });
        }

        // Spawn tasks to add artifacts
        for i in 0..num_artifacts {
            let manager_clone = Arc::clone(&manager);
            let task_id_clone = task_id.clone();
            join_set.spawn(async move {
                let artifact = Artifact {
                    artifact_id: format!("art_{}", i),
                    parts: Vec::new(),
                    name: None,
                    description: None,
                    extensions: Vec::new(),
                    metadata: None,
                };
                manager_clone
                    .add_artifact("app1", "user1", &task_id_clone, artifact)
                    .await
                    .map(|_| ())
            });
        }

        // Wait for all tasks to complete
        while let Some(result) = join_set.join_next().await {
            assert!(result.is_ok());
            assert!(result.unwrap().is_ok());
        }

        // Verify the final state
        let final_task = manager
            .get_task("app1", "user1", &task_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(final_task.history.len(), num_messages);
        assert_eq!(final_task.artifacts.len(), num_artifacts);

        // Check if all unique IDs are present
        let message_ids: std::collections::HashSet<_> =
            final_task.history.iter().map(|m| &m.message_id).collect();
        assert_eq!(message_ids.len(), num_messages);

        let artifact_ids: std::collections::HashSet<_> = final_task
            .artifacts
            .iter()
            .map(|a| &a.artifact_id)
            .collect();
        assert_eq!(artifact_ids.len(), num_artifacts);
    }
}
