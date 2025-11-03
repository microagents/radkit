//! Task management service for agent execution.
//!
//! This module provides task persistence and event tracking functionality.
//! The [`TaskManager`] trait defines the interface for storing and retrieving
//! task state and events throughout the agent execution lifecycle.
//!
//! # Event Sourcing
//!
//! The task manager uses an event sourcing pattern where task state is stored
//! separately from its event history. This allows for efficient queries and
//! reconstruction of full task history when needed.
//!
//! # Multi-tenancy
//!
//! All operations are namespaced by [`AuthContext`](crate::runtime::context::AuthContext),
//! ensuring data isolation between different users and applications.

pub mod in_memory;

pub use in_memory::InMemoryTaskManager;

use crate::compat::{MaybeSend, MaybeSync};
use crate::errors::AgentResult;
use crate::runtime::context::AuthContext;
use a2a_types::{Artifact, Message, TaskArtifactUpdateEvent, TaskStatus, TaskStatusUpdateEvent};

// ============================================================================
// Data Structures
// ============================================================================

/// Represents the state of a single task, mirroring the A2A Task object, but
/// stored without the conversational history for efficiency.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    /// Corresponds to A2A `contextId`.
    pub context_id: String,
    pub status: TaskStatus,
    pub artifacts: Vec<Artifact>,
}

/// Represents a significant event that occurred during a task's lifecycle.
/// This enum can be converted from and to the various A2A event types.
#[derive(Debug, Clone)]
pub enum TaskEvent {
    StatusUpdate(TaskStatusUpdateEvent),
    ArtifactUpdate(TaskArtifactUpdateEvent),
    Message(Message),
}

/// Filter for listing tasks, enabling pagination.
#[derive(Debug, Default)]
pub struct ListTasksFilter<'a> {
    pub context_id: Option<&'a str>,
    pub page_size: Option<u32>,
    pub page_token: Option<&'a str>,
}

/// Represents a paginated result set.
#[derive(Debug)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}

// ============================================================================
// TaskManager Trait
// ============================================================================

/// A stateful Data Access Object (DAO) for persisting and retrieving all data
/// related to agent interactions.
#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
pub trait TaskManager: MaybeSend + MaybeSync {
    /// Retrieves a single task by its ID, scoped to the `AuthContext`.
    async fn get_task(&self, auth_ctx: &AuthContext, task_id: &str) -> AgentResult<Option<Task>>;

    /// Lists tasks for the given user/app, with optional filtering and pagination.
    /// This is the primary method for retrieving tasks associated with a "session" (contextId).
    async fn list_tasks(
        &self,
        auth_ctx: &AuthContext,
        filter: &ListTasksFilter<'_>,
    ) -> AgentResult<PaginatedResult<Task>>;

    /// Stores or updates a task's state. This is an upsert operation.
    async fn save_task(&self, auth_ctx: &AuthContext, task: &Task) -> AgentResult<()>;

    /// Appends an event to a task's history.
    ///
    /// The `task_id` and `context_id` are extracted from the event itself (Message,
    /// `StatusUpdate`, or `ArtifactUpdate` all contain these fields). For negotiation
    /// messages where `task_id` is None, events are stored under a synthetic key
    /// `_negotiation:{context_id}`.
    async fn add_task_event(&self, auth_ctx: &AuthContext, event: &TaskEvent) -> AgentResult<()>;

    /// Retrieves all events for a specific task, ordered chronologically.
    async fn get_task_events(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
    ) -> AgentResult<Vec<TaskEvent>>;

    /// Retrieves all Message events across all tasks within a context.
    /// This is useful for building the full conversation history for LLM context.
    async fn get_negotiating_messages(
        &self,
        auth_ctx: &AuthContext,
        context_id: &str,
    ) -> AgentResult<Vec<Message>>;

    /// Lists all task IDs for the given user/app, optionally filtered by `context_id`.
    /// Returns all task IDs without pagination.
    async fn list_task_ids(
        &self,
        auth_ctx: &AuthContext,
        context_id: Option<&str>,
    ) -> AgentResult<Vec<String>>;

    /// Saves the task context state for multi-turn conversations.
    ///
    /// This allows skills to persist data between `on_request` and `on_input_received` calls.
    /// The context is namespaced by `AuthContext` to ensure tenant isolation.
    async fn save_task_context(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        context: &crate::runtime::context::TaskContext,
    ) -> AgentResult<()>;

    /// Loads the task context state for a given task.
    ///
    /// Returns `None` if no context has been saved yet.
    async fn load_task_context(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
    ) -> AgentResult<Option<crate::runtime::context::TaskContext>>;

    /// Associates a skill ID with a task for continuation purposes.
    ///
    /// This is stored when a task is created so we know which skill to call on continuation.
    async fn set_task_skill(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        skill_id: &str,
    ) -> AgentResult<()>;

    /// Retrieves the skill ID associated with a task.
    ///
    /// Returns `None` if no skill has been associated with this task.
    async fn get_task_skill(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
    ) -> AgentResult<Option<String>>;
}
