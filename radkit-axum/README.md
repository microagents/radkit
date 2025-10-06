# radkit-axum

Axum-based A2A (Agent-to-Agent) protocol server for Radkit agents.

This crate provides an HTTP server implementation that exposes Radkit agents as A2A-compliant endpoints, enabling them to communicate with other A2A agents and clients.

## Features

- **A2A Protocol Compliance**: Full implementation of A2A protocol methods
- **Flexible Authentication**: Pluggable auth system for various authentication strategies
- **Streaming Support**: Server-Sent Events (SSE) for real-time task updates
- **JSON-RPC 2.0**: Standards-compliant JSON-RPC transport
- **Agent Card Discovery**: Automatic agent capability discovery
- **Middleware Support**: Standard Axum middleware compatibility

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
radkit = "0.1"
radkit-axum = "0.1"
axum = "0.7"
tokio = { version = "1", features = ["full"] }
```

Basic usage:

```rust
use radkit::agents::Agent;
use radkit::models::MockLlm;
use radkit_axum::A2AServer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create your agent
    let llm = Arc::new(MockLlm::new("gpt-4".to_string()));
    let agent = Agent::builder(
        "You are a helpful AI assistant.".to_string(),
        llm,
    )
    .with_card(|c| c.with_name("my-agent".to_string()).with_description("A helpful assistant".to_string()))
    .build()
    .with_builtin_task_tools();
    
    // Create and run A2A server
    let server = A2AServer::builder(agent)
        .build();
    
    server.serve("0.0.0.0:3000").await?;
    Ok(())
}
```

## Authentication

Implement custom authentication by providing an `AuthExtractor`:

```rust
use radkit_axum::{AuthContext, AuthExtractor, async_trait};

#[derive(Clone)]
struct MyAuth;

#[async_trait]
impl AuthExtractor for MyAuth {
    async fn extract(&self, parts: &mut axum::http::request::Parts) 
        -> Result<AuthContext, radkit_axum::auth::AuthError> {
        
        // Extract from Authorization header, API keys, etc.
        let app_name = "my-app".to_string();
        let user_id = "user123".to_string();
        
        Ok(AuthContext { app_name, user_id, metadata: None })
    }
}

// Use it in your server
let server = A2AServer::builder(agent)
    .with_auth(MyAuth)
    .build();
```

## A2A Protocol Endpoints

The server automatically exposes these A2A-compliant endpoints:

- `POST /message/send` - Send messages to the agent
- `POST /message/stream` - Send messages with streaming responses (SSE)
- `POST /tasks/get` - Get task status and results
- `POST /tasks/cancel` - Cancel running tasks
- `GET /.well-known/agent-card.json` - Agent capability discovery
- `GET /health` - Health check

## Agent Card

Agent cards describe your agent's capabilities and are automatically generated:

```rust
let agent_card = a2a_types::AgentCard {
    name: "My AI Agent".to_string(),
    description: "Does amazing things".to_string(),
    capabilities: Some(a2a_types::AgentCapabilities {
        streaming: true,
        push_notifications: false,
        // ...
    }),
    skills: vec![
        a2a_types::AgentSkill {
            id: "general-help".to_string(),
            name: "General Assistance".to_string(),
            // ...
        }
    ],
    // ...
};

let server = A2AServer::builder(agent)
    .with_agent_card(agent_card)
    .build();
```

## Examples

See the `/examples` directory for complete examples:

- `basic_server.rs` - Simple API key authentication
- `jwt_server.rs` - JWT-based authentication

Run examples with:

```bash
cargo run --example basic_server
```

## Testing Your Server

Test with curl:

```bash
# Get agent card
curl http://localhost:3000/.well-known/agent-card.json

# Send a message (with default auth headers)
curl -X POST http://localhost:3000/message/send \
  -H 'X-App-Name: myapp' \
  -H 'X-User-Id: user1' \
  -H 'Content-Type: application/json' \
  -d '{
    "jsonrpc": "2.0",
    "method": "message/send",
    "params": {
      "message": {
        "role": "user",
        "parts": [{"kind": "text", "text": "Hello!"}]
      }
    },
    "id": 1
  }'
