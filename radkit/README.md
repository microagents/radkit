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

## Documentation

For detailed documentation, see the [main README](https://github.com/microagents/radkit).

## License

Apache-2.0