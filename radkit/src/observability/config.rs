//! Configuration types for RadKit observability
//!
//! This module provides configuration for telemetry backends, sampling strategies,
//! and LLM pricing models.

use std::collections::HashSet;
use std::time::Duration;

/// Telemetry backend configuration
#[derive(Debug, Clone)]
pub enum TelemetryBackend {
    /// Telemetry is disabled
    Disabled,

    /// Use existing global tracer/meter providers (Pattern 4 - for embedded library use)
    UseGlobal,

    /// Export via OTLP gRPC
    OtlpGrpc {
        endpoint: String,
        timeout: Duration,
        headers: Vec<(String, String)>,
    },

    /// Export via OTLP HTTP
    OtlpHttp {
        endpoint: String,
        timeout: Duration,
        headers: Vec<(String, String)>,
    },

    /// Output to console (for development)
    Console,
}

impl TelemetryBackend {
    /// Create backend from environment variables
    pub fn from_env() -> Self {
        // Check if explicitly disabled
        if std::env::var("RADKIT_DISABLE_TRACING")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
        {
            return TelemetryBackend::Disabled;
        }

        // Check for OTLP endpoint
        if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
            return TelemetryBackend::OtlpGrpc {
                endpoint,
                timeout: Duration::from_secs(10),
                headers: vec![],
            };
        }

        // Default to Console in debug builds, Disabled in release
        #[cfg(debug_assertions)]
        return TelemetryBackend::Console;

        #[cfg(not(debug_assertions))]
        return TelemetryBackend::Disabled;
    }

    /// Check if backend is enabled
    pub fn is_enabled(&self) -> bool {
        !matches!(self, TelemetryBackend::Disabled)
    }

    /// Check if backend uses global providers
    pub fn uses_global(&self) -> bool {
        matches!(self, TelemetryBackend::UseGlobal)
    }
}

/// Sampling strategy for traces
#[derive(Debug, Clone)]
pub enum SamplingStrategy {
    /// Sample a fixed ratio of traces (0.0 to 1.0)
    Ratio(f64),

    /// Parent-based sampling with default ratio for root spans
    ParentBased { default_ratio: f64 },

    /// Different sampling rates for success vs error
    ErrorBased {
        success_ratio: f64,
        error_ratio: f64,
    },

    /// Always sample (for testing)
    AlwaysOn,

    /// Never sample
    AlwaysOff,
}

/// LLM pricing configuration for cost tracking
#[derive(Debug, Clone)]
pub struct LlmPricing {
    /// Model name/identifier
    pub model: String,

    /// Price per 1K prompt tokens (USD)
    pub prompt_price: f64,

    /// Price per 1K completion tokens (USD)
    pub completion_price: f64,
}

/// Main telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for identification in traces
    pub service_name: String,

    /// Backend configuration
    pub backend: TelemetryBackend,

    /// Sampling strategy
    pub sampling_strategy: SamplingStrategy,

    /// Enable console output in addition to telemetry
    pub enable_console: bool,

    /// Log level filter
    pub log_level: String,

    /// Redact PII from traces (hash user IDs, sanitize tool args)
    pub redact_pii: bool,

    /// Custom LLM pricing (overrides built-in defaults)
    pub llm_pricing: Vec<LlmPricing>,

    /// Trusted sources for A2A trace propagation
    pub trusted_trace_sources: HashSet<String>,

    /// Reject traces from untrusted sources
    pub reject_untrusted_traces: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        // Read sample rate from environment or default to 10%
        let sample_rate = std::env::var("RADKIT_TRACE_SAMPLE_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.1);

        Self {
            service_name: std::env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "radkit".to_string()),
            backend: TelemetryBackend::from_env(),
            sampling_strategy: SamplingStrategy::ParentBased {
                default_ratio: sample_rate,
            },
            enable_console: cfg!(debug_assertions),
            log_level: std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".to_string()),
            redact_pii: true,
            llm_pricing: vec![],
            trusted_trace_sources: HashSet::new(),
            reject_untrusted_traces: false,
        }
    }
}

impl TelemetryConfig {
    /// Check if telemetry is enabled
    pub fn is_enabled(&self) -> bool {
        self.backend.is_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_env_disabled() {
        unsafe {
            std::env::set_var("RADKIT_DISABLE_TRACING", "true");
        }
        let backend = TelemetryBackend::from_env();
        assert!(matches!(backend, TelemetryBackend::Disabled));
        unsafe {
            std::env::remove_var("RADKIT_DISABLE_TRACING");
        }
    }

    #[test]
    fn test_backend_is_enabled() {
        assert!(!TelemetryBackend::Disabled.is_enabled());
        assert!(TelemetryBackend::Console.is_enabled());
        assert!(TelemetryBackend::UseGlobal.is_enabled());
    }

    #[test]
    fn test_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "radkit");
        assert!(config.redact_pii);
        assert!(!config.reject_untrusted_traces);
    }

    #[test]
    fn test_sampling_strategy() {
        let ratio = SamplingStrategy::Ratio(0.1);
        assert!(matches!(ratio, SamplingStrategy::Ratio(r) if r == 0.1));
    }
}
