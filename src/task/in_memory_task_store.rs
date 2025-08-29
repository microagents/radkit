use crate::errors::{AgentError, AgentResult};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tracing;

use super::task_store::TaskStore;
use crate::a2a::{SendStreamingMessageResult, Task};

/// In-memory implementation of TaskStore.
///
/// This implementation provides:
/// - Multi-tenant security isolation via three-tier storage
/// - Automatic cleanup of empty containers
/// - Development and testing-friendly storage
///
/// Security Model:
/// Tasks are stored in a three-tier hierarchy: app -> user -> task_id -> Task
/// This ensures complete isolation between different apps and users.
///
/// Storage Structure:
/// ```text
/// DashMap<app_name, DashMap<user_id, DashMap<task_id, Task>>>
/// ```
///
/// Performance Characteristics:
/// - O(1) task lookup and storage operations
/// - Memory usage scales with number of tasks
/// - Not suitable for large-scale production (use database backend)
pub struct InMemoryTaskStore {
    /// Secure three-tier task storage: app -> user -> task_id -> Task
    tasks: Arc<DashMap<String, Arc<DashMap<String, Arc<DashMap<String, Task>>>>>>,
    /// A2A events storage by task_id: task_id -> Vec<A2A Events>
    task_events: Arc<DashMap<String, Vec<SendStreamingMessageResult>>>,
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
            tasks: Arc::new(DashMap::new()),
            task_events: Arc::new(DashMap::new()),
        }
    }

    /// Clear all tasks from storage.
    /// Primarily used for testing and development.
    pub async fn clear(&self) {
        self.tasks.clear();
        self.task_events.clear();
    }

    /// Get statistics about the current storage state.
    /// Returns (total_apps, total_users, total_tasks)
    pub async fn stats(&self) -> (usize, usize, usize) {
        let total_apps = self.tasks.len();
        let mut total_users = 0;
        let mut total_tasks = 0;
        
        for app_ref in self.tasks.iter() {
            let app_users = app_ref.value();
            total_users += app_users.len();
            for user_ref in app_users.iter() {
                total_tasks += user_ref.value().len();
            }
        }
        (total_apps, total_users, total_tasks)
    }

    /// Add an A2A event for a specific task
    pub async fn add_task_event(
        &self,
        task_id: &str,
        event: SendStreamingMessageResult,
    ) -> AgentResult<()> {
        self.task_events
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
        Ok(self.task_events.get(task_id).map(|entry| entry.clone()).unwrap_or_default())
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
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                if let Some(task_ref) = user_tasks.get(task_id) {
                    return Ok(Some(task_ref.clone()));
                }
            }
        }
        Ok(None)
    }

    async fn save_task(&self, app_name: &str, user_id: &str, task: &Task) -> AgentResult<()> {
        // Navigate/create the three-tier structure
        let app_users = self.tasks
            .entry(app_name.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));

        let user_tasks = app_users
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));

        // Store the task
        user_tasks.insert(task.id.clone(), task.clone());

        Ok(())
    }

    async fn delete_task(&self, app_name: &str, user_id: &str, task_id: &str) -> AgentResult<()> {
        // Navigate to the task and remove it
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                user_tasks.remove(task_id);

                // Clean up empty user container
                if user_tasks.is_empty() {
                    drop(user_tasks); // Drop the reference before removing
                    app_users.remove(user_id);
                }
            }

            // Clean up empty app container
            if app_users.is_empty() {
                drop(app_users); // Drop the reference before removing
                self.tasks.remove(app_name);
            }
        }

        // Also remove task events
        self.task_events.remove(task_id);

        Ok(())
    }

    async fn list_tasks(
        &self,
        app_name: &str,
        user_id: &str,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<Task>> {
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                let mut result: Vec<Task> = if let Some(context_filter) = context_id {
                    // Filter by context_id
                    user_tasks
                        .iter()
                        .filter(|entry| entry.value().context_id == context_filter)
                        .map(|entry| entry.value().clone())
                        .collect()
                } else {
                    // Return all tasks for this user
                    user_tasks.iter().map(|entry| entry.value().clone()).collect()
                };

                // Sort by creation time (newest first)
                // Since A2A Task doesn't have created_at, we use the task ID creation order as proxy
                result.sort_by(|a, b| b.id.cmp(&a.id));

                return Ok(result);
            }
        }
        Ok(Vec::new())
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
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                return Ok(user_tasks.contains_key(task_id));
            }
        }
        Ok(false)
    }

    async fn append_message(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        message: crate::a2a::Message,
    ) -> AgentResult<()> {
        // Navigate to the task and update it atomically
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                if let Some(mut task_ref) = user_tasks.get_mut(task_id) {
                    task_ref.history.push(message);
                    return Ok(());
                }
            }
        }
        
        Err(AgentError::TaskNotFound {
            task_id: task_id.to_string(),
        })
    }

    async fn append_artifact(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        artifact: crate::a2a::Artifact,
    ) -> AgentResult<()> {
        // Navigate to the task and update it atomically
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                if let Some(mut task_ref) = user_tasks.get_mut(task_id) {
                    task_ref.artifacts.push(artifact);
                    return Ok(());
                }
            }
        }
        
        Err(AgentError::TaskNotFound {
            task_id: task_id.to_string(),
        })
    }

    async fn update_task_status(
        &self,
        app_name: &str,
        user_id: &str,
        task_id: &str,
        status: crate::a2a::TaskStatus,
    ) -> AgentResult<()> {
        // Navigate to the task and update it atomically
        if let Some(app_users) = self.tasks.get(app_name) {
            if let Some(user_tasks) = app_users.get(user_id) {
                if let Some(mut task_ref) = user_tasks.get_mut(task_id) {
                    task_ref.status = status;
                    return Ok(());
                }
            }
        }
        
        Err(AgentError::TaskNotFound {
            task_id: task_id.to_string(),
        })
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
        let events = self.task_events.get(task_id).map(|entry| entry.clone()).unwrap_or_default();

        Ok(Some((task, events)))
    }

    async fn add_task_event(
        &self,
        task_id: &str,
        event: crate::a2a::SendStreamingMessageResult,
    ) -> AgentResult<()> {
        // Add event directly to storage
        self.task_events
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
        Ok(self.task_events.get(task_id).map(|entry| entry.clone()).unwrap_or_default())
    }
}
