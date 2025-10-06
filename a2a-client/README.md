# A2A Client

A Rust HTTP client for calling remote A2A (Agent-to-Agent) protocol compliant agents.

## Version Compatibility

| Crate Version | A2A Protocol Version | Notes |
|---------------|---------------------|-------|
| 0.1.0 | [0.3.0](https://github.com/a2aproject/A2A/releases/tag/v0.3.0) | Initial implementation with full protocol support |

See the [A2A Protocol Releases](https://github.com/a2aproject/A2A/releases/) for the specification.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
a2a-client = "0.1.0"
a2a-types = "0.1.1"
```

## Quick Start

### Basic Usage

```rust
use a2a_client::A2AClient;
use a2a_types::{Message, MessageRole, MessageSendParams, Part};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client from agent card URL
    let client = A2AClient::from_card_url("https://agent.example.com")
        .await?
        .with_auth_token("your_api_key");

    // Create a message
    let message = Message {
        kind: "message".to_string(),
        message_id: "msg_123".to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Hello, agent!".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };

    // Send message
    let result = client
        .send_message(MessageSendParams {
            message,
            configuration: None,
            metadata: None,
        })
        .await?;

    println!("Response: {:?}", result);
    Ok(())
}
```

### Streaming Messages

```rust
use futures_util::StreamExt;
use a2a_client::A2AClient;
use a2a_types::{Message, MessageSendParams, SendStreamingMessageResult};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = A2AClient::from_card_url("https://agent.example.com").await?;

    let mut stream = client
        .send_streaming_message(MessageSendParams { /* ... */ })
        .await?;

    while let Some(result) = stream.next().await {
        match result? {
            SendStreamingMessageResult::Task(task) => {
                println!("Task: {:?}", task);
            }
            SendStreamingMessageResult::Message(msg) => {
                println!("Message: {:?}", msg);
            }
            SendStreamingMessageResult::TaskStatusUpdate(update) => {
                println!("Status: {:?}", update);
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact) => {
                println!("Artifact: {:?}", artifact);
            }
        }
    }

    Ok(())
}
```

### Custom HTTP Client (Advanced)

For advanced use cases like custom timeouts, proxies, retry logic, or auth flows, provide your own `reqwest::Client`:

```rust
use a2a_client::A2AClient;
use reqwest::{Client, header};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure custom HTTP client
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str("Bearer your_token")?,
    );
    headers.insert(
        "X-Custom-Header",
        header::HeaderValue::from_static("custom-value"),
    );

    let http_client = Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .default_headers(headers)
        .user_agent("my-app/1.0")
        .build()?;

    // Create A2A client with custom HTTP client
    let client = A2AClient::from_card_url_with_client(
        "https://agent.example.com",
        http_client
    ).await?;

    // Use client as normal...
    Ok(())
}
```

## API Reference

### Client Construction

#### `A2AClient::from_card_url(base_url)`
Create a client by fetching the agent card from `{base_url}/.well-known/agent-card.json`:

```rust
let client = A2AClient::from_card_url("https://agent.example.com").await?;
```

#### `A2AClient::from_card_url_with_client(base_url, http_client)`
Create a client with a custom `reqwest::Client`:

```rust
let http_client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .build()?;
let client = A2AClient::from_card_url_with_client(
    "https://agent.example.com",
    http_client
).await?;
```

#### `A2AClient::from_card(agent_card)`
Create a client directly from a pre-fetched agent card:

```rust
let agent_card = /* ... fetch or construct AgentCard ... */;
let client = A2AClient::from_card(agent_card)?;
```

#### `A2AClient::from_card_with_client(agent_card, http_client)`
Create a client from an agent card with a custom HTTP client:

```rust
let http_client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .build()?;
let client = A2AClient::from_card_with_client(agent_card, http_client)?;
```

#### `with_auth_token(token)`
Add bearer token authentication (builder pattern):

```rust
let client = A2AClient::from_card_url("https://agent.example.com")
    .await?
    .with_auth_token("your_api_key");
