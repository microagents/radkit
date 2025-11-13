//! Helpers for translating internal errors into protocol-specific payloads.

use crate::errors::AgentError;
use a2a_types::{
    InternalError, InvalidParamsError, InvalidRequestError, JSONRPCError, TaskNotFoundError,
    UnsupportedOperationError,
};
use serde_json::json;

/// Map an [`AgentError`] into an A2A-compliant [`JSONRPCError`].
///
/// This keeps protocol wiring outside of the executor so that execution logic
/// only deals with domain errors.
#[must_use]
pub fn to_jsonrpc_error(error: AgentError) -> JSONRPCError {
    use AgentError::{
        AgentNotFound, BlockingNotSupported, InvalidConfiguration, InvalidInput,
        MissingConfiguration, MissingInput, NotImplemented, SkillNotFound, SkillSlot, TaskNotFound,
        Validation,
    };

    match error {
        InvalidInput(message) | MissingInput(message) | SkillSlot(message) => {
            invalid_params_error(Some(message), None)
        }
        TaskNotFound { task_id } => task_not_found_error(
            Some(format!("Task not found: {task_id}")),
            Some(json!({ "taskId": task_id })),
        ),
        SkillNotFound { skill_id } => invalid_params_error(
            Some(format!("Skill not found: {skill_id}")),
            Some(json!({ "skillId": skill_id })),
        ),
        AgentNotFound { agent_id } => invalid_request_error(
            Some(format!("Agent not found: {agent_id}")),
            Some(json!({ "agentId": agent_id })),
        ),
        NotImplemented { feature } => unsupported_operation_error(
            Some(format!("Feature not implemented: {feature}")),
            Some(json!({ "feature": feature })),
        ),
        BlockingNotSupported => unsupported_operation_error(
            Some("Blocking operations are not supported".to_string()),
            None,
        ),
        InvalidConfiguration { field, reason } => invalid_request_error(
            Some(format!("Invalid configuration for {field}: {reason}")),
            Some(json!({ "field": field, "reason": reason })),
        ),
        MissingConfiguration { field } => invalid_request_error(
            Some(format!("Missing configuration: {field}")),
            Some(json!({ "field": field })),
        ),
        Validation { field, reason } => invalid_params_error(
            Some(format!("Validation failed for {field}: {reason}")),
            Some(json!({ "field": field, "reason": reason })),
        ),
        other => internal_error(
            None,
            Some(json!({
                "details": other.to_string(),
            })),
        ),
    }
}

fn invalid_params_error(message: Option<String>, data: Option<serde_json::Value>) -> JSONRPCError {
    let defaults = InvalidParamsError::default();
    JSONRPCError {
        code: defaults.code,
        message: message.unwrap_or(defaults.message),
        data,
    }
}

fn invalid_request_error(message: Option<String>, data: Option<serde_json::Value>) -> JSONRPCError {
    let defaults = InvalidRequestError::default();
    JSONRPCError {
        code: defaults.code,
        message: message.unwrap_or(defaults.message),
        data,
    }
}

fn unsupported_operation_error(
    message: Option<String>,
    data: Option<serde_json::Value>,
) -> JSONRPCError {
    let defaults = UnsupportedOperationError::default();
    JSONRPCError {
        code: defaults.code,
        message: message.unwrap_or(defaults.message),
        data,
    }
}

fn internal_error(message: Option<String>, data: Option<serde_json::Value>) -> JSONRPCError {
    let defaults = InternalError::default();
    JSONRPCError {
        code: defaults.code,
        message: message.unwrap_or(defaults.message),
        data,
    }
}

fn task_not_found_error(message: Option<String>, data: Option<serde_json::Value>) -> JSONRPCError {
    let defaults = TaskNotFoundError::default();
    JSONRPCError {
        code: defaults.code,
        message: message.unwrap_or(defaults.message),
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::AgentError;

    #[test]
    fn invalid_input_maps_to_invalid_params() {
        let err = to_jsonrpc_error(AgentError::InvalidInput("bad".into()));
        assert_eq!(err.code, InvalidParamsError::default().code);
        assert!(err.message.contains("bad"));
    }

    #[test]
    fn agent_not_found_maps_to_invalid_request() {
        let err = to_jsonrpc_error(AgentError::AgentNotFound {
            agent_id: "missing".into(),
        });
        assert_eq!(err.code, InvalidRequestError::default().code);
        assert!(err.message.contains("missing"));
    }

    #[test]
    fn unsupported_operation_maps_correctly() {
        let err = to_jsonrpc_error(AgentError::BlockingNotSupported);
        assert_eq!(err.code, UnsupportedOperationError::default().code);
    }
}
