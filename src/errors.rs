/// Main error type for the Agent SDK
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    // === LLM Provider Errors ===
    #[error("LLM provider error ({provider}): {message}")]
    LlmProvider { provider: String, message: String },

    #[error("LLM API authentication failed: {provider}")]
    LlmAuthentication { provider: String },

    #[error("LLM API rate limit exceeded: {provider}")]
    LlmRateLimit { provider: String },

    #[error("LLM content filtered: {reason}")]
    ContentFiltered { reason: String },

    #[error("LLM context length exceeded: {current_tokens} > {max_tokens}")]
    ContextLengthExceeded {
        current_tokens: u32,
        max_tokens: u32,
    },

    #[error("LLM error: {source}")]
    LlmError {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Feature not implemented: {feature}")]
    NotImplemented { feature: String },

    // === Task Management Errors ===
    #[error("Task not found: {task_id}")]
    TaskNotFound { task_id: String },

    #[error("Task already exists: {task_id}")]
    TaskAlreadyExists { task_id: String },

    #[error("Invalid task state transition: {from} -> {to}")]
    InvalidTaskStateTransition { from: String, to: String },

    #[error("Task operation failed: {operation} on {task_id}: {reason}")]
    TaskOperationFailed {
        operation: String,
        task_id: String,
        reason: String,
    },

    // === Session Management Errors ===
    #[error("Session not found: {session_id} for {app_name}/{user_id}")]
    SessionNotFound {
        session_id: String,
        app_name: String,
        user_id: String,
    },

    #[error("Session access denied: {session_id} for {app_name}/{user_id}")]
    SessionAccessDenied {
        session_id: String,
        app_name: String,
        user_id: String,
    },

    #[error("Session state error: {session_id}: {reason}")]
    SessionStateError { session_id: String, reason: String },

    // === Tool Execution Errors ===
    #[error("Tool not found: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("Tool execution failed: {tool_name}: {reason}")]
    ToolExecutionFailed { tool_name: String, reason: String },

    #[error("Tool validation error: {tool_name}: {reason}")]
    ToolValidationError { tool_name: String, reason: String },

    #[error("Tool timeout: {tool_name} exceeded {timeout_ms}ms")]
    ToolTimeout { tool_name: String, timeout_ms: u64 },

    // === Security Errors ===
    #[error("Access denied: insufficient permissions for {app_name}/{user_id}")]
    AccessDenied { app_name: String, user_id: String },

    #[error("Invalid credentials for {app_name}")]
    InvalidCredentials { app_name: String },

    #[error("Security policy violation: {policy}: {reason}")]
    SecurityViolation { policy: String, reason: String },

    // === Configuration Errors ===
    #[error("Invalid configuration: {field}: {reason}")]
    InvalidConfiguration { field: String, reason: String },

    #[error("Missing configuration: {field}")]
    MissingConfiguration { field: String },

    // === Network/IO Errors ===
    #[error("Network error: {operation}: {reason}")]
    Network { operation: String, reason: String },

    #[error("Serialization error: {format}: {reason}")]
    Serialization { format: String, reason: String },

    // === General System Errors ===
    #[error("Resource exhausted: {resource}: {reason}")]
    ResourceExhausted { resource: String, reason: String },

    #[error("Internal error: {component}: {reason}")]
    Internal { component: String, reason: String },

    #[error("Validation error: {field}: {reason}")]
    Validation { field: String, reason: String },

    // === Timeout Errors ===
    #[error("Operation timed out: {operation} after {duration_ms}ms")]
    Timeout { operation: String, duration_ms: u64 },
}

