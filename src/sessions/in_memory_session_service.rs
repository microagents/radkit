use crate::errors::AgentResult;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use tracing;

use super::session::Session;
use super::session_service::SessionService;

/// In-memory implementation of SessionService.
/// Suitable for development, testing, and single-instance deployments.
/// Not recommended for production multi-instance deployments.
/// SECURITY: Sessions are stored as app -> user -> session_id -> Session to prevent cross-access
pub struct InMemorySessionService {
    /// SECURE session storage: app -> user -> session_id -> Session
    sessions: Arc<DashMap<String, Arc<DashMap<String, Arc<DashMap<String, Session>>>>>>,
    /// User state storage: app -> user -> key -> value
    user_state: Arc<DashMap<String, Arc<DashMap<String, Arc<DashMap<String, Value>>>>>>,
    /// App state storage: app -> key -> value
    app_state: Arc<DashMap<String, Arc<DashMap<String, Value>>>>,
}

impl InMemorySessionService {
    /// Create a new in-memory session service
    ///
    /// ⚠️  **WARNING: NOT FOR PRODUCTION USE** ⚠️
    /// This is an in-memory implementation suitable for development and testing only.
    /// Sessions and state will accumulate indefinitely causing memory leaks.
    /// Use a proper database-backed SessionService implementation for production.
    pub fn new() -> Self {
        tracing::warn!(
            "🚨 InMemorySessionService created - NOT SUITABLE FOR PRODUCTION! Sessions will accumulate indefinitely. Use database-backed implementation instead."
        );

        Self {
            sessions: Arc::new(DashMap::new()),
            user_state: Arc::new(DashMap::new()),
            app_state: Arc::new(DashMap::new()),
        }
    }

    /// Clear all sessions (useful for testing)
    pub async fn clear(&self) {
        self.sessions.clear();
        self.user_state.clear();
        self.app_state.clear();
    }

    /// Save a session without any state processing (internal helper)
    async fn save_session_raw(&self, session: &Session) -> AgentResult<()> {
        // Store the session in the secure three-level structure
        let app_users = self.sessions
            .entry(session.app_name.clone())
            .or_insert_with(|| Arc::new(DashMap::new()));
        let user_sessions = app_users
            .entry(session.user_id.clone())
            .or_insert_with(|| Arc::new(DashMap::new()));
        user_sessions.insert(session.id.clone(), session.clone());

        Ok(())
    }

    /// Merge app and user state into a session (like Python's _merge_state)
    async fn merge_state(&self, session: &mut Session) -> AgentResult<()> {
        // Get app state for this app and add with app: prefix
        if let Some(app_state) = self.app_state.get(&session.app_name) {
            for entry in app_state.iter() {
                session.state.insert(format!("app:{}", entry.key()), entry.value().clone());
            }
        }

        // Get user state for this user in this app and add with user: prefix
        if let Some(users) = self.user_state.get(&session.app_name) {
            if let Some(user_state) = users.get(&session.user_id) {
                for entry in user_state.iter() {
                    session.state.insert(format!("user:{}", entry.key()), entry.value().clone());
                }
            }
        }

        Ok(())
    }
}

impl Default for InMemorySessionService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionService for InMemorySessionService {
    async fn get_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<Option<Session>> {
        if let Some(app_users) = self.sessions.get(app_name) {
            if let Some(user_sessions) = app_users.get(user_id) {
                if let Some(session_ref) = user_sessions.get(session_id) {
                    let mut merged_session = session_ref.clone();
                    self.merge_state(&mut merged_session).await?;
                    return Ok(Some(merged_session));
                }
            }
        }
        Ok(None)
    }

    async fn save_session(&self, session: &Session) -> AgentResult<()> {
        // Strip merged state before storing (keep only session-level state)
        let mut raw_session = session.clone();
        raw_session
            .state
            .retain(|key, _| !key.starts_with("app:") && !key.starts_with("user:"));

        self.save_session_raw(&raw_session).await
    }

    async fn create_session(&self, app_name: String, user_id: String) -> AgentResult<Session> {
        let session = Session::new(app_name, user_id);
        self.save_session_raw(&session).await?;
        // Return merged version like Python does
        let mut merged_session = session.clone();
        self.merge_state(&mut merged_session).await?;
        Ok(merged_session)
    }

