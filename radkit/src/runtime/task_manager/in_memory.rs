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
    use super::{
        AgentResult, AuthContext, ListTasksFilter, Message, PaginatedResult, Task, TaskEvent,
        TaskManager,
    };
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
        /// Store `TaskContext` state for multi-turn conversations
        task_contexts: Arc<DashMap<String, String>>, // Store as JSON string
        /// Store skill ID associations for task continuation
        task_skills: Arc<DashMap<String, String>>,
    }

    impl InMemoryTaskManager {
        /// Creates a new `InMemoryTaskManager`.
        #[must_use]
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
            let mut all_tasks: Vec<Task> = self
                .tasks
                .iter()
                .filter(|item| item.key().starts_with(&prefix))
                .map(|item| item.value().clone())
                .filter(|task| filter.context_id.is_none_or(|sid| task.context_id == sid))
                .collect();

            // Sort tasks by ID for consistent ordering
            all_tasks.sort_by(|a, b| a.id.cmp(&b.id));

            // Apply pagination
            let page_size = filter.page_size.unwrap_or(100) as usize;
            let start_offset = filter
                .page_token
                .and_then(|token| token.parse::<usize>().ok())
                .unwrap_or(0);

            let total = all_tasks.len();
            let end_offset = (start_offset + page_size).min(total);
            let items = all_tasks[start_offset..end_offset].to_vec();

            let next_page_token = if end_offset < total {
                Some(end_offset.to_string())
            } else {
                None
            };

            Ok(PaginatedResult {
                items,
                next_page_token,
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
                        std::clone::Clone::clone,
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
                    for event in entry.value() {
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

            // Sort messages deterministically by their message identifiers.
            messages.sort_by(|a, b| a.message_id.cmp(&b.message_id));

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
    use super::{
        AgentResult, AuthContext, ListTasksFilter, Message, PaginatedResult, Task, TaskEvent,
        TaskManager,
    };
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
        #[must_use]
        pub fn new() -> Self {
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
            let mut all_tasks: Vec<Task> = self
                .tasks
                .borrow()
                .iter()
                .filter(|(key, _)| key.starts_with(&prefix))
                .map(|(_, task)| task.clone())
                .filter(|task| filter.context_id.is_none_or(|sid| task.context_id == sid))
                .collect();

            // Sort tasks by ID for consistent ordering
            all_tasks.sort_by(|a, b| a.id.cmp(&b.id));

            // Apply pagination
            let page_size = filter.page_size.unwrap_or(100) as usize;
            let start_offset = filter
                .page_token
                .and_then(|token| token.parse::<usize>().ok())
                .unwrap_or(0);

            let total = all_tasks.len();
            let end_offset = (start_offset + page_size).min(total);
            let items = all_tasks[start_offset..end_offset].to_vec();

            let next_page_token = if end_offset < total {
                Some(end_offset.to_string())
            } else {
                None
            };

            Ok(PaginatedResult {
                items,
                next_page_token,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::context::{AuthContext, TaskContext};
    use crate::runtime::task_manager::{ListTasksFilter, TaskEvent};
    use a2a_types::{
        Message, MessageRole, TaskArtifactUpdateEvent, TaskState, TaskStatus, TaskStatusUpdateEvent,
    };

    fn auth() -> AuthContext {
        AuthContext {
            app_name: "app".into(),
            user_name: "user".into(),
        }
    }

    fn make_message(id: &str, context: &str) -> Message {
        Message {
            kind: "message".into(),
            message_id: id.into(),
            role: MessageRole::Agent,
            parts: Vec::new(),
            context_id: Some(context.into()),
            task_id: None,
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stores_tasks_events_and_context() {
        let manager = InMemoryTaskManager::new();
        let auth_ctx = auth();
        let task = Task {
            id: "task-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Submitted,
                timestamp: None,
                message: None,
            },
            artifacts: Vec::new(),
        };

        manager
            .save_task(&auth_ctx, &task)
            .await
            .expect("save task");

        let retrieved = manager
            .get_task(&auth_ctx, "task-1")
            .await
            .expect("get task")
            .expect("task exists");
        assert_eq!(retrieved.id, task.id);

        // negotiation messages ordering
        let msg_a = make_message("b", "ctx-1");
        let msg_b = make_message("a", "ctx-1");
        manager
            .add_task_event(&auth_ctx, &TaskEvent::Message(msg_a.clone()))
            .await
            .expect("add message");
        manager
            .add_task_event(&auth_ctx, &TaskEvent::Message(msg_b.clone()))
            .await
            .expect("add message");

        let status_event = TaskStatusUpdateEvent {
            kind: a2a_types::STATUS_UPDATE_KIND.to_string(),
            task_id: "task-1".into(),
            context_id: "ctx-1".into(),
            status: TaskStatus {
                state: TaskState::Working,
                timestamp: None,
                message: None,
            },
            is_final: false,
            metadata: None,
        };
        manager
            .add_task_event(&auth_ctx, &TaskEvent::StatusUpdate(status_event))
            .await
            .expect("status");

        let artifact_event = TaskArtifactUpdateEvent {
            kind: a2a_types::ARTIFACT_UPDATE_KIND.to_string(),
            task_id: "task-1".into(),
            context_id: "ctx-1".into(),
            artifact: a2a_types::Artifact {
                artifact_id: "artifact".into(),
                parts: Vec::new(),
                name: None,
                description: None,
                extensions: Vec::new(),
                metadata: None,
            },
            append: None,
            last_chunk: None,
            metadata: None,
        };
        manager
            .add_task_event(&auth_ctx, &TaskEvent::ArtifactUpdate(artifact_event))
            .await
            .expect("artifact");

        let events = manager
            .get_task_events(&auth_ctx, "task-1")
            .await
            .expect("events");
        assert_eq!(events.len(), 2);

        let negotiation = manager
            .get_negotiating_messages(&auth_ctx, "ctx-1")
            .await
            .expect("negotiation");
        assert_eq!(negotiation.len(), 2);
        assert_eq!(negotiation[0].message_id, "a");
        assert_eq!(negotiation[1].message_id, "b");

        let ids = manager
            .list_task_ids(&auth_ctx, Some("ctx-1"))
            .await
            .expect("ids");
        assert_eq!(ids, vec!["task-1".to_string()]);

        let mut context = TaskContext::new();
        context.save_data("flag", &true).expect("save flag");
        manager
            .save_task_context(&auth_ctx, "task-1", &context)
            .await
            .expect("save ctx");
        let restored = manager
            .load_task_context(&auth_ctx, "task-1")
            .await
            .expect("load ctx")
            .expect("context present");
        let flag: Option<bool> = restored.load_data("flag").expect("flag");
        assert_eq!(flag, Some(true));

        manager
            .set_task_skill(&auth_ctx, "task-1", "skill")
            .await
            .expect("set skill");
        let skill = manager
            .get_task_skill(&auth_ctx, "task-1")
            .await
            .expect("get skill");
        assert_eq!(skill.as_deref(), Some("skill"));

        let page = manager
            .list_tasks(
                &auth_ctx,
                &ListTasksFilter {
                    context_id: Some("ctx-1"),
                    page_size: Some(10),
                    page_token: None,
                },
            )
            .await
            .expect("list tasks");
        assert_eq!(page.items.len(), 1);
    }
}