impl AgentError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            // Retryable errors
            Self::LlmRateLimit { .. } => true,
            Self::Network { .. } => true,
            Self::Timeout { .. } => true,
            Self::ResourceExhausted { .. } => true,

            // Non-retryable errors
            Self::LlmAuthentication { .. } => false,
            Self::ContentFiltered { .. } => false,
            Self::ContextLengthExceeded { .. } => false,
            Self::NotImplemented { .. } => false,
            Self::TaskNotFound { .. } => false,
            Self::SessionNotFound { .. } => false,
            Self::ToolNotFound { .. } => false,
            Self::AccessDenied { .. } => false,
            Self::InvalidCredentials { .. } => false,
            Self::SecurityViolation { .. } => false,
            Self::InvalidConfiguration { .. } => false,
            Self::MissingConfiguration { .. } => false,
            Self::Validation { .. } => false,
            Self::InvalidTaskStateTransition { .. } => false,

            // Maybe retryable - depends on the specific reason
            Self::LlmProvider { .. } => false,
            Self::LlmError { .. } => false,
            Self::TaskOperationFailed { .. } => false,
            Self::SessionStateError { .. } => false,
            Self::ToolExecutionFailed { .. } => false,
            Self::ToolValidationError { .. } => false,
            Self::ToolTimeout { .. } => true,
            Self::SessionAccessDenied { .. } => false,
            Self::Serialization { .. } => false,
            Self::Internal { .. } => false,
            Self::TaskAlreadyExists { .. } => false,
        }
    }

    /// Get error category for metrics/logging
    pub fn category(&self) -> &'static str {
        match self {
            Self::LlmProvider { .. }
            | Self::LlmAuthentication { .. }
            | Self::LlmRateLimit { .. }
            | Self::ContentFiltered { .. }
            | Self::ContextLengthExceeded { .. }
            | Self::LlmError { .. }
            | Self::NotImplemented { .. } => "llm",

            Self::TaskNotFound { .. }
            | Self::TaskAlreadyExists { .. }
            | Self::InvalidTaskStateTransition { .. }
            | Self::TaskOperationFailed { .. } => "task",

            Self::SessionNotFound { .. }
            | Self::SessionAccessDenied { .. }
            | Self::SessionStateError { .. } => "session",

            Self::ToolNotFound { .. }
            | Self::ToolExecutionFailed { .. }
            | Self::ToolValidationError { .. }
            | Self::ToolTimeout { .. } => "tool",

            Self::AccessDenied { .. }
            | Self::InvalidCredentials { .. }
            | Self::SecurityViolation { .. } => "security",

            Self::InvalidConfiguration { .. } | Self::MissingConfiguration { .. } => "config",

            Self::Network { .. } | Self::Serialization { .. } => "io",

            Self::ResourceExhausted { .. }
            | Self::Internal { .. }
            | Self::Validation { .. }
            | Self::Timeout { .. } => "system",
        }
    }

    /// Check if this error should be logged as an error vs warning
    pub fn is_error_level(&self) -> bool {
        match self {
            // Warning level - expected/recoverable
            Self::TaskNotFound { .. } => false,
            Self::SessionNotFound { .. } => false,
            Self::ToolNotFound { .. } => false,
            Self::LlmRateLimit { .. } => false,
            Self::ContentFiltered { .. } => false,
            Self::ContextLengthExceeded { .. } => false,
            Self::Timeout { .. } => false,

            // Error level - unexpected/serious
            Self::LlmAuthentication { .. } => true,
            Self::Internal { .. } => true,
            Self::SecurityViolation { .. } => true,
            Self::AccessDenied { .. } => true,
            Self::InvalidCredentials { .. } => true,

            // Context-dependent - default to warning
            _ => false,
        }
    }
}

/// Convenience type alias
pub type AgentResult<T> = std::result::Result<T, AgentError>;

/// Helper macros for common error patterns
#[macro_export]
macro_rules! not_found {
    ($entity:expr, $id:expr) => {
        match stringify!($entity) {
            "task" => $crate::errors::AgentError::TaskNotFound {
                task_id: $id.to_string(),
            },
            "session" => $crate::errors::AgentError::SessionNotFound {
                session_id: $id.to_string(),
                app_name: "unknown".to_string(),
                user_id: "unknown".to_string(),
            },
            "tool" => $crate::errors::AgentError::ToolNotFound {
                tool_name: $id.to_string(),
            },
            _ => $crate::errors::AgentError::Internal {
                component: stringify!($entity).to_string(),
                reason: format!("{} not found: {}", stringify!($entity), $id),
            },
        }
    };
}

