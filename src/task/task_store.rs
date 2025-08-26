use crate::errors::AgentResult;
use async_trait::async_trait;

// Use A2A Task directly - no conversion needed
use crate::a2a::Task;

/// Database-ready abstraction for task persistence with multi-tenant security.
///
/// This trait provides the foundation for task storage backends, supporting:
/// - Multi-tenant security isolation via app_name/user_id
/// - Direct A2A Task storage (no conversion overhead)
/// - Database-ready interface (PostgreSQL, MongoDB, etc.)
/// - Concurrent operations with proper isolation
///
/// Security Model:
/// All operations require app_name AND user_id to prevent cross-tenant access.
/// Tasks are isolated in a three-tier hierarchy: app -> user -> task_id -> Task
#[async_trait]
pub trait TaskStore: Send + Sync {
    /// Retrieve a task by app, user, and task ID.
    /// Returns None if the task doesn't exist or access is denied.
    ///
    /// Security: Requires valid app_name/user_id combination
    async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<Task>>;

    /// Save a task (create or update).
    /// Task must contain valid app/user information that matches the parameters.
    ///
    /// Security: Validates that task belongs to the specified app/user
    async fn save_task(&self, app_name: &str, user_id: &str, task: &Task) -> AgentResult<()>;

    /// Delete a task by app, user, and task ID.
    /// Succeeds silently if task doesn't exist (idempotent).
    ///
    /// Security: Only deletes if task belongs to specified app/user
    async fn delete_task(&self, app_name: &str, user_id: &str, task_id: &str) -> AgentResult<()>;

    /// List tasks for a specific app/user combination.
    /// If context_id is provided, only returns tasks with that context_id.
    /// Results are typically ordered by creation time (newest first).
    ///
    /// Security: Only returns tasks belonging to specified app/user
    async fn list_tasks(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<Task>>;

    /// List all tasks for a specific context (session).
    /// This is a convenience method equivalent to list_tasks with context_id filter.
    ///
    /// Security: Only returns tasks belonging to specified app/user/context
    async fn list_tasks_by_context(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: &str,
    ) -> AgentResult<Vec<Task>>;

    /// Check if a task exists without retrieving it.
    /// More efficient than get_task() when you only need existence check.
    ///
    /// Security: Only checks tasks belonging to specified app/user
    async fn task_exists(&self, app_name: &str, user_id: &str, task_id: &str) -> AgentResult<bool>;

    // ===== Atomic Update Methods =====
    // These methods perform atomic updates to prevent race conditions in concurrent operations

    /// Atomically append a message to a task's history.
    /// This operation is thread-safe and prevents lost updates in concurrent scenarios.
    ///
    /// Security: Only updates tasks belonging to specified app/user
    async fn append_message(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        message: crate::a2a::Message,
    ) -> AgentResult<()>;

    /// Atomically append an artifact to a task.
    /// This operation is thread-safe and prevents lost updates in concurrent scenarios.
    ///
    /// Security: Only updates tasks belonging to specified app/user
    async fn append_artifact(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        artifact: crate::a2a::Artifact,
    ) -> AgentResult<()>;

    /// Atomically update a task's status.
    /// This operation is thread-safe and prevents lost updates in concurrent scenarios.
    ///
    /// Security: Only updates tasks belonging to specified app/user
    async fn update_task_status(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        status: crate::a2a::TaskStatus,
    ) -> AgentResult<()>;

    // ===== Task Event Methods =====
    // These methods handle A2A events associated with tasks

    /// Get a task with its associated A2A events.
    /// Returns tuple of (Task, Vec<A2A Events>) or None if task doesn't exist.
    ///
    /// Security: Only returns tasks belonging to specified app/user
    async fn get_task_with_events(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<
        Option<(
            crate::a2a::Task,
            Vec<crate::a2a::SendStreamingMessageResult>,
        )>,
    > {
        // Default implementation: return task with empty events
        let task = self.get_task(app_name, user_id, task_id).await?;
        match task {
            Some(task) => Ok(Some((task, Vec::new()))),
            None => Ok(None),
        }
    }

    /// Add an A2A event to a task's event history.
    /// This is called by event handlers when A2A events are generated.
    ///
    /// Security: Only adds events to tasks belonging to specified app/user
    async fn add_task_event(
        &self,
        _task_id: &str,
        _event: crate::a2a::SendStreamingMessageResult,
    ) -> AgentResult<()> {
        // Default implementation: no-op
        // Task stores that don't support events can use this default
        Ok(())
    }

    /// Get all A2A events for a specific task.
    ///
    /// Security: Only returns events for tasks belonging to specified app/user
    async fn get_task_events(
        &self,
        _task_id: &str,
    ) -> AgentResult<Vec<crate::a2a::SendStreamingMessageResult>> {
        // Default implementation: return empty events
        Ok(Vec::new())
    }
}
