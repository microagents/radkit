//! Status mapping utilities for converting skill results to A2A protocol types.
//!
//! This module centralizes all conversion logic between radkit's internal skill
//! handler results and the A2A protocol's task status types. Keeping these conversions
//! isolated ensures we maintain a clean boundary between our internal types and the
//! external protocol.

use crate::agent::{OnInputResult, OnRequestResult};
use a2a_types::{TaskState, TaskStatus, TaskStatusUpdateEvent};

/// Creates a `TaskStatus` representing the Working state.
///
/// This is the initial state when a task begins execution or when it resumes
/// from `InputRequired` state.
#[must_use] pub fn working_status() -> TaskStatus {
    TaskStatus {
        state: TaskState::Working,
        timestamp: Some(now()),
        message: None,
    }
}

/// Converts an `OnRequestResult` to an A2A `TaskStatus`.
///
/// Maps the skill handler's return value to the appropriate A2A task state:
/// - `InputRequired` → `TaskState::InputRequired`
/// - `Completed` → `TaskState::Completed`
/// - `Failed` → `TaskState::Failed`
/// - `Rejected` → `TaskState::Rejected`
///
/// # Arguments
/// * `result` - The result from calling a skill's `on_request` method
///
/// # Returns
/// The corresponding A2A `TaskStatus` with timestamp
#[must_use] pub fn on_request_to_status(result: &OnRequestResult) -> TaskStatus {
    let state = match result {
        OnRequestResult::InputRequired { .. } => TaskState::InputRequired,
        OnRequestResult::Completed { .. } => TaskState::Completed,
        OnRequestResult::Failed { .. } => TaskState::Failed,
        OnRequestResult::Rejected { .. } => TaskState::Rejected,
    };

    TaskStatus {
        state,
        timestamp: Some(now()),
        message: None,
    }
}

/// Converts an `OnInputResult` to an A2A `TaskStatus`.
///
/// Similar to `on_request_to_status` but for the continuation flow when
/// a task is resuming from `InputRequired` state.
///
/// Note: `OnInputResult` does not have a Rejected variant because rejection
/// can only occur before a task starts, not during continuation.
///
/// # Arguments
/// * `result` - The result from calling a skill's `on_input_received` method
///
/// # Returns
/// The corresponding A2A `TaskStatus` with timestamp
#[must_use] pub fn on_input_to_status(result: &OnInputResult) -> TaskStatus {
    let state = match result {
        OnInputResult::InputRequired { .. } => TaskState::InputRequired,
        OnInputResult::Completed { .. } => TaskState::Completed,
        OnInputResult::Failed { .. } => TaskState::Failed,
    };

    TaskStatus {
        state,
        timestamp: Some(now()),
        message: None,
    }
}

/// Creates a `TaskStatusUpdateEvent` from a task status.
///
/// This constructs the A2A protocol event that gets emitted when a task's
/// status changes. The event includes the task and context identifiers along
/// with the finality flag.
///
/// # Arguments
/// * `task_id` - The unique identifier of the task
/// * `context_id` - The context identifier grouping related tasks
/// * `status` - The new status of the task
/// * `is_final` - Whether this is a terminal state change
///
/// # Returns
/// A properly structured `TaskStatusUpdateEvent`
#[must_use] pub fn create_status_update_event(
    task_id: &str,
    context_id: &str,
    status: TaskStatus,
    is_final: bool,
) -> TaskStatusUpdateEvent {
    TaskStatusUpdateEvent {
        kind: a2a_types::STATUS_UPDATE_KIND.to_string(),
        task_id: task_id.to_string(),
        context_id: context_id.to_string(),
        status,
        is_final,
        metadata: None,
    }
}

