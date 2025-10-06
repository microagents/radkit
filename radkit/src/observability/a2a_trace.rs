//! A2A Trace Propagation
//!
//! This module provides W3C Trace Context propagation across A2A protocol messages.
//! Uses the Message.metadata field to carry trace context headers.

use a2a_types::Message;
use opentelemetry::propagation::TextMapPropagator;
use std::collections::HashMap;

use crate::observability::config::TelemetryConfig;

/// Inject trace context into A2A message metadata
///
/// Adds W3C Trace Context (traceparent, tracestate) to message metadata.
/// The message is modified in-place and returned for convenience.
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::inject_trace_context;
/// # use a2a_types::Message;
///
/// let message = /* create your message */
/// # Message {
/// #     kind: "message".to_string(),
/// #     message_id: "test".to_string(),
/// #     role: a2a_types::MessageRole::User,
/// #     parts: vec![],
/// #     context_id: None,
/// #     task_id: None,
/// #     reference_task_ids: vec![],
/// #     extensions: vec![],
/// #     metadata: None,
/// # };
/// let message_with_trace = inject_trace_context(message);
/// ```
pub fn inject_trace_context(mut message: Message) -> Message {
    let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
    let context = opentelemetry::Context::current();

    let mut carrier = HashMap::new();
    propagator.inject_context(&context, &mut carrier);

    let mut metadata = message.metadata.unwrap_or_default();
    if let Some(traceparent) = carrier.get("traceparent") {
        metadata.insert(
            "traceparent".to_string(),
            serde_json::Value::String(traceparent.clone()),
        );
    }
    if let Some(tracestate) = carrier.get("tracestate") {
        metadata.insert(
            "tracestate".to_string(),
            serde_json::Value::String(tracestate.clone()),
        );
    }

    message.metadata = Some(metadata);
    message
}

/// Extract trace context from A2A message metadata
///
/// Returns the OpenTelemetry context if trace headers are present.
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::extract_trace_context;
/// # use a2a_types::Message;
///
/// # let message = Message {
/// #     kind: "message".to_string(),
/// #     message_id: "test".to_string(),
/// #     role: a2a_types::MessageRole::User,
/// #     parts: vec![],
/// #     context_id: None,
/// #     task_id: None,
/// #     reference_task_ids: vec![],
/// #     extensions: vec![],
/// #     metadata: None,
/// # };
/// if let Some(parent_context) = extract_trace_context(&message) {
///     // Use context to create child span
/// }
/// ```
pub fn extract_trace_context(message: &Message) -> Option<opentelemetry::Context> {
    let metadata = message.metadata.as_ref()?;
    let traceparent = metadata.get("traceparent")?.as_str()?;

    let mut carrier = HashMap::new();
    carrier.insert("traceparent".to_string(), traceparent.to_string());

    if let Some(tracestate) = metadata.get("tracestate").and_then(|v| v.as_str()) {
        carrier.insert("tracestate".to_string(), tracestate.to_string());
    }

    let propagator = opentelemetry_sdk::propagation::TraceContextPropagator::new();
    Some(propagator.extract(&carrier))
}

