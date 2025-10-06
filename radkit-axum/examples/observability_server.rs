//! Complete observability example with OpenTelemetry, distributed tracing, and cost tracking
//!
//! This example demonstrates:
//! - OpenTelemetry OTLP export to Jaeger
//! - Automatic distributed tracing across RadKit operations
//! - LLM cost tracking with configurable pricing
//! - Multi-tenant support with PII redaction
//!
//! # Running this example
//!
//! 1. Start Jaeger:
//! ```bash
//! docker run -d --name jaeger \
//!   -p 16686:16686 \
//!   -p 4317:4317 \
//!   jaegertracing/all-in-one:latest
//! ```
//!
//! 2. Run the example:
//! ```bash
//! cargo run --example observability_server
//! ```
//!
//! 3. Send a request:
//! ```bash
//! curl -X POST http://localhost:3000/message/send \
//!   -H 'X-App-Name: myapp' \
//!   -H 'X-User-Id: user1' \
//!   -H 'Content-Type: application/json' \
//!   -d '{"jsonrpc":"2.0","method":"message/send","params":{"message":{"role":"user","messageId":"1","parts":[{"kind":"text","text":"Hello!"}]}},"id":1}'
//! ```
//!
//! 4. View traces at http://localhost:16686

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use radkit::agents::Agent;
use radkit::models::MockLlm;
use radkit::observability::{configure_radkit_telemetry, LlmPricing, TelemetryBackend, TelemetryConfig};
use radkit_axum::{async_trait, A2AServer, AuthContext, AuthExtractor};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Multi-tenant authentication with app_name and user_id from headers
#[derive(Clone)]
struct HeaderAuth;

#[async_trait]
impl AuthExtractor for HeaderAuth {
    async fn extract(
        &self,
        parts: &mut axum::http::request::Parts,
    ) -> Result<AuthContext, radkit_axum::auth::AuthError> {
        // Extract app_name from X-App-Name header
        let app_name = parts
            .headers
            .get("X-App-Name")
            .and_then(|v| v.to_str().ok())
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?
            .to_string();

        // Extract user_id from X-User-Id header
        let user_id = parts
            .headers
            .get("X-User-Id")
            .and_then(|v| v.to_str().ok())
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?
            .to_string();

        Ok(AuthContext { app_name, user_id })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup OpenTelemetry for your Axum server
    println!("🔧 Setting up OpenTelemetry...");

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317"),
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config().with_resource(Resource::new(vec![
                KeyValue::new("service.name", "radkit-a2a-server"),
            ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("radkit=debug".parse()?))
        .init();

    // 2. Configure RadKit to use your OpenTelemetry setup (Pattern 4)
    println!("🔧 Configuring RadKit observability...");

    configure_radkit_telemetry(TelemetryConfig {
        // Note: service_name is set on the tracer above, not here
        // UseGlobal uses the parent's tracer, so service.name comes from the Resource
        backend: TelemetryBackend::UseGlobal, // Use parent's telemetry
        redact_pii: true, // Hash user IDs for GDPR compliance
        llm_pricing: vec![
            LlmPricing {
                model: "mock-model".to_string(),
                prompt_price: 0.001,      // $0.001 per 1K tokens
                completion_price: 0.002,  // $0.002 per 1K tokens
            },
            LlmPricing {
                model: "claude-3-5-sonnet-20241022".to_string(),
                prompt_price: 0.003,
                completion_price: 0.015,
            },
        ],
        trusted_trace_sources: vec!["trusted-client".to_string()].into_iter().collect(),
        reject_untrusted_traces: false, // Accept traces from any source for demo
        ..Default::default()
    })?;

    // 3. Create your RadKit agent
    println!("🤖 Creating RadKit agent...");

    let agent = Agent::builder(
        "You are a helpful AI assistant with full observability.",
        MockLlm::new("mock-model".to_string()),
    )
    .with_card(|card| {
        card.with_name("Observability Demo Agent")
            .with_description("Demonstrates OpenTelemetry integration with RadKit")
            .with_version("1.0.0")
            .with_url("http://localhost:3000")
            .with_streaming(true)
            .add_skill_with("general-help", "General Assistance", |skill| {
                skill
                    .with_description("Helps with questions while tracking all operations")
                    .add_tag("observability")
                    .add_tag("demo")
            })
    })
    .with_builtin_task_tools()
    .build();

    // 4. Create A2A server
    println!("🚀 Starting A2A server with observability...");

    let server = A2AServer::builder(agent)
        .with_auth(HeaderAuth)
        .build();

    // Print usage instructions
    println!("\n✅ Server ready on http://localhost:3000");
    println!("\n📊 Observability Features:");
    println!("  • OpenTelemetry traces exported to Jaeger");
    println!("  • Automatic distributed tracing across RadKit operations");
    println!("  • LLM cost tracking with configurable pricing");
    println!("  • PII-safe user ID hashing (SHA256)");
    println!("  • Multi-tenant support (per app_name)");

    println!("\n🔍 View traces at: http://localhost:16686");

    println!("\n📝 Example request:");
    println!("  curl -X POST http://localhost:3000/message/send \\");
    println!("    -H 'X-App-Name: myapp' \\");
    println!("    -H 'X-User-Id: user123' \\");
    println!("    -H 'Content-Type: application/json' \\");
    println!("    -d '{{\"jsonrpc\":\"2.0\",\"method\":\"message/send\",\"params\":{{\"message\":{{\"role\":\"user\", \"messageId\": \"1\", \"parts\":[{{\"kind\":\"text\",\"text\":\"What can you help me with?\"}}]}}}},\"id\":1}}'");

    println!("\n📊 What you'll see in Jaeger:");
    println!("  • HTTP Request → POST /message/send");
    println!("  • RadKit Agent → radkit.agent.send_message");
    println!("  • Conversation → radkit.conversation.execute_core");
    println!("  • LLM Call → radkit.llm.generate_content");
    println!("  • Tool Execution → radkit.tools.process_calls");
    println!("  • All with distributed tracing across the entire flow!");

    println!("\n💡 Tips:");
    println!("  • Check span attributes for cost, tokens, user_id (hashed)");
    println!("  • Filter by 'service.name=radkit-a2a-server' in Jaeger");
    println!("  • Look for 'llm.cost_usd' attribute on LLM spans");
    println!("  • User IDs are SHA256 hashed for privacy (redact_pii=true)");

    // Start the server
    server.serve("0.0.0.0:3000").await?;

    // Shutdown OpenTelemetry
    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
