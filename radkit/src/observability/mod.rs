//! RadKit Observability
//!
//! This module provides comprehensive observability for RadKit using OpenTelemetry.
//!
//! # Features
//! - **4 Integration Patterns**: Standalone, Embedded, Multi-agent, Use-global
//! - **Distributed Tracing**: W3C Trace Context with parent-based sampling
//! - **Metrics**: Counters for LLM tokens, messages, and tool executions
//! - **A2A Trace Propagation**: Automatic trace context across agent boundaries
//! - **PII Redaction**: GDPR-compliant hashing and sanitization
//! - **Cost Tracking**: Configurable LLM pricing
//!
//! # Quick Start
//!
//! ## Pattern 1: RadKit as Main App
//! ```rust,no_run
//! use radkit::observability::{TelemetryConfig, init_telemetry};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let _guard = init_telemetry(TelemetryConfig::default())?;
//!     // Your app logic
//!     Ok(())
//! }
//! ```
//!
//! ## Pattern 2: Embedded Library
//! ```rust,no_run
//! use radkit::observability::create_telemetry_layer;
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let (otel_layer, _guard) = create_telemetry_layer(
//!         radkit::observability::TelemetryConfig::default()
//!     )?;
//!
//!     tracing_subscriber::registry()
//!         .with(tracing_subscriber::fmt::layer())
//!         .with(otel_layer)
//!         .init();
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Pattern 4: Use Parent's Telemetry
//! ```rust,no_run
//! use radkit::observability::{TelemetryConfig, TelemetryBackend, configure_radkit_telemetry};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Parent has already set up OpenTelemetry
//!     configure_radkit_telemetry(TelemetryConfig {
//!         backend: TelemetryBackend::UseGlobal,
//!         ..Default::default()
//!     })?;
//!
//!     Ok(())
//! }
//! ```

mod a2a_trace;
mod config;
mod metrics;
pub mod semantic_conventions;
mod telemetry;
pub mod utils;

// Re-export config types
pub use config::{LlmPricing, SamplingStrategy, TelemetryBackend, TelemetryConfig};

// Re-export telemetry initialization
pub use telemetry::{
    configure_radkit_telemetry, create_telemetry_layer, init_telemetry, ObservabilityError,
    TelemetryGuard, TelemetryLayerGuard,
};

// Re-export utility functions
pub use utils::{
    calculate_llm_cost, hash_user_id, record_error, record_llm_cost, record_llm_tokens,
    record_success, sanitize_tool_args,
};

// Re-export metrics functions
pub use metrics::{
    initialize_metrics, record_agent_message_metric, record_llm_tokens_metric,
    record_tool_execution_metric,
};

// Re-export A2A trace propagation
pub use a2a_trace::{extract_trace_context, extract_trace_context_safe, inject_trace_context};

// Re-export semantic conventions for convenience
pub use semantic_conventions::genai;