/// Extract trace context with security checks
///
/// Only accepts traces from trusted sources when reject_untrusted_traces is enabled.
///
/// # Arguments
/// * `message` - The A2A message containing trace context
/// * `sender_agent_name` - The name of the agent that sent this message
/// * `config` - Telemetry configuration with trust settings
///
/// # Security
/// - If `reject_untrusted_traces` is true and `trusted_trace_sources` is empty, all traces are rejected
/// - If `reject_untrusted_traces` is true, only traces from whitelisted sources are accepted
/// - If `reject_untrusted_traces` is false, all traces are accepted
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::{extract_trace_context_safe, TelemetryConfig};
/// # use a2a_types::Message;
/// # use std::collections::HashSet;
///
/// let config = TelemetryConfig {
///     trusted_trace_sources: ["agent-1".to_string()].iter().cloned().collect(),
///     reject_untrusted_traces: true,
///     ..Default::default()
/// };
///
/// # let message = Message {
/// #     kind: "message".to_string(),
/// #     message_id: "test".to_string(),
/// #     role: a2a_types::MessageRole::User,
/// #     parts: vec![],
/// #     context_id: None,
/// #     task_id: None,
/// #     reference_task_ids: vec![],
/// #     extensions: vec![],
/// #     metadata: None,
/// # };
/// if let Some(context) = extract_trace_context_safe(&message, "agent-1", &config) {
///     // Trace is from trusted source
/// }
/// ```
pub fn extract_trace_context_safe(
    message: &Message,
    sender_agent_name: &str,
    config: &TelemetryConfig,
) -> Option<opentelemetry::Context> {
    if config.reject_untrusted_traces {
        // If rejecting untrusted traces, we must have a whitelist
        if config.trusted_trace_sources.is_empty() {
            tracing::warn!(
                "Rejecting trace from '{}' - reject_untrusted_traces=true but no trusted sources configured",
                sender_agent_name
            );
            return None;
        }

        // Check if sender is in whitelist
        if !config
            .trusted_trace_sources
            .contains(sender_agent_name)
        {
            tracing::warn!(
                "Rejecting trace from untrusted source: '{}' (not in whitelist)",
                sender_agent_name
            );
            return None;
        }
    }

    extract_trace_context(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2a_types::MessageRole;

    fn create_test_message() -> Message {
        Message {
            kind: "message".to_string(),
            message_id: "test-123".to_string(),
            role: MessageRole::User,
            parts: vec![],
            context_id: None,
            task_id: None,
            reference_task_ids: vec![],
            extensions: vec![],
            metadata: None,
        }
    }

    #[test]
    fn test_inject_trace_context() {
        let message = create_test_message();
        let message_with_trace = inject_trace_context(message);

        assert!(message_with_trace.metadata.is_some());
        let metadata = message_with_trace.metadata.unwrap();
        // Note: traceparent will be present if there's an active span context
        // In this test environment, it might not be present if no span is active
        assert!(metadata.contains_key("traceparent") || metadata.is_empty());
    }

    #[test]
    fn test_extract_trace_context_no_metadata() {
        let message = create_test_message();
        let context = extract_trace_context(&message);
        assert!(context.is_none());
    }

    #[test]
    fn test_extract_trace_context_safe_untrusted() {
        let config = TelemetryConfig {
            trusted_trace_sources: ["agent-1".to_string()].iter().cloned().collect(),
            reject_untrusted_traces: true,
            ..Default::default()
        };

        let message = create_test_message();
        let context = extract_trace_context_safe(&message, "agent-2", &config);
        assert!(context.is_none()); // agent-2 not in whitelist
    }

    #[test]
    fn test_extract_trace_context_safe_trusted() {
        let config = TelemetryConfig {
            trusted_trace_sources: ["agent-1".to_string()].iter().cloned().collect(),
            reject_untrusted_traces: true,
            ..Default::default()
        };

        let message = create_test_message();
        // Even though agent-1 is trusted, there's no trace context in the message
        let context = extract_trace_context_safe(&message, "agent-1", &config);
        assert!(context.is_none());
    }

    #[test]
    fn test_extract_trace_context_safe_empty_whitelist() {
        let config = TelemetryConfig {
            trusted_trace_sources: Default::default(),
            reject_untrusted_traces: true,
            ..Default::default()
        };

        let message = create_test_message();
        let context = extract_trace_context_safe(&message, "agent-1", &config);
        assert!(context.is_none()); // Empty whitelist with reject_untrusted_traces = true
    }

    #[test]
    fn test_extract_trace_context_safe_permissive() {
        let config = TelemetryConfig {
            trusted_trace_sources: Default::default(),
            reject_untrusted_traces: false, // Accept all
            ..Default::default()
        };

        let message = create_test_message();
        let context = extract_trace_context_safe(&message, "any-agent", &config);
        // Will be None because there's no trace in the message, not because of trust check
        assert!(context.is_none());
    }
}
