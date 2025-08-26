use crate::errors::AgentResult;
use async_trait::async_trait;
use serde_json::Value;

use super::session::Session;

/// Trait for session persistence and management.
/// Provides abstraction over different storage backends (in-memory, database, etc.)
#[async_trait]
pub trait SessionService: Send + Sync {
    /// Retrieve a session by app, user, and session ID (always includes merged app/user state)
    async fn get_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<Option<Session>>;

    /// Save a session (create or update) - extracts app/user from session
    async fn save_session(&self, session: &Session) -> AgentResult<()>;

    /// Create a new session with auto-generated ID
    async fn create_session(&self, app_name: String, user_id: String) -> AgentResult<Session>;

    /// Delete a session
    async fn delete_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<()>;

    /// List all sessions for a specific app and user
    async fn list_sessions(&self, app_name: &str, user_id: &str) -> AgentResult<Vec<Session>>;

    /// Check if a session exists (SECURE: requires app and user)
    async fn session_exists(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<bool>;

    /// Update session activity timestamp (SECURE: requires app and user)
    async fn touch_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<()>;

    // State management methods

    /// Update app state immediately (like Python ADK)
    async fn update_app_state(&self, app_name: &str, key: &str, value: Value) -> AgentResult<()>;

    /// Update user state immediately (like Python ADK)
    async fn update_user_state(
        &self,
        app_name: &str,
        user_id: &str,
        key: &str,
        value: Value,
    ) -> AgentResult<()>;
}
