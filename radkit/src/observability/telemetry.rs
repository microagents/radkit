//! Telemetry initialization for RadKit
//!
//! This module provides three initialization patterns:
//! - Pattern 1: `init_telemetry()` - RadKit as main app
//! - Pattern 2: `create_telemetry_layer()` - RadKit as embedded library
//! - Pattern 4: `configure_radkit_telemetry()` - Use existing parent telemetry

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_subscriber::Layer;

use crate::observability::config::{SamplingStrategy, TelemetryBackend, TelemetryConfig};

// OTLP exporter configuration traits
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig, WithTonicConfig};

/// Guard for Pattern 1 (standalone app) - manages subscriber lifetime
pub struct TelemetryGuard {
    _tracer_provider: Option<SdkTracerProvider>,
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

        // Note: In OTel 0.31+, SdkTracerProvider's Drop impl handles shutdown automatically
    }
}

/// Guard for Pattern 2 (embedded library) - manages tracer provider lifetime
pub struct TelemetryLayerGuard {
    _tracer_provider: Option<SdkTracerProvider>,
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
    OpenTelemetry(String),

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
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));

    // Handle UseGlobal separately to avoid type mismatch
    let (otel_layer, tracer_provider) = if matches!(config.backend, TelemetryBackend::UseGlobal) {
        if !config.is_enabled() {
            (None, None)
        } else {
            let tracer = opentelemetry::global::tracer(config.service_name.clone());
            let layer = tracing_opentelemetry::layer().with_tracer(tracer);
            (Some(layer), None)
        }
    } else if config.is_enabled() {
        let (tracer, provider) = create_tracer_global(&config)?;
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        (Some(layer), provider)
    } else {
        (None, None)
    };

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
/// This pattern supports all backends including UseGlobal:
/// - **UseGlobal**: Uses existing global provider (parent must set it first)
/// - **OTLP/Console**: Creates local provider without setting global
///
/// # Example
/// ```rust,no_run
/// use radkit::observability::{TelemetryConfig, TelemetryBackend, create_telemetry_layer};
/// use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Scenario 1: Parent has already set up OpenTelemetry
///     // let parent_provider = /* ... */;
///     // opentelemetry::global::set_tracer_provider(parent_provider);
///
///     // RadKit uses the existing global provider
///     let (otel_layer, _otel_guard) = create_telemetry_layer::<tracing_subscriber::Registry>(
///         TelemetryConfig {
///             backend: TelemetryBackend::UseGlobal,
///             ..Default::default()
///         }
///     )?;
///
///     // Parent controls subscriber composition
///     // Note: Compose layers in your parent application's subscriber setup
///     assert!(otel_layer.is_some());
///
///     Ok(())
/// }
/// ```
pub fn create_telemetry_layer<S>(
    config: TelemetryConfig,
) -> Result<
    (
        Option<Box<dyn Layer<S> + Send + Sync>>,
        Option<TelemetryLayerGuard>,
    ),
    ObservabilityError,
>
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    if !config.is_enabled() {
        return Ok((None, None));
    }

    // UseGlobal: Use existing global provider (parent must have set it)
    // This does NOT create or set any global state - just uses what's already there
    if matches!(config.backend, TelemetryBackend::UseGlobal) {
        let tracer = opentelemetry::global::tracer(config.service_name.clone());
        let layer = tracing_opentelemetry::layer().with_tracer(tracer);
        let guard = TelemetryLayerGuard {
            _tracer_provider: None,
        };
        return Ok((Some(Box::new(layer)), Some(guard)));
    }

    // Other backends: Create local tracer WITHOUT setting global
    // Parent app maintains full control over global state
    let (tracer, tracer_provider) = create_tracer_local(&config)?;

    let guard = TelemetryLayerGuard {
        _tracer_provider: tracer_provider,
    };

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((Some(Box::new(layer)), Some(guard)))
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
/// Returns BoxedTracer from global provider for type consistency with UseGlobal
fn create_tracer_global(
    config: &TelemetryConfig,
) -> Result<
    (
        opentelemetry::global::BoxedTracer,
        Option<SdkTracerProvider>,
    ),
    ObservabilityError,
> {
    let (_tracer, provider) = create_tracer_impl(config)?;
    // Set global provider and get BoxedTracer from it
    if let Some(ref p) = provider {
        opentelemetry::global::set_tracer_provider(p.clone());
        let boxed_tracer = opentelemetry::global::tracer(config.service_name.clone());
        Ok((boxed_tracer, provider))
    } else {
        Err(ObservabilityError::Config(
            "No tracer provider created".to_string(),
        ))
    }
}

