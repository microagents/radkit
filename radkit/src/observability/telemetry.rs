//! Telemetry initialization for RadKit
//!
//! This module provides three initialization patterns:
//! - Pattern 1: `init_telemetry()` - RadKit as main app
//! - Pattern 2: `create_telemetry_layer()` - RadKit as embedded library
//! - Pattern 4: `configure_radkit_telemetry()` - Use existing parent telemetry

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::runtime;
use opentelemetry_sdk::trace::{Sampler, TracerProvider};
use tracing_subscriber::Layer;

use crate::observability::config::{SamplingStrategy, TelemetryBackend, TelemetryConfig};

/// Guard for Pattern 1 (standalone app) - manages subscriber lifetime
pub struct TelemetryGuard {
    _tracer_provider: Option<TracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        //   WARNING: This can block indefinitely if OTLP endpoint is unreachable
        //
        // OpenTelemetry's shutdown_tracer_provider() tries to flush all pending spans
        // to the configured backend. If the backend is down or network is slow, this
        // can hang.
        //
        // Mitigation options:
        // 1. Set reasonable OTLP timeout in config (recommended: 10s max)
        // 2. Use tokio::time::timeout() in your application shutdown logic
        // 3. Accept that shutdown may be slow in degraded network conditions
        //
        // For production apps, wrap shutdown in timeout:
        // tokio::time::timeout(Duration::from_secs(30), async { drop(guard) }).await
        opentelemetry::global::shutdown_tracer_provider();
    }
}

/// Guard for Pattern 2 (embedded library) - manages tracer provider lifetime
pub struct TelemetryLayerGuard {
    _tracer_provider: Option<TracerProvider>,
}

impl Drop for TelemetryLayerGuard {
    fn drop(&mut self) {
        // Provider's Drop impl handles shutdown
    }
}

