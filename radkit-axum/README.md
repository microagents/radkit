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

## Version Compatibility

| radkit-axum | axum  | radkit |
|-------------|-------|--------|
| 0.1.x       | 0.7.x | 0.1.x  |

## License

Apache-2.0