/// Create tracer without setting global (for Pattern 2)
/// Returns SdkTracer directly - parent app controls global state
fn create_tracer_local(
    config: &TelemetryConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, Option<SdkTracerProvider>), ObservabilityError> {
    // Just create the tracer, don't touch global state
    create_tracer_impl(config)
}

/// Core tracer creation implementation
fn create_tracer_impl(
    config: &TelemetryConfig,
) -> Result<(opentelemetry_sdk::trace::Tracer, Option<SdkTracerProvider>), ObservabilityError> {
    if !config.is_enabled() {
        return Err(ObservabilityError::Disabled);
    }

    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();

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
        TelemetryBackend::Disabled => Err(ObservabilityError::Disabled),

        TelemetryBackend::UseGlobal => {
            // UseGlobal should be handled by caller (init_telemetry/create_telemetry_layer)
            // If we get here, it's a programming error
            return Err(ObservabilityError::Config(
                "UseGlobal backend must be handled by caller, not create_tracer_impl".to_string(),
            ));
        }

        TelemetryBackend::OtlpGrpc {
            endpoint,
            timeout,
            headers,
        } => {
            use opentelemetry_otlp::SpanExporter;

            // Build gRPC metadata from headers
            let mut metadata = tonic::metadata::MetadataMap::new();
            for (key, value) in headers {
                if let (Ok(k), Ok(v)) = (
                    tonic::metadata::MetadataKey::from_bytes(key.as_bytes()),
                    tonic::metadata::MetadataValue::try_from(value),
                ) {
                    metadata.insert(k, v);
                }
            }

            let mut exporter_builder = SpanExporter::builder().with_tonic();

            // Configure endpoint
            if !endpoint.is_empty() {
                exporter_builder = exporter_builder.with_endpoint(endpoint.clone());
            }

            // Configure timeout
            exporter_builder = exporter_builder.with_timeout(*timeout);

            // Configure headers as metadata
            if !metadata.is_empty() {
                exporter_builder = exporter_builder.with_metadata(metadata);
            }

            let exporter = exporter_builder
                .build()
                .map_err(|e| ObservabilityError::OpenTelemetry(e.to_string()))?;

            let tracer_provider = SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(resource.clone())
                .with_sampler(sampler)
                .build();

            use opentelemetry::trace::TracerProvider as _;
            let tracer = tracer_provider.tracer(config.service_name.clone());
            Ok((tracer, Some(tracer_provider)))
        }

        TelemetryBackend::OtlpHttp {
            endpoint,
            timeout,
            headers,
        } => {
            use opentelemetry_otlp::SpanExporter;
            use std::collections::HashMap;

            // Build HTTP headers map
            let headers_map: HashMap<String, String> = headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let mut exporter_builder = SpanExporter::builder().with_http();

            // Configure endpoint
            if !endpoint.is_empty() {
                exporter_builder = exporter_builder.with_endpoint(endpoint.clone());
            }

            // Configure timeout
            exporter_builder = exporter_builder.with_timeout(*timeout);

            // Configure headers
            if !headers_map.is_empty() {
                exporter_builder = exporter_builder.with_headers(headers_map);
            }

            let exporter = exporter_builder
                .build()
                .map_err(|e| ObservabilityError::OpenTelemetry(e.to_string()))?;

            let tracer_provider = SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(resource.clone())
                .with_sampler(sampler)
                .build();

            use opentelemetry::trace::TracerProvider as _;
            let tracer = tracer_provider.tracer(config.service_name.clone());
            Ok((tracer, Some(tracer_provider)))
        }

        TelemetryBackend::Console => {
            use opentelemetry::trace::TracerProvider as _;

            let tracer_provider = SdkTracerProvider::builder()
                .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
                .with_resource(resource)
                .with_sampler(sampler)
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

    #[test]
    fn test_create_telemetry_layer_supports_use_global() {
        // Pattern 2 should support UseGlobal without setting global provider
        let config = TelemetryConfig {
            backend: TelemetryBackend::UseGlobal,
            ..Default::default()
        };

        type TestSubscriber = tracing_subscriber::Registry;
        let result: Result<_, _> = create_telemetry_layer::<TestSubscriber>(config);

        // Should succeed and return a layer
        assert!(result.is_ok());
        let (layer_opt, guard_opt) = result.unwrap();
        assert!(layer_opt.is_some());
        assert!(guard_opt.is_some());

        // Guard should have no provider (using global, not creating)
        let guard = guard_opt.unwrap();
        assert!(guard._tracer_provider.is_none());
    }
}