/// Error types for observability operations
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    #[error("Telemetry is disabled")]
    Disabled,

    #[error("Global subscriber already initialized")]
    AlreadyInitialized,

    #[error("OpenTelemetry error: {0}")]
    OpenTelemetry(#[from] opentelemetry::trace::TraceError),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Pattern 1: Initialize telemetry for standalone applications
///
/// RadKit manages the global subscriber. Use this when RadKit is your main application.
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::{TelemetryConfig, init_telemetry};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // IMPORTANT: Keep this guard alive!
///     let _telemetry_guard = match init_telemetry(TelemetryConfig::default()) {
///         Ok(guard) => {
///             tracing::info!(" Observability initialized successfully");
///             Some(guard)
///         }
///         Err(e) => {
///             eprintln!("  WARNING: Failed to initialize observability: {}", e);
///             eprintln!("    Application will continue WITHOUT observability");
///             None  // Graceful degradation - app continues working
///         }
///     };
///
///     // Your app logic here
///     Ok(())
/// }
/// ```
pub fn init_telemetry(config: TelemetryConfig) -> Result<TelemetryGuard, ObservabilityError> {
    let (tracer, tracer_provider) = if config.is_enabled() {
        let (t, p) = create_tracer_global(&config)?;
        (Some(t), p) // p is already Option<TracerProvider>
    } else {
        (None, None)
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    let otel_layer = tracer.map(|t| tracing_opentelemetry::layer().with_tracer(t));

    let fmt_layer = if config.enable_console {
        Some(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .compact(),
        )
    } else {
        None
    };

    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    tracing_subscriber::registry()
        .with(env_filter)
        .with(otel_layer)
        .with(fmt_layer)
        .try_init()
        .map_err(|_| ObservabilityError::AlreadyInitialized)?;

    Ok(TelemetryGuard {
        _tracer_provider: tracer_provider,
    })
}

/// Pattern 2: Create telemetry layer for embedded library use
///
/// Parent application controls the subscriber. Use this when RadKit is embedded in a larger system.
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::{TelemetryConfig, create_telemetry_layer};
/// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Parent controls subscriber
///     let (otel_layer, _otel_guard) = create_telemetry_layer(
///         TelemetryConfig::default()
///     )?;
///
///     tracing_subscriber::registry()
///         .with(tracing_subscriber::fmt::layer())
///         .with(otel_layer)
///         .init();
///
///     Ok(())
/// }
/// ```
pub fn create_telemetry_layer<S>(
    config: TelemetryConfig,
) -> Result<(Option<impl Layer<S>>, Option<TelemetryLayerGuard>), ObservabilityError>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    if !config.is_enabled() {
        return Ok((None, None));
    }

    let (tracer, tracer_provider) = create_tracer_local(&config)?;

    let guard = TelemetryLayerGuard {
        _tracer_provider: tracer_provider, // Already Option<TracerProvider>
    };

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((Some(layer), Some(guard)))
}

/// Pattern 4: Configure RadKit to use existing global telemetry
///
/// Use this when parent application has already set up OpenTelemetry.
/// RadKit will use the global tracer provider and respect parent's configuration.
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::{TelemetryConfig, TelemetryBackend, configure_radkit_telemetry};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Parent already set up OpenTelemetry
///     // opentelemetry::global::set_tracer_provider(parent_tracer_provider);
///
///     // RadKit uses global tracer, just configure preferences
///     configure_radkit_telemetry(TelemetryConfig {
///         service_name: "radkit-component".to_string(),
///         backend: TelemetryBackend::UseGlobal,
///         redact_pii: true,
///         ..Default::default()
///     })?;
///
///     Ok(())
/// }
/// ```
pub fn configure_radkit_telemetry(config: TelemetryConfig) -> Result<(), ObservabilityError> {
    // Store config globally for PII redaction, pricing, and other preferences
    crate::observability::utils::set_global_config(config);
    Ok(())
}

/// Create tracer and set it as global (for Pattern 1)
fn create_tracer_global(
    config: &TelemetryConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, Option<TracerProvider>), ObservabilityError> {
    let (tracer, provider) = create_tracer_impl(config)?;
    // Only set global provider if we created one (not for UseGlobal)
    if let Some(ref p) = provider {
        opentelemetry::global::set_tracer_provider(p.clone());
    }
    Ok((tracer, provider))
}

/// Create tracer without setting global (for Pattern 2)
fn create_tracer_local(
    config: &TelemetryConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, Option<TracerProvider>), ObservabilityError> {
    create_tracer_impl(config)
}

/// Core tracer creation implementation
fn create_tracer_impl(
    config: &TelemetryConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, Option<TracerProvider>), ObservabilityError> {
    if !config.is_enabled() {
        return Err(ObservabilityError::Disabled);
    }

    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    let sampler = match &config.sampling_strategy {
        SamplingStrategy::Ratio(r) => Sampler::TraceIdRatioBased(*r),
        SamplingStrategy::ParentBased { default_ratio } => {
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(*default_ratio)))
        }
        SamplingStrategy::AlwaysOn => Sampler::AlwaysOn,
        SamplingStrategy::AlwaysOff => Sampler::AlwaysOff,
        SamplingStrategy::ErrorBased { success_ratio, .. } => {
            // For now, use success_ratio as default
            // Could implement custom sampler for full error-based logic
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(*success_ratio)))
        }
    };

    match &config.backend {
        TelemetryBackend::Disabled => return Err(ObservabilityError::Disabled),

        TelemetryBackend::UseGlobal => {
            // Use existing global tracer provider
            // We need to create a local provider that wraps the global one
            use opentelemetry::trace::TracerProvider as _;
            let provider = TracerProvider::builder().build();
            let tracer = provider.tracer(config.service_name.clone());
            return Ok((tracer, None));
        }

        TelemetryBackend::OtlpGrpc {
            endpoint,
            timeout,
            headers,
        } => {
            let mut exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_timeout(*timeout);

            if !headers.is_empty() {
                let mut metadata = tonic::metadata::MetadataMap::new();
                for (k, v) in headers.iter() {
                    if let (Ok(key), Ok(value)) = (
                        k.parse::<tonic::metadata::MetadataKey<_>>(),
                        v.parse::<tonic::metadata::MetadataValue<_>>(),
                    ) {
                        metadata.insert(key, value);
                    }
                }
                exporter = exporter.with_metadata(metadata);
            }

            let tracer = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(exporter)
                .with_trace_config(
                    opentelemetry_sdk::trace::Config::default()
                        .with_sampler(sampler.clone())
                        .with_resource(resource.clone())
                        .with_max_events_per_span(32)
                        .with_max_attributes_per_span(128),
                )
                .install_batch(runtime::Tokio)?;

            // Note: install_batch() returns Tracer directly and manages provider globally
            Ok((tracer, None))
        }

        TelemetryBackend::OtlpHttp {
            endpoint,
            timeout,
            headers,
        } => {
            let mut exporter = opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint(endpoint)
                .with_timeout(*timeout);

            if !headers.is_empty() {
                let headers_map: std::collections::HashMap<String, String> = headers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                exporter = exporter.with_headers(headers_map);
            }

            let tracer = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(exporter)
                .with_trace_config(
                    opentelemetry_sdk::trace::Config::default()
                        .with_sampler(sampler.clone())
                        .with_resource(resource.clone())
                        .with_max_events_per_span(32)
                        .with_max_attributes_per_span(128),
                )
                .install_batch(runtime::Tokio)?;

            // Note: install_batch() returns Tracer directly and manages provider globally
            Ok((tracer, None))
        }

        TelemetryBackend::Console => {
            use opentelemetry::trace::TracerProvider as _;

            let tracer_provider = TracerProvider::builder()
                .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
                .with_config(
                    opentelemetry_sdk::trace::Config::default()
                        .with_sampler(sampler)
                        .with_resource(resource),
                )
                .build();

            let tracer = tracer_provider.tracer(config.service_name.clone());
            Ok((tracer, Some(tracer_provider)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_backend() {
        let config = TelemetryConfig {
            backend: TelemetryBackend::Disabled,
            ..Default::default()
        };

        let result = create_tracer_impl(&config);
        assert!(matches!(result, Err(ObservabilityError::Disabled)));
    }

    #[test]
    fn test_configure_radkit_telemetry() {
        let config = TelemetryConfig {
            backend: TelemetryBackend::UseGlobal,
            ..Default::default()
        };

        let result = configure_radkit_telemetry(config);
        assert!(result.is_ok());
    }
}
