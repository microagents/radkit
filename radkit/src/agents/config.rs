//! Agent configuration types and utilities
//!
//! This module contains configuration structures and helper types for agent setup,
//! execution parameters, and authentication contexts.

use serde_json::Value;
use std::collections::HashMap;

use crate::agents::Agent;
use crate::errors::AgentResult;
use a2a_types::{MessageSendParams, Task, TaskQueryParams};

/// Configuration for agent execution behavior
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of tool calling iterations to prevent infinite loops
    pub max_iterations: usize,
    /// Timeout for the entire execution in seconds
    pub timeout_seconds: Option<u64>,
    /// Additional custom configuration
    pub custom_config: HashMap<String, Value>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            timeout_seconds: Some(300), // 5 minutes
            custom_config: HashMap::new(),
        }
    }
}

impl AgentConfig {
    /// Create a new config with custom max iterations
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// Create a new config with custom timeout
    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    /// Create a new config without timeout
    pub fn without_timeout(mut self) -> Self {
        self.timeout_seconds = None;
        self
    }

    /// Add custom configuration value
    pub fn with_custom_config(mut self, key: String, value: Value) -> Self {
        self.custom_config.insert(key, value);
        self
    }
}