#[macro_export]
macro_rules! validation_error {
    ($field:expr, $reason:expr) => {
        $crate::errors::AgentError::Validation {
            field: $field.to_string(),
            reason: $reason.to_string(),
        }
    };
}

#[macro_export]
macro_rules! tool_error {
    ($tool_name:expr, $reason:expr) => {
        $crate::errors::AgentError::ToolExecutionFailed {
            tool_name: $tool_name.to_string(),
            reason: $reason.to_string(),
        }
    };
}

/// Convert AgentError to ToolResult for tool execution contexts
impl From<AgentError> for crate::tools::ToolResult {
    fn from(error: AgentError) -> Self {
        crate::tools::ToolResult::error(error.to_string())
    }
}

/// Convert common std errors to AgentError
impl From<serde_json::Error> for AgentError {
    fn from(error: serde_json::Error) -> Self {
        AgentError::Serialization {
            format: "json".to_string(),
            reason: error.to_string(),
        }
    }
}

impl From<reqwest::Error> for AgentError {
    fn from(error: reqwest::Error) -> Self {
        AgentError::Network {
            operation: "http_request".to_string(),
            reason: error.to_string(),
        }
    }
}

impl From<tokio::time::error::Elapsed> for AgentError {
    fn from(_error: tokio::time::error::Elapsed) -> Self {
        AgentError::Timeout {
            operation: "unknown".to_string(),
            duration_ms: 0, // We don't have the duration info from Elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_categories() {
        let task_err = AgentError::TaskNotFound {
            task_id: "test".to_string(),
        };
        assert_eq!(task_err.category(), "task");
        assert!(!task_err.is_retryable());
        assert!(!task_err.is_error_level());

        let llm_err = AgentError::LlmRateLimit {
            provider: "anthropic".to_string(),
        };
        assert_eq!(llm_err.category(), "llm");
        assert!(llm_err.is_retryable());
        assert!(!llm_err.is_error_level());

        let security_err = AgentError::SecurityViolation {
            policy: "data_access".to_string(),
            reason: "unauthorized".to_string(),
        };
        assert_eq!(security_err.category(), "security");
        assert!(!security_err.is_retryable());
        assert!(security_err.is_error_level());
    }

    #[test]
    fn test_error_macros() {
        // Test not_found! macro for different entity types
        let task_err = not_found!(task, "task123");
        assert!(matches!(
            task_err,
            AgentError::TaskNotFound { task_id } if task_id == "task123"
        ));

        let session_err = not_found!(session, "sess456");
        assert!(matches!(
            session_err,
            AgentError::SessionNotFound { session_id, .. } if session_id == "sess456"
        ));

        let tool_err = not_found!(tool, "my_tool");
        assert!(matches!(
            tool_err,
            AgentError::ToolNotFound { tool_name } if tool_name == "my_tool"
        ));

        // Test validation_error! macro
        let validation_err = validation_error!("email", "invalid format");
        assert!(matches!(
            validation_err,
            AgentError::Validation { field, reason }
                if field == "email" && reason == "invalid format"
        ));

        // Test tool_error! macro
        let tool_exec_err = tool_error!("calculator", "division by zero");
        assert!(matches!(
            tool_exec_err,
            AgentError::ToolExecutionFailed { tool_name, reason }
                if tool_name == "calculator" && reason == "division by zero"
        ));
    }

    #[test]
    fn test_error_conversions() {
        // Test serde_json conversion
        let json_err: AgentError = serde_json::from_str::<serde_json::Value>("invalid json")
            .unwrap_err()
            .into();
        assert_eq!(json_err.category(), "io");

        // Test ToolResult conversion
        let agent_err = AgentError::ToolNotFound {
            tool_name: "test_tool".to_string(),
        };
        let tool_result: crate::tools::ToolResult = agent_err.into();
        assert!(!tool_result.success);
        assert!(tool_result.error_message.is_some());
    }
}