    async fn delete_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<()> {
        // Remove the session itself from the structure
        if let Some(app_users) = self.sessions.get(app_name) {
            if let Some(user_sessions) = app_users.get(user_id) {
                user_sessions.remove(session_id);

                // Clean up empty user if no sessions left
                if user_sessions.is_empty() {
                    drop(user_sessions); // Drop the reference before removing
                    app_users.remove(user_id);
                }
            }

            // Clean up empty app if no users left
            if app_users.is_empty() {
                drop(app_users); // Drop the reference before removing
                self.sessions.remove(app_name);
            }
        }
        Ok(())
    }

    async fn list_sessions(&self, app_name: &str, user_id: &str) -> AgentResult<Vec<Session>> {
        if let Some(app_users) = self.sessions.get(app_name) {
            if let Some(user_sessions) = app_users.get(user_id) {
                return Ok(user_sessions.iter().map(|entry| entry.value().clone()).collect());
            }
        }
        Ok(Vec::new())
    }

    async fn session_exists(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<bool> {
        if let Some(app_users) = self.sessions.get(app_name) {
            if let Some(user_sessions) = app_users.get(user_id) {
                return Ok(user_sessions.contains_key(session_id));
            }
        }
        Ok(false)
    }

    async fn touch_session(
        &self,
        app_name: &str,
        user_id: &str,
        session_id: &str,
    ) -> AgentResult<()> {
        if let Some(app_users) = self.sessions.get(app_name) {
            if let Some(user_sessions) = app_users.get(user_id) {
                if let Some(mut session) = user_sessions.get_mut(session_id) {
                    session.last_activity = chrono::Utc::now();
                }
            }
        }
        Ok(())
    }

    // State management methods

    async fn update_app_state(&self, app_name: &str, key: &str, value: Value) -> AgentResult<()> {
        let app_state = self.app_state
            .entry(app_name.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));
        app_state.insert(key.to_string(), value);
        Ok(())
    }