/// Checks if a `TaskState` is terminal (cannot transition further).
///
/// Per the A2A specification, terminal states are those where no further
/// state transitions are possible (except for the special case of Canceled).
/// `InputRequired` is NOT terminal - it can transition to other states when
/// input is provided.
///
/// # Arguments
/// * `state` - The task state to check
///
/// # Returns
/// `true` if the state is terminal, `false` otherwise
#[must_use] pub const fn is_terminal_state(state: &TaskState) -> bool {
    matches!(
        state,
        TaskState::Completed
            | TaskState::Failed
            | TaskState::Rejected
            | TaskState::Canceled
            | TaskState::Unknown
    )
}

/// Checks if a task can be continued (resumed with new input).
///
/// Per the A2A specification, only tasks in the `InputRequired` state
/// can accept additional input and continue execution.
///
/// # Arguments
/// * `state` - The task state to check
///
/// # Returns
/// `true` if the task can receive input and continue, `false` otherwise
#[must_use] pub const fn can_continue(state: &TaskState) -> bool {
    matches!(state, TaskState::InputRequired)
}

/// Helper to get the current ISO 8601 timestamp.
///
/// Returns the current UTC time in RFC 3339 format, which is compatible
/// with the A2A protocol's timestamp requirements.
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Content;

    #[test]
    fn test_working_status() {
        let status = working_status();
        assert_eq!(status.state, TaskState::Working);
        assert!(status.timestamp.is_some());
        assert!(status.message.is_none());
    }

    #[test]
    fn test_on_request_to_status_input_required() {
        let result = OnRequestResult::InputRequired {
            message: Content::from_text("Need more info"),
            slot: crate::agent::SkillSlot::new(()),
        };
        let status = on_request_to_status(&result);
        assert_eq!(status.state, TaskState::InputRequired);
        assert!(status.timestamp.is_some());
    }

    #[test]
    fn test_on_request_to_status_completed() {
        let result = OnRequestResult::Completed {
            message: Some(Content::from_text("Done")),
            artifacts: vec![],
        };
        let status = on_request_to_status(&result);
        assert_eq!(status.state, TaskState::Completed);
        assert!(status.timestamp.is_some());
    }

    #[test]
    fn test_on_request_to_status_failed() {
        let result = OnRequestResult::Failed {
            error: Content::from_text("Something went wrong"),
        };
        let status = on_request_to_status(&result);
        assert_eq!(status.state, TaskState::Failed);
        assert!(status.timestamp.is_some());
    }

    #[test]
    fn test_on_request_to_status_rejected() {
        let result = OnRequestResult::Rejected {
            reason: Content::from_text("Out of scope"),
        };
        let status = on_request_to_status(&result);
        assert_eq!(status.state, TaskState::Rejected);
        assert!(status.timestamp.is_some());
    }

    #[test]
    fn test_is_terminal_state() {
        assert!(is_terminal_state(&TaskState::Completed));
        assert!(is_terminal_state(&TaskState::Failed));
        assert!(is_terminal_state(&TaskState::Rejected));
        assert!(is_terminal_state(&TaskState::Canceled));
        assert!(is_terminal_state(&TaskState::Unknown));

        assert!(!is_terminal_state(&TaskState::Working));
        assert!(!is_terminal_state(&TaskState::InputRequired));
        assert!(!is_terminal_state(&TaskState::Submitted));
    }

    #[test]
    fn test_can_continue() {
        assert!(can_continue(&TaskState::InputRequired));

        assert!(!can_continue(&TaskState::Working));
        assert!(!can_continue(&TaskState::Completed));
        assert!(!can_continue(&TaskState::Failed));
        assert!(!can_continue(&TaskState::Rejected));
        assert!(!can_continue(&TaskState::Canceled));
    }

    #[test]
    fn test_create_status_update_event() {
        let status = working_status();
        let event = create_status_update_event("task-123", "ctx-456", status.clone(), false);

        assert_eq!(event.kind, "status-update");
        assert_eq!(event.task_id, "task-123");
        assert_eq!(event.context_id, "ctx-456");
        assert_eq!(event.status.state, TaskState::Working);
        assert!(!event.is_final);
    }
}
