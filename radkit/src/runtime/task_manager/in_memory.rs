//! In-memory implementation of the `TaskManager` trait.
//!
//! This module provides both native (thread-safe) and WASM (single-threaded)
//! implementations of the task manager using in-memory storage.

use crate::errors::AgentResult;
use crate::runtime::context::AuthContext;
use crate::runtime::task_manager::{
    ListTasksFilter, PaginatedResult, Task, TaskEvent, TaskManager,
};
use a2a_types::Message;

// ============================================================================
// Native Implementation (Thread-Safe)
// ============================================================================

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
mod native {
    use super::*;
    use dashmap::DashMap;
    use std::sync::Arc;

    /// An in-memory, thread-safe implementation of the [`TaskManager`].
    ///
    /// This implementation uses `DashMap` for concurrent access, making it suitable
    /// for the multi-threaded `tokio` runtime on native targets.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::runtime::InMemoryTaskManager;
    /// use radkit::runtime::task_manager::{Task, TaskManager};
    ///
    /// let manager = InMemoryTaskManager::new();
    /// let auth_ctx = AuthContext {
    ///     app_name: "my-app".to_string(),
    ///     user_name: "user1".to_string(),
    /// };
    ///
    /// let task = Task {
    ///     id: "task-1".to_string(),
    ///     context_id: "ctx-1".to_string(),
    ///     status: TaskStatus::default(),
    ///     artifacts: vec![],
    /// };
    ///
    /// manager.save_task(&auth_ctx, &task).await?;
    /// ```
    #[derive(Debug, Default)]
    pub struct InMemoryTaskManager {
        tasks: Arc<DashMap<String, Task>>,
        events: Arc<DashMap<String, Vec<TaskEvent>>>,
        /// Store TaskContext state for multi-turn conversations
        task_contexts: Arc<DashMap<String, String>>, // Store as JSON string
        /// Store skill ID associations for task continuation
        task_skills: Arc<DashMap<String, String>>,
    }

    impl InMemoryTaskManager {
        /// Creates a new `InMemoryTaskManager`.
        pub fn new() -> Self {
            Self::default()
        }

        /// Creates a namespaced key from the auth context and the original key.
        fn get_namespaced_key(&self, auth_ctx: &AuthContext, key: &str) -> String {
            format!("{}:{}:{}", auth_ctx.app_name, auth_ctx.user_name, key)
        }
    }

    #[async_trait::async_trait]
    impl TaskManager for InMemoryTaskManager {
        async fn get_task(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<Task>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self.tasks.get(&key).map(|t| t.value().clone()))
        }

        async fn list_tasks(
            &self,
            auth_ctx: &AuthContext,
            filter: &ListTasksFilter<'_>,
        ) -> AgentResult<PaginatedResult<Task>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let all_tasks: Vec<Task> = self
                .tasks
                .iter()
                .filter(|item| item.key().starts_with(&prefix))
                .map(|item| item.value().clone())
                .filter(|task| filter.context_id.map_or(true, |sid| task.context_id == sid))
                .collect();