    async fn update_user_state(
        &self,
        app_name: &str,
        user_id: &str,
        key: &str,
        value: Value,
    ) -> AgentResult<()> {
        let app_users = self.user_state
            .entry(app_name.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));
        let user_state = app_users
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(DashMap::new()));
        user_state.insert(key.to_string(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = InMemorySessionService::new();

        let session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();
        let retrieved = store
            .get_session(&session.app_name, &session.user_id, &session.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(session.id, retrieved.id);
        assert_eq!(session.app_name, retrieved.app_name);
        assert_eq!(session.user_id, retrieved.user_id);
    }

    #[tokio::test]
    async fn test_session_auto_generated_id() {
        let store = InMemorySessionService::new();

        let session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // ID should be auto-generated UUID
        assert!(!session.id.is_empty());

        let retrieved = store
            .get_session("test_app", "user123", &session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.id, session.id);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let store = InMemorySessionService::new();

        // Create sessions for different apps and users
        store
            .create_session("app1".to_string(), "user1".to_string())
            .await
            .unwrap();
        store
            .create_session("app1".to_string(), "user2".to_string())
            .await
            .unwrap();
        store
            .create_session("app2".to_string(), "user1".to_string())
            .await
            .unwrap();

        let app1_user1_sessions = store.list_sessions("app1", "user1").await.unwrap();
        assert_eq!(app1_user1_sessions.len(), 1);

        let app1_user2_sessions = store.list_sessions("app1", "user2").await.unwrap();
        let total_app1_sessions = app1_user1_sessions.len() + app1_user2_sessions.len();
        assert_eq!(total_app1_sessions, 2);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let store = InMemorySessionService::new();

        let session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();
        assert!(
            store
                .session_exists(&session.app_name, &session.user_id, &session.id)
                .await
                .unwrap()
        );

        store
            .delete_session(&session.app_name, &session.user_id, &session.id)
            .await
            .unwrap();
        assert!(
            !store
                .session_exists(&session.app_name, &session.user_id, &session.id)
                .await
                .unwrap()
        );
    }

    // Edge case tests for state management

    #[tokio::test]
    async fn test_state_autoloading_empty_state() {
        let store = InMemorySessionService::new();

        // Create session without any pre-existing app or user state
        let session = store
            .create_session("new_app".to_string(), "new_user".to_string())
            .await
            .unwrap();

        // Should have empty state but no errors
        assert_eq!(session.get_state("app:nonexistent"), None);
        assert_eq!(session.get_state("user:nonexistent"), None);
        assert_eq!(session.get_state("nonexistent"), None);
    }

    #[tokio::test]
    async fn test_state_autoloading_with_existing_state() {
        let store = InMemorySessionService::new();

        // Set up app state first using immediate update methods
        store
            .update_app_state("test_app", "max_tokens", json!(1000))
            .await
            .unwrap();
        store
            .update_app_state("test_app", "theme", json!("dark"))
            .await
            .unwrap();

        // Set up user state using immediate update methods
        store
            .update_user_state("test_app", "user123", "language", json!("spanish"))
            .await
            .unwrap();
        store
            .update_user_state("test_app", "user123", "notifications", json!(true))
            .await
            .unwrap();

        // Create new session - should automatically load both app and user state
        let session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Verify app state is loaded (now with app: prefix)
        assert_eq!(session.get_state("app:max_tokens"), Some(&json!(1000)));
        assert_eq!(session.get_state("app:theme"), Some(&json!("dark")));

        // Verify user state is loaded (now with user: prefix)
        assert_eq!(session.get_state("user:language"), Some(&json!("spanish")));
        assert_eq!(session.get_state("user:notifications"), Some(&json!(true)));
    }

    #[tokio::test]
    async fn test_state_isolation_between_apps() {
        let store = InMemorySessionService::new();

        // Set up state for app1 using immediate updates
        store
            .update_app_state("app1", "feature_x", json!(true))
            .await
            .unwrap();
        store
            .update_user_state("app1", "user123", "preference_a", json!("value1"))
            .await
            .unwrap();

        // Set up state for app2 using immediate updates
        store
            .update_app_state("app2", "feature_y", json!(false))
            .await
            .unwrap();
        store
            .update_user_state("app2", "user123", "preference_b", json!("value2"))
            .await
            .unwrap();

        // Create sessions for both apps with same user
        let session1 = store
            .create_session("app1".to_string(), "user123".to_string())
            .await
            .unwrap();

        let session2 = store
            .create_session("app2".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Verify state isolation using prefixed keys
        assert_eq!(session1.get_state("app:feature_x"), Some(&json!(true)));
        assert_eq!(session1.get_state("app:feature_y"), None);
        assert_eq!(
            session1.get_state("user:preference_a"),
            Some(&json!("value1"))
        );
        assert_eq!(session1.get_state("user:preference_b"), None);

        assert_eq!(session2.get_state("app:feature_y"), Some(&json!(false)));
        assert_eq!(session2.get_state("app:feature_x"), None);
        assert_eq!(
            session2.get_state("user:preference_b"),
            Some(&json!("value2"))
        );
        assert_eq!(session2.get_state("user:preference_a"), None);
    }

    #[tokio::test]
    async fn test_state_isolation_between_users() {
        let store = InMemorySessionService::new();

        // Set up user state for different users in same app using immediate methods
        store
            .update_user_state("test_app", "user1", "theme", json!("dark"))
            .await
            .unwrap();
        store
            .update_user_state("test_app", "user1", "language", json!("en"))
            .await
            .unwrap();
        store
            .update_user_state("test_app", "user2", "theme", json!("light"))
            .await
            .unwrap();
        store
            .update_user_state("test_app", "user2", "language", json!("es"))
            .await
            .unwrap();

        // Both users should see the same app state
        store
            .update_app_state("test_app", "version", json!("1.0.0"))
            .await
            .unwrap();

        let session1 = store
            .create_session("test_app".to_string(), "user1".to_string())
            .await
            .unwrap();

        let session2 = store
            .create_session("test_app".to_string(), "user2".to_string())
            .await
            .unwrap();

        // Get sessions using actual session IDs (now always merged)
        let merged_session1 = store
            .get_session("test_app", "user1", &session1.id)
            .await
            .unwrap()
            .unwrap();
        let merged_session2 = store
            .get_session("test_app", "user2", &session2.id)
            .await
            .unwrap()
            .unwrap();

        // Both see app state
        assert_eq!(
            merged_session1.get_state("app:version"),
            Some(&json!("1.0.0"))
        );
        assert_eq!(
            merged_session2.get_state("app:version"),
            Some(&json!("1.0.0"))
        );

        // But different user preferences
        assert_eq!(
            merged_session1.get_state("user:theme"),
            Some(&json!("dark"))
        );
        assert_eq!(
            merged_session1.get_state("user:language"),
            Some(&json!("en"))
        );

        assert_eq!(
            merged_session2.get_state("user:theme"),
            Some(&json!("light"))
        );
        assert_eq!(
            merged_session2.get_state("user:language"),
            Some(&json!("es"))
        );
    }

    #[tokio::test]
    async fn test_immediate_state_persistence() {
        let store = InMemorySessionService::new();

        // Create first session
        let mut session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Set session-specific state
        session.set_state("session_key".to_string(), json!("session_value"));

        // Set user and app state immediately
        store
            .update_user_state("test_app", "user123", "user_pref", json!("user_value"))
            .await
            .unwrap();
        store
            .update_app_state("test_app", "app_config", json!("app_value"))
            .await
            .unwrap();

        // Save session (saves session-level state only)
        store.save_session(&session).await.unwrap();

        // Create new session for same user/app - should have merged app/user state
        let new_session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Get new session (now always merged)
        let merged_new_session = store
            .get_session("test_app", "user123", &new_session.id)
            .await
            .unwrap()
            .unwrap();

        // User and app state should be loaded
        assert_eq!(
            merged_new_session.get_state("user:user_pref"),
            Some(&json!("user_value"))
        );
        assert_eq!(
            merged_new_session.get_state("app:app_config"),
            Some(&json!("app_value"))
        );
        // Session state should NOT be loaded (it's session-specific)
        assert_eq!(merged_new_session.get_state("session_key"), None);
    }

    #[tokio::test]
    async fn test_get_session_vs_create() {
        let store = InMemorySessionService::new();

        // Set up some app and user state using immediate methods
        store
            .update_app_state("test_app", "setting1", json!("value1"))
            .await
            .unwrap();
        store
            .update_user_state("test_app", "user123", "pref1", json!("pref_value"))
            .await
            .unwrap();

        // Create session - doesn't have merged state initially
        let created_session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Get same session - should have app/user state (always merged now)
        let retrieved_session = store
            .get_session("test_app", "user123", &created_session.id)
            .await
            .unwrap()
            .unwrap();

        // All sessions should have app/user state since get_session always merges
        assert_eq!(
            created_session.get_state("app:setting1"),
            Some(&json!("value1"))
        );
        assert_eq!(
            retrieved_session.get_state("app:setting1"),
            Some(&json!("value1"))
        );

        // And user state
        assert_eq!(
            created_session.get_state("user:pref1"),
            Some(&json!("pref_value"))
        );
        assert_eq!(
            retrieved_session.get_state("user:pref1"),
            Some(&json!("pref_value"))
        );
    }

    #[tokio::test]
    async fn test_nonexistent_session_retrieval() {
        let store = InMemorySessionService::new();

        let result = store
            .get_session("app", "user", "nonexistent-session")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_state_key_prefixes_edge_cases() {
        let store = InMemorySessionService::new();

        let mut session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Test edge cases with prefixes - session-level keys only
        // Note: Keys starting with "app:" and "user:" are intended for app/user level state
        // and will be filtered out when saving sessions, as per three-tier architecture
        session.set_state("temp:temp_data".to_string(), json!("temporary"));
        session.set_state("custom:namespace:key".to_string(), json!("namespaced"));
        session.set_state("normal_key".to_string(), json!("normal"));
        session.set_state("key_with:colon".to_string(), json!("colon_value"));

        store.save_session(&session).await.unwrap();

        // Test retrieval - only session-level keys should be preserved
        let retrieved = store
            .get_session("test_app", "user123", &session.id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            retrieved.get_state("temp:temp_data"),
            Some(&json!("temporary"))
        );
        assert_eq!(
            retrieved.get_state("custom:namespace:key"),
            Some(&json!("namespaced"))
        );
        assert_eq!(retrieved.get_state("normal_key"), Some(&json!("normal")));
        assert_eq!(
            retrieved.get_state("key_with:colon"),
            Some(&json!("colon_value"))
        );

        // Keys that start with "app:" or "user:" should not be present at session level
        // (they are filtered out as they belong to app/user levels)
        assert_eq!(retrieved.get_state("app:some_key"), None);
        assert_eq!(retrieved.get_state("user:some_key"), None);
    }

    #[tokio::test]
    async fn test_concurrent_state_updates() {
        use std::sync::Arc;
        use tokio::task::JoinSet;

        let store = Arc::new(InMemorySessionService::new());
        let mut join_set = JoinSet::new();

        // Spawn multiple tasks updating app state concurrently using immediate methods
        for i in 0..10 {
            let store_clone = store.clone();
            join_set.spawn(async move {
                store_clone
                    .update_app_state(
                        "test_app",
                        &format!("key_{}", i),
                        json!(format!("value_{}", i)),
                    )
                    .await
                    .unwrap();
            });
        }

        // Wait for all updates to complete
        while let Some(result) = join_set.join_next().await {
            result.unwrap();
        }

        // Create a session to verify the app state was updated
        let session = store
            .create_session("test_app".to_string(), "user".to_string())
            .await
            .unwrap();

        // Verify all updates were applied through the merged state
        for i in 0..10 {
            assert_eq!(
                session.get_state(&format!("app:key_{}", i)),
                Some(&json!(format!("value_{}", i)))
            );
        }
    }

    #[tokio::test]
    async fn test_concurrent_session_creation_and_retrieval() {
        use std::sync::Arc;
        use tokio::task::JoinSet;

        let store = Arc::new(InMemorySessionService::new());
        let mut join_set = JoinSet::new();

        // Set up some initial state using immediate method
        store
            .update_app_state("test_app", "shared_config", json!("shared_value"))
            .await
            .unwrap();

        // Spawn multiple tasks creating and retrieving sessions concurrently
        for i in 0..20 {
            let store_clone = store.clone();
            join_set.spawn(async move {
                let _session_id = format!("session_{}", i);
                let user_id = format!("user_{}", i % 5); // 5 different users

                // Create session
                let session = store_clone
                    .create_session("test_app".to_string(), user_id.clone())
                    .await
                    .unwrap();

                // Immediately retrieve it using the session's actual ID
                let _retrieved = store_clone
                    .get_session("test_app", &user_id, &session.id)
                    .await
                    .unwrap()
                    .unwrap();

                // Get session to check app state (always merged now)
                let merged = store_clone
                    .get_session("test_app", &user_id, &session.id)
                    .await
                    .unwrap()
                    .unwrap();

                // Should have app state in merged version
                assert_eq!(
                    merged.get_state("app:shared_config"),
                    Some(&json!("shared_value"))
                );

                session.id
            });
        }

        // Collect all session IDs
        let mut session_ids = Vec::new();
        while let Some(result) = join_set.join_next().await {
            session_ids.push(result.unwrap());
        }

        assert_eq!(session_ids.len(), 20);

        // Verify all sessions exist
        for (i, session_id) in session_ids.iter().enumerate() {
            let user_id = format!("user_{}", i % 5);
            assert!(
                store
                    .session_exists("test_app", &user_id, session_id)
                    .await
                    .unwrap()
            );
        }
    }

    #[tokio::test]
    async fn test_large_state_values() {
        let store = InMemorySessionService::new();

        // Create large state values
        let large_string = "x".repeat(10000);
        let large_object = json!({
            "data": large_string,
            "array": (0..1000).collect::<Vec<i32>>(),
            "nested": {
                "deep": {
                    "value": "deeply nested"
                }
            }
        });

        let mut session = store
            .create_session("test_app".to_string(), "user123".to_string())
            .await
            .unwrap();

        // Set session-level large data
        session.set_state("large_data".to_string(), large_object.clone());

        // Set user and app state using immediate methods
        store
            .update_user_state("test_app", "user123", "large_pref", large_object.clone())
            .await
            .unwrap();
        store
            .update_app_state("test_app", "large_config", large_object.clone())
            .await
            .unwrap();

        store.save_session(&session).await.unwrap();

        // Retrieve and verify (always merged now)
        let retrieved = store
            .get_session("test_app", "user123", &session.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.get_state("large_data"), Some(&large_object));
        assert_eq!(retrieved.get_state("user:large_pref"), Some(&large_object));
        assert_eq!(retrieved.get_state("app:large_config"), Some(&large_object));
    }
}
