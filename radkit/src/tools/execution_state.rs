//! Execution state management for tools.
//!
//! This module provides key-value storage that's scoped to a tool execution session.
//! Tools can use execution state to share data across multiple invocations within
//! the same execution context.
//!
//! # Overview
//!
//! - [`ExecutionState`]: Trait for execution-scoped key-value storage
//! - [`DefaultExecutionState`]: Default implementation using concurrent maps
//!
//! # Platform Differences
//!
//! The implementation varies by target platform:
//! - **Native**: Uses `dashmap::DashMap` for lock-free concurrent access
//! - **WASM**: Uses `std::cell::RefCell<HashMap>` for single-threaded interior mutability
//!
//! # Examples
//!
//! ```ignore
//! use radkit::tools::{DefaultExecutionState, ExecutionState};
//! use serde_json::json;
//!
//! let state = DefaultExecutionState::new();
//!
//! // Store data
//! state.set_state("user_id", json!("12345"));
//!
//! // Retrieve data
//! if let Some(user_id) = state.get_state("user_id") {
//!     println!("User ID: {}", user_id);
//! }
//! ```

use serde_json::Value;
use std::sync::Arc;

use crate::{MaybeSend, MaybeSync};

#[cfg(all(target_os = "wasi", target_env = "p1"))]
use std::cell::RefCell;

#[cfg(all(target_os = "wasi", target_env = "p1"))]
use std::collections::HashMap as StdHashMap;

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
pub type HashMap = Arc<dashmap::DashMap<String, Value>>;

#[cfg(all(target_os = "wasi", target_env = "p1"))]
pub type HashMap = Arc<RefCell<StdHashMap<String, Value>>>;

/// Trait encapsulating storage for execution-scoped key-value data.
///
/// Execution state provides a way for tools to share data within a single
/// execution session. Data is stored as JSON values and can be accessed
/// by string keys.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` (via `MaybeSend + MaybeSync`) to
/// support concurrent access across async tasks.
///
/// # Platform Behavior
///
/// On WASM targets, the default implementation uses `RefCell` for interior
/// mutability. This is safe because WASM execution is single-threaded.
///
/// # Panics
///
/// The WASM implementation will panic if you attempt to borrow the state
/// mutably while it's already borrowed (either mutably or immutably). This
/// is a programming error that indicates incorrect concurrent access patterns.
pub trait ExecutionState: MaybeSend + MaybeSync {
    /// Persists a JSON value under the provided key, replacing any previous value.
    ///
    /// # Arguments
    ///
    /// * `key` - Unique identifier for the value
    /// * `value` - JSON value to store
    ///
    /// # Panics
    ///
    /// On WASM, panics if the state is already borrowed mutably. This indicates
    /// a programming error (reentrancy issue).
    fn set_state(&self, key: &str, value: Value);

    /// Retrieves a JSON value for the given key, cloning it out of the store.
    ///
    /// Returns `None` if the key doesn't exist.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to look up
    ///
    /// # Panics
    ///
    /// On WASM, panics if the state is already borrowed mutably. This indicates
    /// a programming error (reentrancy issue).
    fn get_state(&self, key: &str) -> Option<Value>;
}

/// Default in-memory implementation backed by a concurrent map on native targets
/// and a single-threaded cell on WASM.
#[derive(Clone)]
pub struct DefaultExecutionState {
    state: HashMap,
}

impl DefaultExecutionState {
    /// Construct a new execution state store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    fn create_state() -> HashMap {
        Arc::new(dashmap::DashMap::default())
    }

    #[cfg(all(target_os = "wasi", target_env = "p1"))]
    fn create_state() -> HashMap {
        Arc::new(RefCell::new(StdHashMap::new()))
    }
}

impl Default for DefaultExecutionState {
    fn default() -> Self {
        Self {
            state: Self::create_state(),
        }
    }
}

#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
impl ExecutionState for DefaultExecutionState {
    fn set_state(&self, key: &str, value: Value) {
        self.state.insert(key.to_owned(), value);
    }

    fn get_state(&self, key: &str) -> Option<Value> {
        self.state.get(key).map(|entry| entry.value().clone())
    }
}

#[cfg(all(target_os = "wasi", target_env = "p1"))]
impl ExecutionState for DefaultExecutionState {
    fn set_state(&self, key: &str, value: Value) {
        self.state.borrow_mut().insert(key.to_owned(), value);
    }

    fn get_state(&self, key: &str) -> Option<Value> {
        self.state.borrow().get(key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_execution_state_round_trips_values() {
        let state = DefaultExecutionState::new();
        assert!(state.get_state("missing").is_none());

        state.set_state("key", json!(42));
        assert_eq!(state.get_state("key"), Some(json!(42)));
    }
}
