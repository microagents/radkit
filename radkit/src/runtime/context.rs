//! Execution context for agent tasks and skill handlers.
//!
//! This module provides context types that carry execution state across
//! skill handler invocations during agent task execution.
//!
//! # Overview
//!
//! - [`Context`]: Immutable execution context shared across skill handlers
//! - [`TaskContext`]: Mutable context for persisting state between turns
//!
//! # Examples
//!
//! ```ignore
//! use radkit::context::{Context, TaskContext};
//!
//! // Create execution context
//! let ctx = Context::new()?;
//!
//! // In a skill handler, persist data between turns
//! let mut task_ctx = TaskContext::new()?;
//! task_ctx.save_data("user_preference", &"dark_mode")?;
//!
//! // Later, retrieve the data
//! let preference: Option<String> = task_ctx.load_data("user_preference")?;
//! ```

#[cfg(feature = "runtime")]
use crate::agent::Artifact;
use crate::errors::AgentError;
#[cfg(feature = "runtime")]
use crate::errors::AgentResult;
#[cfg(feature = "runtime")]
use crate::models::utils;
#[cfg(feature = "runtime")]
use crate::models::{Content, Role};
#[cfg(feature = "runtime")]
use crate::runtime::core::event_bus::TaskEventBus;
#[cfg(feature = "runtime")]
use crate::runtime::core::status_mapper;
#[cfg(feature = "runtime")]
use crate::runtime::task_manager::{TaskEvent, TaskManager};
#[cfg(feature = "runtime")]
use a2a_types::{TaskArtifactUpdateEvent, TaskState, TaskStatus};
#[cfg(feature = "runtime")]
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(feature = "runtime")]
use std::sync::Arc;

/// Authentication and tenancy context for the current execution.
///
/// This struct holds authentication and tenancy information that is used
/// to namespace operations in services like [`MemoryService`](crate::runtime::memory::MemoryService)
/// and [`TaskManager`](crate::runtime::task_manager::TaskManager).
///
/// # Multi-tenancy
///
/// All runtime services use `AuthContext` to ensure data isolation between
/// different applications and users. This guarantees that one user or application
/// cannot access another's data.
///
/// # Examples
///
/// ```
/// use radkit::runtime::context::AuthContext;
///
/// let auth_ctx = AuthContext {
///     app_name: "my-app".to_string(),
///     user_name: "alice".to_string(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The name of the application or agent.
    pub app_name: String,
    /// The name of the current user.
    pub user_name: String,
}

/// Execution context shared across skill handlers for a single agent task.
///
/// Provides immutable access to execution state including tasks and
/// configuration. This context is shared across all skill handlers
/// processing the same agent task.
pub struct Context {
    pub auth: AuthContext,
}

/// Mutable handle that allows skills to persist state between turns.
#[derive(Debug, Default, Serialize, Deserialize)]
struct TaskContextState {
    #[serde(default)]
    state: HashMap<String, serde_json::Value>,
    #[serde(default)]
    slot: Option<serde_json::Value>,
}

/// Task context provides a key-value store for skills to save and load
/// data across multiple invocations within the same task, along with helper
/// methods for streaming progress updates during execution.
pub struct TaskContext {
    persisted: TaskContextState,
    #[cfg(feature = "runtime")]
    auth: Option<AuthContext>,
    #[cfg(feature = "runtime")]
    task_manager: Option<Arc<dyn TaskManager>>,
    #[cfg(feature = "runtime")]
    task_id: Option<String>,
    #[cfg(feature = "runtime")]
    context_id: Option<String>,
    #[cfg(feature = "runtime")]
    event_bus: Option<Arc<TaskEventBus>>,
}

impl Context {
    /// Creates a new execution context.
    #[must_use]
    pub const fn new(auth: AuthContext) -> Self {
        Self { auth }
    }
}

impl Default for TaskContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskContext {
    /// Creates a new, empty task context.
    #[must_use]
    pub fn new() -> Self {
        Self {
            persisted: TaskContextState::default(),
            #[cfg(feature = "runtime")]
            auth: None,
            #[cfg(feature = "runtime")]
            task_manager: None,
            #[cfg(feature = "runtime")]
            task_id: None,
            #[cfg(feature = "runtime")]
            context_id: None,
            #[cfg(feature = "runtime")]
            event_bus: None,
        }
    }

    /// Internal helper for constructing a context with runtime handles attached.
    #[cfg(feature = "runtime")]
    pub(crate) fn for_task(
        auth: AuthContext,
        task_manager: Arc<dyn TaskManager>,
        event_bus: Arc<TaskEventBus>,
        context_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        let mut ctx = Self::new();
        ctx.attach_handles(auth, task_manager, event_bus, context_id, task_id);
        ctx
    }

