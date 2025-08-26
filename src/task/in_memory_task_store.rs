use crate::errors::{AgentError, AgentResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing;

use super::task_store::TaskStore;
use crate::a2a::{SendStreamingMessageResult, Task};

/// In-memory implementation of TaskStore.
///
/// This implementation provides:
/// - Multi-tenant security isolation via three-tier storage
/// - Thread-safe concurrent operations using RwLock
/// - Automatic cleanup of empty containers
/// - Development and testing-friendly storage
///
/// Security Model:
/// Tasks are stored in a three-tier hierarchy: app -> user -> task_id -> Task
/// This ensures complete isolation between different apps and users.
///
/// Storage Structure:
/// ```text
/// HashMap<app_name, HashMap<user_id, HashMap<task_id, Task>>>
/// ```
///
/// Performance Characteristics:
/// - O(1) task lookup and storage operations
/// - Memory usage scales with number of tasks
/// - Not suitable for large-scale production (use database backend)
pub struct InMemoryTaskStore {
    /// Secure three-tier task storage: app -> user -> task_id -> Task
    tasks: Arc<RwLock<HashMap<String, HashMap<String, HashMap<String, Task>>>>>,
    /// A2A events storage by task_id: task_id -> Vec<A2A Events>
    task_events: Arc<RwLock<HashMap<String, Vec<SendStreamingMessageResult>>>>,
}

impl InMemoryTaskStore {
    /// Create a new empty in-memory task store
    ///
    /// ⚠️  **WARNING: NOT FOR PRODUCTION USE** ⚠️
    /// This is an in-memory implementation suitable for development and testing only.
    /// Events will accumulate indefinitely causing memory leaks.
    /// Use a proper database-backed TaskStore implementation for production.
    pub fn new() -> Self {
        tracing::warn!(
            "🚨 InMemoryTaskStore created - NOT SUITABLE FOR PRODUCTION! Events will accumulate indefinitely. Use database-backed implementation instead."
        );

        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            task_events: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Clear all tasks from storage.
    /// Primarily used for testing and development.
    pub async fn clear(&self) {
        let mut tasks = self.tasks.write().await;
        tasks.clear();
        let mut task_events = self.task_events.write().await;
        task_events.clear();
    }

    /// Get statistics about the current storage state.
    /// Returns (total_apps, total_users, total_tasks)
    pub async fn stats(&self) -> (usize, usize, usize) {
        let tasks = self.tasks.read().await;
        let total_apps = tasks.len();
        let total_users: usize = tasks.values().map(|users| users.len()).sum();
        let total_tasks: usize = tasks
            .values()
            .flat_map(|users| users.values())
            .map(|user_tasks| user_tasks.len())
            .sum();
        (total_apps, total_users, total_tasks)
    }

    /// Add an A2A event for a specific task
    pub async fn add_task_event(
        &self,
        task_id: &str,
        event: SendStreamingMessageResult,
    ) -> AgentResult<()> {
        let mut task_events = self.task_events.write().await;
        task_events
            .entry(task_id.to_string())
            .or_insert_with(Vec::new)
            .push(event);
        Ok(())
    }

    /// Get all A2A events for a specific task
    pub async fn get_task_events(
        &self,
        task_id: &str,
    ) -> AgentResult<Vec<SendStreamingMessageResult>> {
        let task_events = self.task_events.read().await;
        Ok(task_events.get(task_id).cloned().unwrap_or_default())
    }

    /// Get task with its A2A events
    pub async fn get_task_with_events(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<(Task, Vec<SendStreamingMessageResult>)>> {
        // Get task
        let task = match self.get_task(app_name, user_id, task_id).await? {
            Some(task) => task,
            None => return Ok(None),
        };

        // Get events
        let events = self.get_task_events(task_id).await?;

        Ok(Some((task, events)))
    }
}

impl Default for InMemoryTaskStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn get_task(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
    ) -> AgentResult<Option<Task>> {
        let tasks = self.tasks.read().await;

        let task = tasks
            .get(app_name)
            .and_then(|app_users| app_users.get(user_id))
            .and_then(|user_tasks| user_tasks.get(task_id))
            .cloned();

        Ok(task)
    }

    async fn save_task(&self, app_name: &str, user_id: &str, task: &Task) -> AgentResult<()> {
        let mut tasks = self.tasks.write().await;

        // Navigate/create the three-tier structure
        let app_users = tasks
            .entry(app_name.to_string())
            .or_insert_with(HashMap::new);

        let user_tasks = app_users
            .entry(user_id.to_string())
            .or_insert_with(HashMap::new);

        // Store the task
        user_tasks.insert(task.id.clone(), task.clone());

        Ok(())
    }

    async fn delete_task(&self, app_name: &str, user_id: &str, task_id: &str) -> AgentResult<()> {
        let mut tasks = self.tasks.write().await;

        // Navigate to the task and remove it
        if let Some(app_users) = tasks.get_mut(app_name) {
            if let Some(user_tasks) = app_users.get_mut(user_id) {
                user_tasks.remove(task_id);

                // Clean up empty user container
                if user_tasks.is_empty() {
                    app_users.remove(user_id);
                }
            }

            // Clean up empty app container
            if app_users.is_empty() {
                tasks.remove(app_name);
            }
        }

        // Also remove task events
        let mut task_events = self.task_events.write().await;
        task_events.remove(task_id);

        Ok(())
    }

    async fn list_tasks(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<Task>> {
        let tasks = self.tasks.read().await;

        let user_tasks = match tasks
            .get(app_name)
            .and_then(|app_users| app_users.get(user_id))
        {
            Some(user_tasks) => user_tasks,
            None => return Ok(Vec::new()),
        };

        let mut result: Vec<Task> = if let Some(context_filter) = context_id {
            // Filter by context_id
            user_tasks
                .values()
                .filter(|task| task.context_id == context_filter)
                .cloned()
                .collect()
        } else {
            // Return all tasks for this user
            user_tasks.values().cloned().collect()
        };

        // Sort by creation time (newest first)
        // Since A2A Task doesn't have created_at, we use the task ID creation order as proxy
        result.sort_by(|a, b| b.id.cmp(&a.id));

        Ok(result)
    }

    async fn list_tasks_by_context(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: &str,
    ) -> AgentResult<Vec<Task>> {
        self.list_tasks(app_name, user_id, Some(context_id)).await
    }

    async fn task_exists(&self, app_name: &str, user_id: &str, task_id: &str) -> AgentResult<bool> {
        let tasks = self.tasks.read().await;

        let exists = tasks
            .get(app_name)
            .and_then(|app_users| app_users.get(user_id))
            .and_then(|user_tasks| user_tasks.get(task_id))
            .is_some();

        Ok(exists)
    }

    async fn append_message(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        message: crate::a2a::Message,
    ) -> AgentResult<()> {
        let mut tasks = self.tasks.write().await;

        // Navigate to the task and update it atomically while holding the write lock
        let task = tasks
            .get_mut(app_name)
            .and_then(|app_users| app_users.get_mut(user_id))
            .and_then(|user_tasks| user_tasks.get_mut(task_id))
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;

        task.history.push(message);
        Ok(())
    }

    async fn append_artifact(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        artifact: crate::a2a::Artifact,
    ) -> AgentResult<()> {
        let mut tasks = self.tasks.write().await;

        // Navigate to the task and update it atomically while holding the write lock
        let task = tasks
            .get_mut(app_name)
            .and_then(|app_users| app_users.get_mut(user_id))
            .and_then(|user_tasks| user_tasks.get_mut(task_id))
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;

        task.artifacts.push(artifact);
        Ok(())
    }

    async fn update_task_status(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        status: crate::a2a::TaskStatus,
    ) -> AgentResult<()> {
        let mut tasks = self.tasks.write().await;

        // Navigate to the task and update it atomically while holding the write lock
        let task = tasks
            .get_mut(app_name)
            .and_then(|app_users| app_users.get_mut(user_id))
            .and_then(|user_tasks| user_tasks.get_mut(task_id))
            .ok_or_else(|| AgentError::TaskNotFound {
                task_id: task_id.to_string(),
            })?;

        task.status = status;
        Ok(())
    }

    // ===== Task Event Methods Implementation =====

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
        // Get task first
        let task = match self.get_task(app_name, user_id, task_id).await? {
            Some(task) => task,
            None => return Ok(None),
        };

        // Get events directly from storage
        let task_events = self.task_events.read().await;
        let events = task_events.get(task_id).cloned().unwrap_or_default();

        Ok(Some((task, events)))
    }

    async fn add_task_event(
        &self,
        task_id: &str,
        event: crate::a2a::SendStreamingMessageResult,
    ) -> AgentResult<()> {
        // Add event directly to storage
        let mut task_events = self.task_events.write().await;
        task_events
            .entry(task_id.to_string())
            .or_insert_with(Vec::new)
            .push(event);
        Ok(())
    }

    async fn get_task_events(
        &self,
        task_id: &str,
    ) -> AgentResult<Vec<crate::a2a::SendStreamingMessageResult>> {
        // Get events directly from storage
        let task_events = self.task_events.read().await;
        Ok(task_events.get(task_id).cloned().unwrap_or_default())
    }
}
