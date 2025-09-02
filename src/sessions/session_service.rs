use crate::errors::AgentResult;
use async_trait::async_trait;
use serde_json::Value;

use super::session::Session;
use super::session_event::SessionEvent;

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

    // ===== New Unified Event System Methods =====
    // These methods are part of Phase 1 migration - they work alongside existing methods

    /// Store an event in the persistence layer (pure storage)
    /// This is used by EventProcessor for clean separation of concerns
    async fn store_event(&self, event: &SessionEvent) -> AgentResult<()> {
        // Default implementation: no-op (for backward compatibility)
        let _ = event;
        Ok(())
    }

    /// Apply state changes from StateChanged events
    /// This is used by EventProcessor for state side effects
    async fn apply_state_change(&self, event: &SessionEvent) -> AgentResult<()> {
        // Default implementation: no-op (for backward compatibility)
        let _ = event;
        Ok(())
    }

    // Note: Task reconstruction methods removed - this is business logic, not persistence
    // Use EventProcessor or calling code to build Task objects from events

    /// Get all events for a session (for debugging and business logic)
    async fn get_session_events(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<Vec<SessionEvent>> {
        // Default implementation: empty events
        let _ = (app_name, user_id, session_id);
        Ok(Vec::new())
    }

    // Note: subscribe_to_events removed - this is business logic, use EventBus directly
}
