<div style="text-align: center;">
  <div class="centered-logo-text-group">
    <img src="docs/docs/assets/logo.svg" alt="RadKit Logo" width="100">
    <h1>Radkit - Rust Agent Development Kit</h1>
  </div>
</div>

## What is Radkit?

Radkit is an **agent framework** for building AI agents in Rust. It aims to provide comprehensive support for the [**A2A (Agent-to-Agent) Protocol**](https://a2a-protocol.org).

Radkit offers:
- 🚀 **A2A-Native Design**: Unlike other agent frameworks (Autogen, CrewAI, Langchain, ADK) where a2a is a secondary layer, Radkit is built from the ground up to support the protocol natively.
- 🤖 **Multi-Provider LLM Support**: Anthropic Claude, Google Gemini, OpenAI and more coming soon
- 🔧 **Advanced Tool System**: Function calling with built-in task management tools, MCP Tools

## Quick Example

```toml
[dependencies]
radkit = "0.0.2"
futures = "0.3.31"
tokio = "1.47.1"
uuid = "1.18.0"
dotenvy = "0.15.7"
```

Create a `.env` file with your Anthropic API key:
```dotenv
ANTHROPIC_API_KEY="YOUR_API_KEY"
````

Then, create an agent and send a message:

```rust
use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult, TaskState,
};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::AnthropicLlm;
use radkit::sessions::InMemorySessionService;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env file.
    dotenvy::dotenv()?;

    // Create an LLM provider (supports Anthropic, Gemini, and Mock providers)
    let llm = Arc::new(AnthropicLlm::new(
        "claude-3-5-sonnet-20241022".to_string(),
        std::env::var("ANTHROPIC_API_KEY")?,
    ));

    // Create an agent using the builder pattern with built-in task management tools
    let agent = Agent::builder(
        "You are a helpful research assistant. When working on tasks, \
         always use update_status to indicate your progress (e.g., 'working' \
         when processing, 'completed' when done). When you produce any \
         results or findings, save them using save_artifact so they can be \
         retrieved later."
            .to_string(),
        llm,
    )
    .with_card(|c| {
        c.with_name("research_assistant")
            .with_description("Research Assistant")
    })
    .with_config(AgentConfig::default().with_max_iterations(10))
    .with_session_service(Arc::new(InMemorySessionService::new()))
    .with_builtin_task_tools() // Enables update_status and save_artifact tools
    .build()?;

    // User simply asks for research - no mention of tools
    let message = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Can you research the Fibonacci sequence and create a \
                   summary of its properties and applications?"
                .to_string(),
            metadata: None,
        }],
        context_id: None, // New session will be created
        task_id: None,    // New task will be created
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    // Stream the execution to capture real-time A2A protocol events
    let mut execution = agent
        .send_streaming_message(
            "my_app".to_string(),
            "user123".to_string(),
            MessageSendParams {
                message,
                configuration: None,
                metadata: None,
            },
        )
        .await?;

    let mut status_events = 0;
    let mut artifact_events = 0;
    let mut final_task = None;

    // Process streaming results and capture A2A protocol events
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::TaskStatusUpdate(status_event) => {
                status_events += 1;
                println!("📊 Status Update: {:?}", status_event.status.state);

                if status_event.status.message.is_some() {
                    println!(
                        "  Message: {:?}",
                        status_event.status.message.as_ref().unwrap()
                    );
                }
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_event) => {
                artifact_events += 1;
                println!("💾 Artifact Saved: {:?}", artifact_event.artifact.name);
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Task Completed: {}", task.id);
                final_task = Some(task);
                break;
            }
            _ => {} // Handle other events as needed
        }
    }

    // Examine the final task state
    if let Some(task) = final_task {
        println!("\n📈 Final Results:");
        println!("  Task State: {:?}", task.status.state);
        println!("  Messages: {}", task.history.len());
        println!("  Artifacts: {}", task.artifacts.len());

        // Check the saved artifact content
        if let Some(artifact) = task.artifacts.first() {
            if let Some(Part::Text { text, .. }) = artifact.parts.first() {
                println!(
                    "  Artifact Content Preview: {}",
                    &text[..text.len().min(200)]
                );
            }
        }
    }
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

## Quick Start Guide

<div id="centered-install-tabs" class="install-command-container" markdown="1">
<p style="text-align:center;">
  <a href="docs/docs/getting-started.md" class="md-button" style="margin:3px">Getting Started</a>
  <a href="docs/docs/tools.md" class="md-button" style="margin:3px">Tools Guide</a>
  <a href="docs/docs/tasks.md" class="md-button" style="margin:3px">Task Guide</a>
  <a href="docs/docs/sessions.md" class="md-button" style="margin:3px">Session Guide</a>
</p>
</div>

## Development Status

**Current Version**: 0.0.1 (Work in Progress - Major Architecture Complete)

✅ **Completed Features**:
- A2A Protocol core implementation with native types
- Multi-provider LLM support (Anthropic Claude, Google Gemini)
- Task lifecycle management with A2A event streaming
- Comprehensive tool system with built-in A2A tools
- MCP (Model Context Protocol) tools integration
- Secure ToolContext with capability-based access control
- State management with three-tier state isolation (app/user/session)

🚧 **Coming Soon**:
- A2A Server mode (HTTP/gRPC endpoints for agent interoperability)
- A2A Client mode (call other A2A agents via function calling)
- OpenAPI tool generation and validation
- Production persistent storage backends (PostgreSQL, Redis)
- WebSocket streaming support for real-time clients

## License

Radkit is licensed under the Apache 2.0 License. See [LICENSE](https://github.com/microagents//blob/main/LICENSE) for details.