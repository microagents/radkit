---
hide:
  - toc
---

<div style="text-align: center;">
  <div class="centered-logo-text-group">
    <img src="assets/logo.svg" alt="RadKit Logo" width="100">
    <h1>Radkit - Rust Agent Development Kit</h1>
  </div>
</div>

## What is Radkit?

Radkit is an **agent framework** for building AI agents in Rust. It provides comprehensive support for the [**A2A (Agent-to-Agent) Protocol**](https://a2a-protocol.org), enabling seamless agent interoperability with zero conversion overhead.

Built with enterprise-grade architecture, Radkit offers:
- 🚀 **A2A-Native Design**: Unlike other agent frameworks (Autogen, CrewAI, Langchain, ADK) where a2a is a secondary layer, Radkit is built from the ground up to support the protocol natively.
- 🤖 **Multi-Provider LLM Support**: Anthropic Claude, Google Gemini, and more
- 🔧 **Advanced Tool System**: Function calling with built-in task management tools

## Quick Example

```rust
use radkit::a2a::{Message, MessageRole, MessageSendParams, Part, SendMessageResult};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::AnthropicLlm;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create an LLM provider
    let llm = Arc::new(AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY")?,
    ));
    
    // Create an agent with built-in tools (services created automatically)
    let agent = Agent::new(
        "Assistant".to_string(),
        "A helpful AI assistant".to_string(),
        "You are a helpful assistant that can manage tasks and save important information.".to_string(),
        llm,
    )
    .with_config(AgentConfig::default().with_max_iterations(10))
    .with_builtin_task_tools(); // Adds update_status and save_artifact tools
    
    // Send a message (non-streaming)
    let params = MessageSendParams {
        message: Message {
            kind: "message".to_string(),
            message_id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![Part::Text {
                text: "Help me calculate the sum of 1 to 100 and save the result.".to_string(),
                metadata: None,
            }],
            context_id: None,  // New session will be created
            task_id: None,     // New task will be created
            reference_task_ids: Vec::new(),
            extensions: Vec::new(),
            metadata: None,
        },
        configuration: None,
        metadata: None,
    };
    
    // Execute and get enriched results with events
    let response = agent.send_message(
        "my_app".to_string(),
        "user123".to_string(),
        params,
    ).await?;
    
    // Access the task result
    if let SendMessageResult::Task(task) = response.result {
        println!("Task completed: {}", task.id);
        println!("Status: {:?}", task.status.state);
        println!("Artifacts generated: {}", task.artifacts.len());
    }
    
    // Analyze captured events for monitoring
    println!("Internal events captured: {}", response.internal_events.len());
    println!("A2A protocol events: {}", response.a2a_events.len());
    
    Ok(())
}
```

## Key Features

### A2A Protocol Native
- **Protocol Methods**: Implements `message/send`, `message/stream`, `tasks/get`
- **Event Streaming**: Tools can generate compliant `TaskStatusUpdate` and `TaskArtifactUpdate` events

### Comprehensive Task System
- **Follows Task Lifecycle**: `Submitted → Working → [InputRequired/AuthRequired] → Completed/Failed`
- **Atomic Operations**: Thread-safe message, artifact, and status updates
- **A2A Events**: Automatic generation of protocol-compliant events
- **Built-in Tools**: `update_status` and `save_artifact` with event emission

## Installation

```toml
[dependencies]
radkit = { git = "https://github.com/microagents/radkit.git" }
tokio = { version = "1.47", features = ["full"] }
uuid = { version = "1.18", features = ["v4"] }
serde_json = "1.0"
futures = "0.3"
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
```

## Quick Start Guide

<div id="centered-install-tabs" class="install-command-container" markdown="1">
<p style="text-align:center;">
  <a href="getting-started/" class="md-button" style="margin:3px">Getting Started</a>
  <a href="tasks/" class="md-button" style="margin:3px">Task Guide</a>
  <a href="sessions/" class="md-button" style="margin:3px">Session Guide</a>
</p>
</div>

## Development Status

**Current Version**: 0.1.0 (Work in Progress - Major Architecture Complete)

✅ **Completed Features**:
- A2A Protocol core implementation with native types
- Multi-provider LLM support (Anthropic Claude, Google Gemini)
- Task lifecycle management with A2A event streaming
- Comprehensive tool system with built-in A2A tools

🚧 **Coming Soon**:
- State management with three-tier state isolation
- A2A Server mode (HTTP/gRPC endpoints for agent interoperability)
- A2A Client mode (call other A2A agents via function calling)
- MCP (Model Context Protocol) tools integration
- OpenAPI tool generation and validation
- Production persistent storage backends (PostgreSQL, Redis)
- WebSocket streaming support for real-time clients

## License

Radkit is licensed under the Apache 2.0 License. See [LICENSE](https://github.com/microagents//blob/main/LICENSE) for details.