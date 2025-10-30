//! In-memory implementation of the `MemoryService` trait.
//!
//! This module provides both native (thread-safe) and WASM (single-threaded)
//! implementations of the memory service.

use serde_json::Value;

use crate::errors::AgentResult;
use crate::runtime::context::AuthContext;
use crate::runtime::memory::MemoryService;

// ============================================================================
// Native Implementation (Thread-Safe)
// ============================================================================

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
mod native {
    use super::*;
    use dashmap::DashMap;
    use std::sync::Arc;

    /// An in-memory implementation of the [`MemoryService`].
    ///
    /// This service uses a `DashMap` for thread-safe, concurrent access to stored data.
    /// Data is namespaced by the `AuthContext`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::runtime::InMemoryMemoryService;
    /// use radkit::runtime::services::MemoryService;
    ///
    /// let service = InMemoryMemoryService::new();
    /// let auth_ctx = AuthContext {
    ///     app_name: "my-app".to_string(),
    ///     user_name: "user1".to_string(),
    /// };
    ///
    /// service.save_serialized(&auth_ctx, "key1", json!({"value": 42})).await?;
    /// let value = service.load_serialized(&auth_ctx, "key1").await?;
    /// ```
    #[derive(Debug, Default)]
    pub struct InMemoryMemoryService {
        store: Arc<DashMap<String, Value>>,
    }

    impl InMemoryMemoryService {
        /// Creates a new `InMemoryMemoryService`.
        pub fn new() -> Self {
            Self::default()
        }

        /// Creates a namespaced key from the auth context and the original key.
        fn get_namespaced_key(&self, auth_ctx: &AuthContext, key: &str) -> String {
            format!("{}:{}:{}", auth_ctx.app_name, auth_ctx.user_name, key)
        }
    }

    #[async_trait::async_trait]
    impl MemoryService for InMemoryMemoryService {
        async fn save_serialized(
            &self,
            auth_ctx: &AuthContext,
            key: &str,
            value: Value,
        ) -> AgentResult<()> {
            let namespaced_key = self.get_namespaced_key(auth_ctx, key);
            self.store.insert(namespaced_key, value);
            Ok(())
        }

        async fn load_serialized(
            &self,
            auth_ctx: &AuthContext,
            key: &str,
        ) -> AgentResult<Option<Value>> {
            let namespaced_key = self.get_namespaced_key(auth_ctx, key);
            Ok(self.store.get(&namespaced_key).map(|v| v.value().clone()))
        }
    }
}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub use native::InMemoryMemoryService;

// ============================================================================
// WASM Implementation (Single-Threaded)
// ============================================================================

#[cfg(all(target_os = "wasi", target_env = "p1"))]
mod wasm {
    use super::{Value, AuthContext, MemoryService, AgentResult};
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// An in-memory implementation of the [`MemoryService`] for WASM.
    ///
    /// This service uses a `RefCell<HashMap>` for single-threaded access to stored data.
    /// Data is namespaced by the `AuthContext`.
    #[derive(Debug, Default)]
    pub struct InMemoryMemoryService {
        store: RefCell<HashMap<String, Value>>,
    }

    impl InMemoryMemoryService {
        /// Creates a new `InMemoryMemoryService`.
        #[must_use] pub fn new() -> Self {
            Self::default()
        }

        /// Creates a namespaced key from the auth context and the original key.
        fn get_namespaced_key(&self, auth_ctx: &AuthContext, key: &str) -> String {
            format!("{}:{}:{}", auth_ctx.app_name, auth_ctx.user_name, key)
        }
    }

    #[async_trait::async_trait(?Send)]
    impl MemoryService for InMemoryMemoryService {
        async fn save_serialized(
            &self,
            auth_ctx: &AuthContext,
            key: &str,
            value: Value,
        ) -> AgentResult<()> {
            let namespaced_key = self.get_namespaced_key(auth_ctx, key);
            self.store.borrow_mut().insert(namespaced_key, value);
            Ok(())
        }

        async fn load_serialized(
            &self,
            auth_ctx: &AuthContext,
            key: &str,
        ) -> AgentResult<Option<Value>> {
            let namespaced_key = self.get_namespaced_key(auth_ctx, key);
            Ok(self.store.borrow().get(&namespaced_key).cloned())
        }
    }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub use wasm::InMemoryMemoryService;