            // Note: In-memory version does not support pagination tokens.
            Ok(PaginatedResult {
                items: all_tasks,
                next_page_token: None,
            })
        }

        async fn save_task(&self, auth_ctx: &AuthContext, task: &Task) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, &task.id);
            self.tasks.insert(key, task.clone());
            Ok(())
        }

        async fn add_task_event(
            &self,
            auth_ctx: &AuthContext,
            event: &TaskEvent,
        ) -> AgentResult<()> {
            // Extract task_id from the event itself
            let event_key = match event {
                TaskEvent::Message(msg) => {
                    // For negotiation messages (task_id is None), use synthetic key
                    msg.task_id.as_ref().map_or_else(
                        || {
                            format!(
                                "_negotiation:{}",
                                msg.context_id.as_deref().unwrap_or("default")
                            )
                        },
                        |tid| tid.clone(),
                    )
                }
                TaskEvent::StatusUpdate(update) => update.task_id.clone(),
                TaskEvent::ArtifactUpdate(update) => update.task_id.clone(),
            };

            let key = self.get_namespaced_key(auth_ctx, &event_key);
            let mut event_log = self.events.entry(key).or_default();
            event_log.push(event.clone());
            Ok(())
        }

        async fn get_task_events(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Vec<TaskEvent>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self
                .events
                .get(&key)
                .map_or_else(Vec::new, |v| v.value().clone()))
        }

        async fn get_negotiating_messages(
            &self,
            auth_ctx: &AuthContext,
            context_id: &str,
        ) -> AgentResult<Vec<Message>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let mut messages = Vec::new();

            // Iterate through all events and filter messages by context_id
            for entry in self.events.iter() {
                if entry.key().starts_with(&prefix) {
                    for event in entry.value().iter() {
                        if let TaskEvent::Message(msg) = event {
                            if msg.context_id.as_deref() == Some(context_id)
                                && msg.task_id.is_none()
                            {
                                messages.push(msg.clone());
                            }
                        }
                    }
                }
            }

            // TODO: Sort messages chronologically by timestamp if available
            Ok(messages)
        }

        async fn list_task_ids(
            &self,
            auth_ctx: &AuthContext,
            context_id: Option<&str>,
        ) -> AgentResult<Vec<String>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let task_ids: Vec<String> = self
                .tasks
                .iter()
                .filter(|item| item.key().starts_with(&prefix))
                .map(|item| item.value().clone())
                .filter(|task| context_id.map_or(true, |cid| task.context_id == cid))
                .map(|task| task.id)
                .collect();

            Ok(task_ids)
        }

        async fn save_task_context(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
            context: &crate::runtime::context::TaskContext,
        ) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            // Serialize TaskContext to JSON
            let json_str = serde_json::to_string(context).map_err(|e| {
                crate::errors::AgentError::Serialization {
                    format: "json".to_string(),
                    reason: format!("Failed to serialize TaskContext: {}", e),
                }
            })?;
            self.task_contexts.insert(key, json_str);
            Ok(())
        }

        async fn load_task_context(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<crate::runtime::context::TaskContext>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            match self.task_contexts.get(&key) {
                Some(json_ref) => {
                    let context = serde_json::from_str(json_ref.value()).map_err(|e| {
                        crate::errors::AgentError::Serialization {
                            format: "json".to_string(),
                            reason: format!("Failed to deserialize TaskContext: {}", e),
                        }
                    })?;
                    Ok(Some(context))
                }
                None => Ok(None),
            }
        }

        async fn set_task_skill(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
            skill_id: &str,
        ) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            self.task_skills.insert(key, skill_id.to_string());
            Ok(())
        }

        async fn get_task_skill(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<String>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self.task_skills.get(&key).map(|s| s.value().clone()))
        }
    }
}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub use native::InMemoryTaskManager;

// ============================================================================
// WASM Implementation (Single-Threaded)
// ============================================================================

#[cfg(all(target_os = "wasi", target_env = "p1"))]
mod wasm {
    use super::{Task, TaskEvent, AuthContext, TaskManager, AgentResult, ListTasksFilter, PaginatedResult, Message};
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// An in-memory, single-threaded implementation of the [`TaskManager`] for WASM.
    ///
    /// This implementation uses `RefCell<HashMap>` for interior mutability, suitable
    /// for the single-threaded WASM environment.
    #[derive(Debug, Default)]
    pub struct InMemoryTaskManager {
        tasks: RefCell<HashMap<String, Task>>,
        events: RefCell<HashMap<String, Vec<TaskEvent>>>,
        /// Store `TaskContext` state for multi-turn conversations
        task_contexts: RefCell<HashMap<String, String>>, // Store as JSON string
        /// Store skill ID associations for task continuation
        task_skills: RefCell<HashMap<String, String>>,
    }

    impl InMemoryTaskManager {
        /// Creates a new `InMemoryTaskManager`.
        #[must_use] pub fn new() -> Self {
            Self::default()
        }