```

### Core Methods

#### Message Sending

```rust
// Non-streaming
let response = client.send_message(params).await?;

// Streaming (returns a Stream)
let stream = client.send_streaming_message(params).await?;
```

#### Task Management

```rust
// Get a specific task
let task = client.get_task(TaskQueryParams {
    id: "task_123".to_string(),
    history_length: Some(10),
    metadata: None,
}).await?;

// Cancel a task
let cancelled_task = client.cancel_task(TaskIdParams {
    id: "task_123".to_string(),
    metadata: None,
}).await?;

// Resubscribe to a task's event stream (for reconnection)
let stream = client.resubscribe_task(TaskIdParams {
    id: "task_123".to_string(),
    metadata: None,
}).await?;

// List tasks (commonly implemented, not official A2A spec)
let tasks = client.list_tasks(Some("context_id".to_string())).await?;
```

#### Push Notification Configuration

```rust
use a2a_types::{TaskPushNotificationConfig, PushNotificationConfig};

// Set push notification config
let config = client.set_task_push_notification_config(TaskPushNotificationConfig {
    task_id: "task_123".to_string(),
    push_notification_config: PushNotificationConfig {
        url: "https://my-app.com/webhook".to_string(),
        token: Some("webhook_token".to_string()),
        id: None,
        authentication: None,
    },
}).await?;

// Get push notification config
let config = client.get_task_push_notification_config(TaskIdParams {
    id: "task_123".to_string(),
    metadata: None,
}).await?;

// List all push notification configs for a task
let configs = client.list_task_push_notification_config(
    ListTaskPushNotificationConfigParams {
        id: "task_123".to_string(),
        metadata: None,
    }
).await?;

// Delete a push notification config
client.delete_task_push_notification_config(
    DeleteTaskPushNotificationConfigParams {
        id: "task_123".to_string(),
        push_notification_config_id: "config_id".to_string(),
        metadata: None,
    }
).await?;
```

#### Extension Methods

Call custom agent extension methods:

```rust
#[derive(serde::Serialize)]
struct CustomParams {
    query: String,
    limit: usize,
}

#[derive(serde::Deserialize)]
struct CustomResponse {
    results: Vec<String>,
}

let response: CustomResponse = client
    .call_extension_method("custom/search", CustomParams {
        query: "hello".to_string(),
        limit: 10,
    })
    .await?;
```

#### Agent Card Access

```rust
// Get cached agent card
let card = client.agent_card();

// Fetch a fresh agent card
let card = client.fetch_agent_card("https://agent.example.com").await?;
```

## Error Handling

The client uses `A2AError` for error reporting:

```rust
use a2a_client::{A2AError, A2AResult};

match client.send_message(params).await {
    Ok(response) => { /* handle success */ }
    Err(A2AError::NetworkError { message }) => {
        eprintln!("Network error: {}", message);
    }
    Err(A2AError::RemoteAgentError { message, code }) => {
        eprintln!("Agent error {}: {}", code.unwrap_or(0), message);
    }
    Err(A2AError::SerializationError { message }) => {
        eprintln!("Serialization error: {}", message);
    }
    Err(e) => {
        eprintln!("Other error: {:?}", e);
    }
}
```

## Compatibility

- **A2A Protocol Version**: 0.3.0
- **Rust Edition**: 2024
- **MSRV**: 1.70+

## Testing

```bash
# Run unit tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_client_requires_valid_card_url
```

## Contributing

Contributions are welcome! Please ensure:

1. Code follows Rust best practices
2. All tests pass
3. New features include tests
4. Documentation is updated

## License

Apache-2.0

## Related Crates

- [`a2a-types`](../a2a-types/) - A2A Protocol type definitions
- [`radkit`](../) - Full AI Agent Development Kit

## Resources

- [A2A Protocol Specification](https://a2a.dev)
- [Radkit Documentation](https://radkit.dev)
