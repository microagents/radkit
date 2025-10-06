//! OpenTelemetry metrics for RadKit
//!
//! This module provides metrics instrumentation with graceful degradation.
//! Metrics work via cached instruments for performance, with fallback to on-demand lookup.

use once_cell::sync::Lazy;
use opentelemetry::{global, metrics::Counter, KeyValue};
use std::sync::RwLock;

/// Cached metric instruments for performance
struct RadkitMetrics {
    llm_tokens: Counter<u64>,
    agent_messages: Counter<u64>,
    tool_executions: Counter<u64>,
}

/// Global metrics cache
static METRICS: Lazy<RwLock<Option<RadkitMetrics>>> = Lazy::new(|| RwLock::new(None));

/// Initialize metrics with cached instruments (OPTIONAL - recommended for performance)
///
/// Call this AFTER setting up the global meter provider (Pattern 4), or after
/// calling init_telemetry()/create_telemetry_layer() (Patterns 1-3).
///
/// If not called, metrics will still work but with slightly higher overhead
/// (meter and instrument lookup on each call).
///
/// # Example
/// ```rust,no_run
/// // Pattern 4 (use-global)
/// opentelemetry::global::set_meter_provider(parent_meter_provider);
/// radkit::observability::initialize_metrics();  // Optional but recommended
///
/// // Pattern 1 (standalone)
/// let _guard = radkit::observability::init_telemetry(config)?;
/// radkit::observability::initialize_metrics();  // Optional but recommended
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn initialize_metrics() {
    let meter = global::meter("radkit");

    let metrics = RadkitMetrics {
        llm_tokens: meter
            .u64_counter("radkit.llm.tokens")
            .with_description("Total LLM tokens consumed")
            .init(),
        agent_messages: meter
            .u64_counter("radkit.agent.messages")
            .with_description("Total agent messages processed")
            .init(),
        tool_executions: meter
            .u64_counter("radkit.tool.executions")
            .with_description("Total tool executions")
            .init(),
    };

    if let Ok(mut m) = METRICS.write() {
        *m = Some(metrics);
    }
}

/// Record LLM token usage as metrics
///
/// Graceful degradation: works with or without initialize_metrics() call.
pub fn record_llm_tokens_metric(model: &str, prompt_tokens: u64, completion_tokens: u64) {
    let total = prompt_tokens + completion_tokens;
    let attrs = [
        KeyValue::new("model", model.to_string()),
        KeyValue::new("token_type", "total"),
    ];

    // Try cached metrics first (fast path)
    if let Ok(metrics) = METRICS.read() {
        if let Some(m) = metrics.as_ref() {
            m.llm_tokens.add(total, &attrs);
            return;
        }
    }

    // Fallback: get meter on-demand (slower but works without initialization)
    let meter = global::meter("radkit");
    let counter = meter
        .u64_counter("radkit.llm.tokens")
        .with_description("Total LLM tokens consumed")
        .init();
    counter.add(total, &attrs);
}

/// Record agent message processing
///
/// Graceful degradation: works with or without initialize_metrics() call.
pub fn record_agent_message_metric(agent_name: &str, success: bool) {
    let attrs = [
        KeyValue::new("agent", agent_name.to_string()),
        KeyValue::new("success", success),
    ];

    // Try cached metrics first (fast path)
    if let Ok(metrics) = METRICS.read() {
        if let Some(m) = metrics.as_ref() {
            m.agent_messages.add(1, &attrs);
            return;
        }
    }

    // Fallback: get meter on-demand
    let meter = global::meter("radkit");
    let counter = meter
        .u64_counter("radkit.agent.messages")
        .with_description("Total agent messages processed")
        .init();
    counter.add(1, &attrs);
}

/// Record tool execution
///
/// Graceful degradation: works with or without initialize_metrics() call.
pub fn record_tool_execution_metric(tool_name: &str, success: bool) {
    let attrs = [
        KeyValue::new("tool", tool_name.to_string()),
        KeyValue::new("success", success),
    ];

    // Try cached metrics first (fast path)
    if let Ok(metrics) = METRICS.read() {
        if let Some(m) = metrics.as_ref() {
            m.tool_executions.add(1, &attrs);
            return;
        }
    }

    // Fallback: get meter on-demand
    let meter = global::meter("radkit");
    let counter = meter
        .u64_counter("radkit.tool.executions")
        .with_description("Total tool executions")
        .init();
    counter.add(1, &attrs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_graceful_degradation() {
        // This test verifies that metrics work gracefully even without explicit initialization
        // Simulating Pattern 4 where parent may or may not have set meter provider

        // Call metrics BEFORE initialize_metrics() - should not panic
        record_llm_tokens_metric("gpt-4", 100, 50);
        record_agent_message_metric("test-agent", true);
        record_tool_execution_metric("test-tool", true);

        // Metrics should work (even if they're no-ops due to no meter provider)
        // The key is: NO PANIC

        // Now initialize metrics (simulating late initialization)
        initialize_metrics();

        // Metrics should still work after initialization
        record_llm_tokens_metric("gpt-4", 200, 100);

        // This test documents the behavior:
        // - Calling metrics before initialization: Works (uses fallback)
        // - Calling initialize_metrics() after first use: Works (switches to cached)
        // - No panics, no crashes - graceful degradation
    }

    #[test]
    fn test_initialize_metrics_idempotent() {
        // Multiple calls to initialize_metrics should be safe
        initialize_metrics();
        initialize_metrics();

        record_llm_tokens_metric("gpt-4", 100, 50);
    }
}