        /// Creates a namespaced key from the auth context and the original key.
        fn get_namespaced_key(&self, auth_ctx: &AuthContext, key: &str) -> String {
            format!("{}:{}:{}", auth_ctx.app_name, auth_ctx.user_name, key)
        }
    }

    #[async_trait::async_trait(?Send)]
    impl TaskManager for InMemoryTaskManager {
        async fn get_task(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<Task>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self.tasks.borrow().get(&key).cloned())
        }

        async fn list_tasks(
            &self,
            auth_ctx: &AuthContext,
            filter: &ListTasksFilter<'_>,
        ) -> AgentResult<PaginatedResult<Task>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let all_tasks: Vec<Task> = self
                .tasks
                .borrow()
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, task)| task.clone())
                .filter(|task| filter.context_id.is_none_or(|sid| task.context_id == sid))
                .collect();

            Ok(PaginatedResult {
                items: all_tasks,
                next_page_token: None,
            })
        }

        async fn save_task(&self, auth_ctx: &AuthContext, task: &Task) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, &task.id);
            self.tasks.borrow_mut().insert(key, task.clone());
            Ok(())
        }

        async fn add_task_event(
            &self,
            auth_ctx: &AuthContext,
            event: &TaskEvent,
        ) -> AgentResult<()> {
            // Extract task_id from the event itself
            let event_key = match event {
                TaskEvent::Message(msg) => {
                    // For negotiation messages (task_id is None), use synthetic key
                    msg.task_id.as_ref().map_or_else(
                        || {
                            format!(
                                "_negotiation:{}",
                                msg.context_id.as_deref().unwrap_or("default")
                            )
                        },
                        std::clone::Clone::clone,
                    )
                }
                TaskEvent::StatusUpdate(update) => update.task_id.clone(),
                TaskEvent::ArtifactUpdate(update) => update.task_id.clone(),
            };

            let key = self.get_namespaced_key(auth_ctx, &event_key);
            self.events
                .borrow_mut()
                .entry(key)
                .or_default()
                .push(event.clone());
            Ok(())
        }

        async fn get_task_events(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Vec<TaskEvent>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self.events.borrow().get(&key).cloned().unwrap_or_default())
        }

        async fn get_negotiating_messages(
            &self,
            auth_ctx: &AuthContext,
            context_id: &str,
        ) -> AgentResult<Vec<Message>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let mut messages = Vec::new();

            // Iterate through all events and filter messages by context_id
            for (key, events) in self.events.borrow().iter() {
                if key.starts_with(&prefix) {
                    for event in events {
                        if let TaskEvent::Message(msg) = event {
                            if msg.context_id.as_deref() == Some(context_id)
                                && msg.task_id.is_none()
                            {
                                messages.push(msg.clone());
                            }
                        }
                    }
                }
            }

            // TODO: Sort messages chronologically by timestamp if available
            Ok(messages)
        }

        async fn list_task_ids(
            &self,
            auth_ctx: &AuthContext,
            context_id: Option<&str>,
        ) -> AgentResult<Vec<String>> {
            let prefix = format!("{}:{}:", auth_ctx.app_name, auth_ctx.user_name);
            let task_ids: Vec<String> = self
                .tasks
                .borrow()
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, task)| task.clone())
                .filter(|task| context_id.is_none_or(|cid| task.context_id == cid))
                .map(|task| task.id)
                .collect();

            Ok(task_ids)
        }

        async fn save_task_context(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
            context: &crate::runtime::context::TaskContext,
        ) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            // Serialize TaskContext to JSON
            let json_str = serde_json::to_string(context).map_err(|e| {
                crate::errors::AgentError::Serialization {
                    format: "json".to_string(),
                    reason: format!("Failed to serialize TaskContext: {e}"),
                }
            })?;
            self.task_contexts.borrow_mut().insert(key, json_str);
            Ok(())
        }

        async fn load_task_context(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<crate::runtime::context::TaskContext>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            match self.task_contexts.borrow().get(&key) {
                Some(json_str) => {
                    let context = serde_json::from_str(json_str).map_err(|e| {
                        crate::errors::AgentError::Serialization {
                            format: "json".to_string(),
                            reason: format!("Failed to deserialize TaskContext: {e}"),
                        }
                    })?;
                    Ok(Some(context))
                }
                None => Ok(None),
            }
        }

        async fn set_task_skill(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
            skill_id: &str,
        ) -> AgentResult<()> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            self.task_skills
                .borrow_mut()
                .insert(key, skill_id.to_string());
            Ok(())
        }

        async fn get_task_skill(
            &self,
            auth_ctx: &AuthContext,
            task_id: &str,
        ) -> AgentResult<Option<String>> {
            let key = self.get_namespaced_key(auth_ctx, task_id);
            Ok(self.task_skills.borrow().get(&key).cloned())
        }
    }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub use wasm::InMemoryTaskManager;
