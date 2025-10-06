# radkit

Core implementation of the Rust AI Agent Development Kit.

## Overview

This crate provides the main Radkit functionality for building AI agents:

- **Agent System** - Complete agent implementation with A2A protocol support
- **LLM Integration** - Support for Anthropic, Google Gemini, OpenAI
- **Tool System** - Extensible tool framework with built-in task management
- **Event System** - Event-driven architecture with publish/subscribe
- **Session Management** - State and conversation management

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
radkit = "0.0.2"
```

Create an agent:

```rust
use radkit::agents::Agent;
use radkit::models::anthropic_llm::AnthropicLlm;
use radkit::tools::builtin_tools;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create LLM
    let llm = AnthropicLlm::new("your-api-key", "claude-3-5-sonnet-latest");
    
    // Create agent with built-in tools
    let agent = Agent::builder(
        "app-789",
        "Hello, how can you help me?",
        None,
    )
    .with_card(|c| c.with_name(llm)
        .with_builtin_task_tools()
        .build();
    
    // Send a message
    let response = agent.send_message(
        "task-123").with_description("user-456"))
    .build().await?;
    
    println!("Agent response: {:?}", response);
    Ok(())
}
```

## Features

### Multi-Provider LLM Support
- Anthropic Claude
- Google Gemini
- OpenAI GPT models
- Mock LLM for testing

### Tool System
- Function calling support
- Built-in task management tools
- Custom tool creation
- MCP (Model Context Protocol) integration

### Event-Driven Architecture
- Real-time event streaming
- Event sourcing for tasks
- Publish/subscribe pattern
- Session event history

### A2A Protocol Native
- Full A2A protocol implementation
- Task lifecycle management
- Streaming message support
- Artifact persistence

### Production Observability

RadKit includes comprehensive OpenTelemetry instrumentation that integrates seamlessly with your existing observability stack.

#### Seamless Integration with Existing OpenTelemetry

If your service already uses OpenTelemetry, RadKit automatically continues those traces:

```rust
use radkit::observability::{configure_radkit_telemetry, TelemetryConfig, TelemetryBackend};

// After your service sets up OpenTelemetry
opentelemetry::global::set_tracer_provider(your_tracer_provider);

// Configure RadKit to use your existing setup
configure_radkit_telemetry(TelemetryConfig {
    backend: TelemetryBackend::UseGlobal,  // Use parent's telemetry
    redact_pii: true,
    ..Default::default()
})?;

// Now all RadKit operations appear in your traces!
let agent = Agent::builder(instruction, llm).build();
agent.send_message(app, user, params).await?;  // Traced automatically
```

#### What Gets Traced

- ✅ **Agent Operations**: send_message, send_streaming_message with user context
- ✅ **Conversation Loops**: Iteration counts, task IDs, success/error status
- ✅ **LLM Calls**: Provider, model, token usage, cost per request
- ✅ **Tool Executions**: Individual tool calls with duration and success rates
- ✅ **Error Tracking**: Automatic error recording with full context

#### Example: HTTP Service with RadKit

```rust
use axum::{Router, routing::post};
use radkit::observability::{configure_radkit_telemetry, TelemetryConfig, TelemetryBackend};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup your service's OpenTelemetry
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(telemetry_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 2. Configure RadKit to use your telemetry
    configure_radkit_telemetry(TelemetryConfig {
        backend: TelemetryBackend::UseGlobal,
        redact_pii: true,
        ..Default::default()
    })?;

    // 3. Create your agent
    let agent = Agent::builder("You are helpful", llm).build();

    // 4. HTTP handler - traces automatically connect!
    let app = Router::new()
        .route("/chat", post(|body: Json<ChatRequest>| async move {
            // This span will be a child of the HTTP request span
            let result = agent.send_message(
                body.app_name,
                body.user_id,
                body.params
            ).await?;

            Ok(Json(result))
        }));

    // Now: HTTP request → Agent → LLM → Tools all in one trace!
    axum::serve(listener, app).await?;
    Ok(())
}
```

#### Trace Propagation

RadKit supports W3C Trace Context propagation across service boundaries:

```rust
// Sending to another RadKit agent
use radkit::observability::inject_trace_context;

let message_with_trace = inject_trace_context(a2a_message);
other_agent.send(message_with_trace).await?;

// Receiving from another agent
use radkit::observability::extract_trace_context_safe;

let parent_context = extract_trace_context_safe(&message, sender, &config);
// Your spans become children of the sender's trace
```

#### Cost Tracking

Built-in LLM cost calculation with configurable pricing:

```rust
let config = TelemetryConfig {
    llm_pricing: vec![
        LlmPricing {
            model: "claude-3-5-sonnet-20241022".to_string(),
            prompt_price: 0.003,      // $0.003 per 1K tokens
            completion_price: 0.015,  // $0.015 per 1K tokens
        }
    ],
    ..Default::default()
};
```

Every LLM call automatically records:
- Token usage (prompt, completion, total)
- Cost in USD
- Model and provider
- Request/response metadata

#### Security & Privacy

- **PII Redaction**: User IDs are SHA256 hashed before tracing
- **Sensitive Data Sanitization**: API keys, tokens, passwords redacted from tool args
- **Trusted Sources**: Only accept trace context from whitelisted agents

```rust
let config = TelemetryConfig {
    redact_pii: true,
    trusted_trace_sources: ["agent-a", "api-gateway"].iter().map(|s| s.to_string()).collect(),
    reject_untrusted_traces: true,
    ..Default::default()
};
```

#### Metrics Available

- `radkit.agent.messages` - Message count by agent and role
- `radkit.llm.tokens` - Token usage by model
- `radkit.tool.executions` - Tool execution count and success rate

## Documentation

For detailed documentation, see the [main README](https://github.com/microagents/radkit).

## License

Apache-2.0