```

## Observability Integration

RadKit-Axum seamlessly integrates with your existing OpenTelemetry setup, providing full distributed tracing across HTTP requests, agent operations, and LLM calls.

### Automatic Trace Propagation

When you set up OpenTelemetry for your Axum server, RadKit automatically continues those traces:

```rust
use radkit_axum::A2AServer;
use radkit::observability::{configure_radkit_telemetry, TelemetryConfig, TelemetryBackend};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup OpenTelemetry for your Axum server
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317")
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config().with_resource(Resource::new(vec![
                KeyValue::new("service.name", "my-a2a-server"),
            ]))
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Configure RadKit to use your OpenTelemetry setup
    // Note: When using UseGlobal, set service.name on the tracer (above), not here
    configure_radkit_telemetry(TelemetryConfig {
        backend: TelemetryBackend::UseGlobal,  // Use parent's telemetry
        redact_pii: true,
        ..Default::default()
    })?;

    // 3. Create your RadKit agent
    let agent = Agent::builder("You are helpful", llm)
        .with_builtin_task_tools()
        .build();

    // 4. Start A2A server - all traces automatically connected!
    let server = A2AServer::builder(agent).build();
    server.serve("0.0.0.0:3000").await?;

    Ok(())
}
```

### What You Get

**Complete Distributed Trace:**
```
HTTP Request (client)
  └─ POST /message/send (axum handler)
     └─ radkit.agent.send_message
        └─ radkit.conversation.execute_core
           ├─ radkit.llm.generate_content (Anthropic Claude)
           │  └─ HTTP POST https://api.anthropic.com
           └─ radkit.tools.process_calls
              └─ radkit.tool.execute_call (update_status)
```

**All with the same trace ID!**

### Adding HTTP Trace Context Extraction (Optional)

For incoming HTTP requests with trace context headers, add middleware to extract them:

```rust
use axum::{middleware, Router};
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;

async fn trace_extractor_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    // Extract W3C Trace Context from HTTP headers
    let propagator = TraceContextPropagator::new();
    let parent_context = propagator.extract(&opentelemetry_http::HeaderExtractor(req.headers()));

    // Attach parent context for this request scope
    let _guard = parent_context.attach();

    // Continue processing - all spans will be children of incoming trace
    next.run(req).await
}

// Add to your A2A server
let server = A2AServer::builder(agent)
    .with_middleware(middleware::from_fn(trace_extractor_middleware))
    .build();
```

Now external clients can send trace context:
```bash
curl -X POST http://localhost:3000/message/send \
  -H 'traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"message/send","params":{...},"id":1}'
```

And your traces will continue from the client's trace ID!

### Metrics and Cost Tracking

Every request automatically records:

**Metrics:**
- `radkit.agent.messages` - Messages processed by agent/role
- `radkit.llm.tokens` - Token usage by model
- `radkit.tool.executions` - Tool executions by name/success

**Span Attributes:**
- LLM provider, model, token counts, cost per request
- Agent name, user ID (hashed), session ID
- Tool names, durations, success rates
- Error messages and status codes

### Viewing Traces

Send traces to any OpenTelemetry backend:

**Jaeger:**
```rust
let tracer = opentelemetry_jaeger::new_agent_pipeline()
    .with_service_name("my-a2a-server")
    .install_batch(opentelemetry_sdk::runtime::Tokio)?;
```

**Zipkin:**
```rust
let tracer = opentelemetry_zipkin::new_pipeline()
    .with_service_name("my-a2a-server")
    .install_batch(opentelemetry_sdk::runtime::Tokio)?;
```

**Datadog, Honeycomb, New Relic, etc.:**
```rust
let tracer = opentelemetry_otlp::new_pipeline()
    .tracing()
    .with_exporter(
        opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint("https://your-backend.com:4317")
    )
    .install_batch(opentelemetry_sdk::runtime::Tokio)?;
```

### Security & Privacy

**PII Protection:**
```rust
configure_radkit_telemetry(TelemetryConfig {
    redact_pii: true,  // User IDs are SHA256 hashed
    ..Default::default()
})?;
```

**Trusted A2A Trace Propagation:**
```rust
configure_radkit_telemetry(TelemetryConfig {
    trusted_trace_sources: ["agent-a", "agent-b"].iter().map(|s| s.to_string()).collect(),
    reject_untrusted_traces: true,  // Only accept traces from trusted agents
    ..Default::default()
})?;
```

### Complete Example

See `examples/observability_server.rs` for a complete working example with:
- OpenTelemetry OTLP export to Jaeger
- HTTP trace context extraction
- Cost tracking configuration
- Multi-tenant support

1. Run Jaeger locally
```bash
docker run -d --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  jaegertracing/all-in-one:latest
```

2. Run the example
```bash
cargo run --example observability_server
```

3. Send a request:
```bash
curl -X POST http://localhost:3000/message/send \
  -H 'X-App-Name: myapp' \
  -H 'X-User-Id: user1' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"message/send","params":{"message":{"role":"user","messageId":"1","parts":[{"kind":"text","text":"Hello!"}]}},"id":1}'
```

4. View traces at http://localhost:16686


## Version Compatibility

| radkit-axum | axum  | radkit | opentelemetry |
|-------------|-------|--------|---------------|
| 0.1.x       | 0.7.x | 0.1.x  | 0.22.x        |

## License

Apache-2.0