    /// Re-attaches runtime handles after a context has been deserialized.
    #[cfg(feature = "runtime")]
    pub(crate) fn attach_handles(
        &mut self,
        auth: AuthContext,
        task_manager: Arc<dyn TaskManager>,
        event_bus: Arc<TaskEventBus>,
        context_id: impl Into<String>,
        task_id: impl Into<String>,
    ) {
        self.auth = Some(auth);
        self.task_manager = Some(task_manager);
        self.context_id = Some(context_id.into());
        self.task_id = Some(task_id.into());
        self.event_bus = Some(event_bus);
    }

    /// Saves data to the task context under the given key.
    ///
    /// The value must be serializable. If a value already exists for the key,
    /// it will be replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if the value cannot be serialized to JSON.
    pub fn save_data<T>(&mut self, key: &str, value: &T) -> Result<(), AgentError>
    where
        T: Serialize,
    {
        let serialized =
            serde_json::to_value(value).map_err(|e| AgentError::ContextError(e.to_string()))?;
        self.persisted.state.insert(key.to_string(), serialized);
        Ok(())
    }

    /// Loads data from the task context for the given key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist. The data is deserialized
    /// into the target type `T`.
    ///
    /// # Errors
    ///
    /// Returns an error if the stored value cannot be deserialized into type T.
    pub fn load_data<T>(&self, key: &str) -> Result<Option<T>, AgentError>
    where
        T: DeserializeOwned,
    {
        match self.persisted.state.get(key) {
            Some(value) => {
                let deserialized = serde_json::from_value(value.clone())
                    .map_err(|e| AgentError::ContextError(e.to_string()))?;
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    /// Stores the pending input slot that the runtime should expect on continuation.
    ///
    /// Note: This is public when the `test-support` feature is enabled for testing purposes.
    #[cfg(all(feature = "runtime", not(any(test, feature = "test-support"))))]
    pub(crate) fn set_pending_slot(&mut self, slot: crate::agent::SkillSlot) {
        self.persisted.slot = Some(slot.into_value());
    }

    /// Stores the pending input slot that the runtime should expect on continuation.
    ///
    /// Note: This is public when the `test-support` feature is enabled for testing purposes.
    #[cfg(all(feature = "runtime", any(test, feature = "test-support")))]
    pub fn set_pending_slot(&mut self, slot: crate::agent::SkillSlot) {
        self.persisted.slot = Some(slot.into_value());
    }

    /// Clears any previously stored input slot.
    #[cfg(feature = "runtime")]
    pub(crate) fn clear_pending_slot(&mut self) {
        self.persisted.slot = None;
    }

    /// Loads and deserializes the currently expected input slot into the desired type.
    ///
    /// # Errors
    ///
    /// Returns an error if the slot value cannot be deserialized into type T.
    pub fn load_slot<T>(&self) -> Result<Option<T>, AgentError>
    where
        T: DeserializeOwned,
    {
        match &self.persisted.slot {
            Some(value) => {
                let slot: T = serde_json::from_value(value.clone())
                    .map_err(|e| AgentError::SkillSlot(e.to_string()))?;
                Ok(Some(slot))
            }
            None => Ok(None),
        }
    }

    /// Returns the raw slot value, if set.
    pub fn current_slot(&self) -> Option<crate::agent::SkillSlot> {
        self.persisted
            .slot
            .clone()
            .map(crate::agent::SkillSlot::from_value_unchecked)
    }

    /// Sends an intermediate status update (`TaskState::Working`, final=false).
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime handles are not available or the task event cannot be added.
    #[cfg(feature = "runtime")]
    pub async fn send_intermediate_update(
        &mut self,
        message: impl Into<Content>,
    ) -> AgentResult<()> {
        let (auth, task_manager, task_id, context_id) = self.handles()?;

        let status = TaskStatus {
            state: TaskState::Working,
            timestamp: Some(Utc::now().to_rfc3339()),
            message: Some(utils::create_a2a_message(
                Some(&context_id),
                Some(&task_id),
                Role::Assistant,
                message.into(),
            )),
        };

        let event = status_mapper::create_status_update_event(&task_id, &context_id, status, false);
        let task_event = TaskEvent::StatusUpdate(event);

        task_manager.add_task_event(&auth, &task_event).await?;
        self.publish_to_bus(&task_event);
        Ok(())
    }

    /// Sends a partial artifact update (`is_final=false`).
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime handles are not available or the task event cannot be added.
    #[cfg(feature = "runtime")]
    pub async fn send_partial_artifact(&mut self, artifact: Artifact) -> AgentResult<()> {
        let (auth, task_manager, task_id, context_id) = self.handles()?;

        let a2a_artifact = utils::artifact_to_a2a(&artifact);
        let event = TaskArtifactUpdateEvent {
            kind: a2a_types::ARTIFACT_UPDATE_KIND.to_string(),
            task_id,
            context_id,
            artifact: a2a_artifact,
            append: None,
            last_chunk: Some(false),
            metadata: None,
        };

        let task_event = TaskEvent::ArtifactUpdate(event);
        task_manager.add_task_event(&auth, &task_event).await?;
        self.publish_to_bus(&task_event);
        Ok(())
    }

    #[cfg(feature = "runtime")]
    fn handles(&self) -> Result<(AuthContext, Arc<dyn TaskManager>, String, String), AgentError> {
        let auth = self
            .auth
            .as_ref()
            .ok_or_else(|| AgentError::ContextError("TaskContext missing auth context".into()))?
            .clone();
        let task_manager = self
            .task_manager
            .as_ref()
            .ok_or_else(|| AgentError::ContextError("TaskContext missing task manager".into()))?
            .clone();
        let task_id = self
            .task_id
            .as_ref()
            .ok_or_else(|| AgentError::ContextError("TaskContext missing task_id".into()))?
            .clone();
        let context_id = self
            .context_id
            .as_ref()
            .ok_or_else(|| AgentError::ContextError("TaskContext missing context_id".into()))?
            .clone();

        Ok((auth, task_manager, task_id, context_id))
    }

    #[cfg(feature = "runtime")]
    fn publish_to_bus(&self, event: &TaskEvent) {
        if let Some(bus) = &self.event_bus {
            bus.publish(event);
        }
    }
}

impl Serialize for TaskContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.persisted.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TaskContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let persisted = TaskContextState::deserialize(deserializer)?;
        Ok(Self {
            persisted,
            #[cfg(feature = "runtime")]
            auth: None,
            #[cfg(feature = "runtime")]
            task_manager: None,
            #[cfg(feature = "runtime")]
            task_id: None,
            #[cfg(feature = "runtime")]
            context_id: None,
            #[cfg(feature = "runtime")]
            event_bus: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::SkillSlot;

    #[test]
    fn task_context_persists_arbitrary_data() {
        let mut ctx = TaskContext::new();
        ctx.save_data("count", &42u32).expect("save data");

        let value: Option<u32> = ctx.load_data("count").expect("load data");
        assert_eq!(value, Some(42));

        let missing: Option<u32> = ctx.load_data("missing").expect("load data");
        assert!(missing.is_none());
    }

    #[test]
    fn task_context_slot_roundtrip() {
        #[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        enum SlotState {
            Awaiting(String),
        }

        let mut ctx = TaskContext::new();
        let slot = SkillSlot::new(SlotState::Awaiting("info".into()));
        ctx.persisted.slot = Some(serde_json::to_value(&slot).unwrap());

        let reconstructed: Option<SlotState> = ctx.load_slot().expect("load slot");
        assert_eq!(reconstructed, Some(SlotState::Awaiting("info".into())));
        assert!(ctx.current_slot().is_some());
    }

    #[cfg(feature = "runtime")]
    #[tokio::test]
    async fn task_context_streaming_methods() {
        use crate::runtime::core::event_bus::TaskEventBus;
        use crate::runtime::task_manager::{InMemoryTaskManager, TaskManager};
        use std::sync::Arc;

        let auth = AuthContext {
            app_name: "app".into(),
            user_name: "user".into(),
        };
        let task_manager = Arc::new(InMemoryTaskManager::new());
        let event_bus = Arc::new(TaskEventBus::new());

        let mut ctx = TaskContext::for_task(
            auth.clone(),
            task_manager.clone(),
            event_bus.clone(),
            "ctx-1",
            "task-1",
        );

        // Test intermediate update
        ctx.send_intermediate_update("Working...").await.unwrap();

        let events = task_manager.get_task_events(&auth, "task-1").await.unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            TaskEvent::StatusUpdate(event) => {
                assert_eq!(event.status.state, TaskState::Working);
                assert!(!event.is_final);
            }
            _ => panic!("Expected StatusUpdate"),
        }

        // Test partial artifact
        let artifact = Artifact::from_text("partial.txt", "some data");
        ctx.send_partial_artifact(artifact).await.unwrap();

        let events = task_manager.get_task_events(&auth, "task-1").await.unwrap();
        assert_eq!(events.len(), 2);
        match &events[1] {
            TaskEvent::ArtifactUpdate(event) => {
                assert_eq!(event.artifact.artifact_id, "partial.txt");
                assert_eq!(event.last_chunk, Some(false));
            }
            _ => panic!("Expected ArtifactUpdate"),
        }
    }
}
