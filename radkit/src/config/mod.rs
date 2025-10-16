//! Radkit Config Loader - YAML/JSON Configuration for Radkit Agents
//!
//! This crate provides YAML/JSON-based configuration loading for radkit agents,
//! allowing declarative agent definitions with tools, models, and MCP connections.
//! All configurations are validated against a JSON Schema.

use crate::errors::LoaderError;

pub mod env_resolver;
pub mod loader;
pub mod schema;
pub mod types;

pub use env_resolver::{EnvKey, EnvResolverFn, default_env_resolver};
pub use loader::*;
pub use schema::*;
pub use types::*;

/// Extension trait for configuration loading
pub trait ConfigLoaderExt {
    /// Validate configuration against JSON schema
    fn validate_config(&self) -> Result<(), LoaderError>;

    /// Get configuration as JSON Value
    fn to_json_value(&self) -> Result<serde_json::Value, LoaderError>;
}

impl ConfigLoaderExt for AgentDefinition {
    fn validate_config(&self) -> Result<(), LoaderError> {
        let json_value = serde_json::to_value(self)
            .map_err(|e| LoaderError::validation(format!("Failed to serialize config: {e}")))?;
        validate_config(&json_value)
    }

    fn to_json_value(&self) -> Result<serde_json::Value, LoaderError> {
        serde_json::to_value(self)
            .map_err(|e| LoaderError::validation(format!("Failed to serialize config: {e}")))
    }
}
