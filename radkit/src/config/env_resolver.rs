//! Centralized environment variable resolution
//!
//! This module provides a simple, explicit way to reference environment variables
//! using the `EnvKey` type. Users can provide custom resolvers via `AgentBuilder`
//! for integration with secret managers, vaults, or other credential storage systems.

use crate::errors::AgentError;
use std::fmt;
use std::sync::Arc;

/// A reference to an environment variable or secret key
///
/// This type makes it explicit that a value should be resolved from
/// the environment (or custom secret manager) rather than used directly.
///
/// # Example
///
/// ```no_run
/// use radkit::config::EnvKey;
/// use radkit::models::OpenAILlm;
///
/// // Explicit env var reference
/// let llm = OpenAILlm::new("gpt-4o".to_string(), EnvKey::new("OPENAI_API_KEY"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvKey(String);

impl EnvKey {
    /// Create a new environment key reference
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Get the key name
    pub fn key(&self) -> &str {
        &self.0
    }

    /// Resolve this key using the default environment resolver (std::env)
    pub fn resolve(&self) -> Result<String, AgentError> {
        std::env::var(&self.0).map_err(|_| AgentError::InvalidConfiguration {
            field: self.0.clone(),
            reason: format!("Environment variable '{}' not found", self.0),
        })
    }

    /// Resolve this key using a custom resolver, or fall back to default if None
    ///
    /// This is the primary method for resolving environment variables with optional
    /// custom resolution logic (e.g., vault, secrets manager).
    ///
    /// # Arguments
    /// * `resolver` - Optional custom resolver. If None, uses default std::env resolver
    ///
    /// # Example
    /// ```no_run
    /// use radkit::config::EnvKey;
    /// use std::sync::Arc;
    ///
    /// let key = EnvKey::new("API_KEY");
    ///
    /// // With custom resolver
    /// let vault_resolver = Arc::new(|k: &str| Ok(format!("vault_{}", k)));
    /// let value = key.resolve_with(Some(vault_resolver)).unwrap();
    ///
    /// // With default resolver
    /// let value = key.resolve_with(None).unwrap();
    /// ```
    pub fn resolve_with(&self, resolver: Option<EnvResolverFn>) -> Result<String, AgentError> {
        if let Some(resolver) = resolver {
            resolver(self.key())
        } else {
            self.resolve()
        }
    }
}

impl fmt::Display for EnvKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for EnvKey {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for EnvKey {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Type alias for environment resolver functions
///
/// This function takes an environment key and returns the resolved value.
/// Users can provide custom implementations for secret managers.
///
/// Uses Arc for shared ownership, making it cloneable across components.
///
/// # Example
///
/// ```no_run
/// use radkit::config::EnvResolverFn;
/// use std::sync::Arc;
///
/// let vault_resolver: EnvResolverFn = Arc::new(|key| {
///     // Fetch from vault instead of environment
///     Ok(format!("secret_from_vault_{}", key))
/// });
/// ```
pub type EnvResolverFn = Arc<dyn Fn(&str) -> Result<String, AgentError> + Send + Sync>;

/// Default environment resolver that uses std::env::var
pub fn default_env_resolver(key: &str) -> Result<String, AgentError> {
    std::env::var(key).map_err(|_| AgentError::InvalidConfiguration {
        field: key.to_string(),
        reason: format!("Environment variable '{key}' not found"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_key_creation() {
        let key = EnvKey::new("API_KEY");
        assert_eq!(key.key(), "API_KEY");
        assert_eq!(key.to_string(), "API_KEY");
    }

    #[test]
    fn test_env_key_from_str() {
        let key: EnvKey = "OPENAI_API_KEY".into();
        assert_eq!(key.key(), "OPENAI_API_KEY");
    }

    #[test]
    fn test_env_key_resolve() {
        unsafe {
            std::env::set_var("TEST_KEY_1", "test_value");
        }
        let key = EnvKey::new("TEST_KEY_1");
        let result = key.resolve().unwrap();
        assert_eq!(result, "test_value");
    }

    #[test]
    fn test_env_key_resolve_missing() {
        let key = EnvKey::new("MISSING_KEY_XYZ");
        let result = key.resolve();
        assert!(result.is_err());
    }

    #[test]
    fn test_default_resolver() {
        unsafe {
            std::env::set_var("TEST_KEY_2", "resolved_value");
        }
        let result = default_env_resolver("TEST_KEY_2").unwrap();
        assert_eq!(result, "resolved_value");
    }

    #[test]
    fn test_custom_resolver_function() {
        let custom_resolver: EnvResolverFn = std::sync::Arc::new(|key| {
            if key == "VAULT_SECRET" {
                Ok("secret_from_vault".to_string())
            } else {
                Err(AgentError::InvalidConfiguration {
                    field: key.to_string(),
                    reason: format!("Key '{}' not found", key),
                })
            }
        });

        assert_eq!(
            custom_resolver("VAULT_SECRET").unwrap(),
            "secret_from_vault"
        );
        assert!(custom_resolver("UNKNOWN").is_err());
    }
}
