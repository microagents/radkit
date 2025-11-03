//! Memory storage service for agent data.
//!
//! This module provides key-value storage functionality for agents to persist data.
//! The [`MemoryService`] trait defines the core interface for storage operations,
//! and [`MemoryServiceExt`] provides convenient typed access methods.
//!
//! # Multi-tenancy
//!
//! All memory operations are namespaced by [`AuthContext`](crate::runtime::context::AuthContext),
//! ensuring that different users and applications cannot access each other's data.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::runtime::memory::{MemoryService, MemoryServiceExt, InMemoryMemoryService};
//! use radkit::runtime::context::AuthContext;
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize)]
//! struct UserData {
//!     name: String,
//!     score: i32,
//! }
//!
//! let memory = InMemoryMemoryService::new();
//! let auth_ctx = AuthContext {
//!     app_name: "my-app".to_string(),
//!     user_name: "alice".to_string(),
//! };
//!
//! // Save typed data
//! let data = UserData { name: "Alice".to_string(), score: 100 };
//! memory.save(&auth_ctx, "user_data", &data).await?;
//!
//! // Load typed data
//! let loaded: Option<UserData> = memory.load(&auth_ctx, "user_data").await?;
//! ```

pub mod in_memory;

pub use in_memory::InMemoryMemoryService;

use serde::{de::DeserializeOwned, Serialize};

use crate::{
    compat::{MaybeSend, MaybeSync},
    errors::AgentResult,
    runtime::context::AuthContext,
};

/// Service for persistent key-value storage.
///
/// Memory service provides storage and retrieval of JSON-serialized values.
/// Use [`MemoryServiceExt`] for convenient typed access.
///
/// # Multi-tenancy
///
/// All operations are namespaced by the [`AuthContext`], ensuring data isolation
/// between different users and applications.
#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
pub trait MemoryService: MaybeSend + MaybeSync {
    /// Stores a JSON value under the given key, namespaced by the auth context.
    ///
    /// If a value already exists for the key, it will be replaced.
    ///
    /// # Arguments
    ///
    /// * `auth_ctx` - The authentication context for namespacing.
    /// * `key` - Storage key.
    /// * `value` - JSON value to store.
    ///
    /// # Errors
    ///
    /// May return [`crate::errors::AgentError::Internal`] if storage fails.
    async fn save_serialized(
        &self,
        auth_ctx: &AuthContext,
        key: &str,
        value: serde_json::Value,
    ) -> AgentResult<()>;

    /// Loads a JSON value for the given key, namespaced by the auth context.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `auth_ctx` - The authentication context for namespacing.
    /// * `key` - Storage key.
    ///
    /// # Errors
    ///
    /// May return [`crate::errors::AgentError::Internal`] if loading fails.
    async fn load_serialized(
        &self,
        auth_ctx: &AuthContext,
        key: &str,
    ) -> AgentResult<Option<serde_json::Value>>;
}

/// Extension trait providing typed access to [`MemoryService`].
///
/// This trait is automatically implemented for all types that implement
/// [`MemoryService`]. It provides convenient methods for storing and
/// retrieving typed values without manual JSON handling.
#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
pub trait MemoryServiceExt {
    /// Stores a typed value under the given key.
    async fn save<T>(&self, auth_ctx: &AuthContext, key: &str, value: &T) -> AgentResult<()>
    where
        T: Serialize + MaybeSend + MaybeSync;

    /// Loads a typed value for the given key.
    async fn load<T>(&self, auth_ctx: &AuthContext, key: &str) -> AgentResult<Option<T>>
    where
        T: DeserializeOwned + MaybeSend;
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl<M> MemoryServiceExt for M
where
    M: MemoryService + MaybeSync + ?Sized,
{
    async fn save<T>(&self, auth_ctx: &AuthContext, key: &str, value: &T) -> AgentResult<()>
    where
        T: Serialize + MaybeSend + MaybeSync,
    {
        let json = serde_json::to_value(value)?;
        self.save_serialized(auth_ctx, key, json).await
    }

    async fn load<T>(&self, auth_ctx: &AuthContext, key: &str) -> AgentResult<Option<T>>
    where
        T: DeserializeOwned + MaybeSend,
    {
        self.load_serialized(auth_ctx, key)
            .await?
            .map(|value| serde_json::from_value(value).map_err(Into::into))
            .transpose()
    }
}
