# Axum Server for A2A Protocol

The `radkit-axum` crate provides a complete HTTP server implementation that exposes Radkit agents as A2A (Agent-to-Agent) protocol compliant endpoints. This enables your agents to communicate with other A2A-compatible agents and systems over HTTP.

## Overview

The A2A protocol is an open standard for AI agent interoperability. By running your Radkit agent with the Axum server, you get:

- **Full A2A compliance** - All protocol methods implemented
- **JSON-RPC 2.0 transport** - Standard request/response format
- **Server-Sent Events (SSE)** - Real-time streaming updates
- **Agent Card discovery** - Automatic capability advertisement
- **Flexible authentication** - Pluggable auth system
- **Production ready** - Built on Axum, Tower, and Hyper

### Agent Architecture Design

Radkit agents are designed as **multi-tenant** systems where every method requires:
- `app_name` - Application/tenant identifier
- `user_id` - User identifier within that application

This enables a single agent instance to serve multiple applications and users while maintaining proper isolation of sessions, tasks, and state. The Axum server's authentication system maps HTTP requests to these required parameters.

## Quick Start

### Installation

Add these dependencies to your `Cargo.toml`:

```toml
[dependencies]
radkit = "0.1"
radkit-axum = "0.1"
axum = "0.7"
tokio = { version = "1", features = ["full"] }
```

### Basic Server

```rust
use radkit::agents::Agent;
use radkit::models::MockLlm;
use radkit_axum::A2AServer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create your agent with AgentCard configuration
    let llm = Arc::new(MockLlm::new("gpt-4".to_string()));
    let agent = Agent::builder(
        "You are a helpful AI assistant.",
        MockLlm::new("gpt-4".to_string()),
    )
    .with_card(|c| c
        .with_name("My AI Agent")
        .with_description("A helpful assistant")
        .with_version("1.0.0")
        .with_url("http://localhost:3000")
        .with_streaming(true)
    )
    .with_builtin_task_tools()
    .build();
    
    // Create and run A2A server
    let server = A2AServer::builder(agent)
        .build();
    
    println!("Server starting on http://localhost:3000");
    server.serve("0.0.0.0:3000").await?;
    Ok(())
}
```

### Testing Your Server

Once running, test with curl:

```bash
# Get agent capabilities
curl http://localhost:3000/.well-known/agent-card.json

# Send a message (using default auth headers)
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

## Authentication with AuthExtractor

**Important**: All Radkit agent methods require `app_name` and `user_id` parameters. The `AuthExtractor` trait allows you to implement custom authentication strategies to extract these values from HTTP requests.

### Default Authentication Behavior

If no `AuthExtractor` is provided, the server uses a default implementation that:
- Extracts `app_name` from the `X-App-Name` header (defaults to `"default"`)
- Extracts `user_id` from the `X-User-Id` header (defaults to `"anonymous"`)

```bash
# Using default authentication
curl -X POST http://localhost:3000/message/send \
  -H 'X-App-Name: myapp' \
  -H 'X-User-Id: user123' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"message/send","params":{...},"id":1}'

# Without headers (uses defaults)
curl -X POST http://localhost:3000/message/send \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"message/send","params":{...},"id":1}'
# app_name="default", user_id="anonymous"
```

### Custom Authentication

The `AuthExtractor` trait allows you to implement custom authentication strategies. The server extracts `app_name` and `user_id` from HTTP requests, which are then passed to your Radkit agent methods.

### AuthExtractor Trait

The core responsibility of any `AuthExtractor` is to map HTTP requests to the `app_name` and `user_id` that Radkit agent methods require:

```rust
#[async_trait]
pub trait AuthExtractor: Send + Sync + 'static {
    /// Extract authentication context from request parts
    /// Must provide app_name and user_id for multi-tenant agent operation
    async fn extract(&self, parts: &mut Parts) -> Result<AuthContext, AuthError>;
}

pub struct AuthContext {
    pub app_name: String,  // Tenant/application identifier (REQUIRED)
    pub user_id: String,   // User identifier within app (REQUIRED)
    pub metadata: Option<serde_json::Value>, // Optional additional data
}
```

**Key Point**: Every HTTP request must be mapped to an `app_name` and `user_id`. This is the bridge between HTTP authentication (tokens, keys, etc.) and Radkit's multi-tenant agent architecture.

### Example: API Key Authentication

```rust
use radkit_axum::{AuthContext, AuthExtractor, async_trait};

struct ApiKeyAuth {
    valid_keys: Vec<String>,
}

#[async_trait]
impl AuthExtractor for ApiKeyAuth {
    async fn extract(&self, parts: &mut axum::http::request::Parts) 
        -> Result<AuthContext, radkit_axum::auth::AuthError> {
        
        // Extract API key from Authorization header
        let api_key = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?;
        
        // Validate API key
        if !self.valid_keys.contains(&api_key.to_string()) {
            return Err(radkit_axum::auth::AuthError::InvalidToken);
        }
        
        // Map API key to app_name and user_id
        let (app_name, user_id) = match api_key {
            "test-key-1" => ("app1", "user1"),
            "test-key-2" => ("app2", "user2"),
            _ => return Err(radkit_axum::auth::AuthError::InvalidToken),
        };
        
        Ok(AuthContext {
            app_name: app_name.to_string(),
            user_id: user_id.to_string(),
            metadata: None,
        })
    }
}

// Use it in your server
let server = A2AServer::builder(agent)
    .with_auth(ApiKeyAuth {
        valid_keys: vec!["test-key-1".to_string(), "test-key-2".to_string()],
    })
    .build();
```

### Example: JWT Authentication

```rust
struct JwtAuth {
    secret: String,
}

#[async_trait]
impl AuthExtractor for JwtAuth {
    async fn extract(&self, parts: &mut axum::http::request::Parts) 
        -> Result<AuthContext, radkit_axum::auth::AuthError> {
        
        // Extract JWT from Authorization header
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?;
        
        // Decode and verify JWT (use a proper JWT library like `jsonwebtoken`)
        let claims = decode_jwt(token, &self.secret)?;
        
        Ok(AuthContext {
            app_name: claims.tenant,
            user_id: claims.sub,
            metadata: Some(serde_json::json!({
                "exp": claims.exp,
                "iat": claims.iat,
            })),
        })
    }
}
```

### Common Authentication Patterns

1. **API Keys**: Simple bearer tokens mapped to app/user
2. **JWT**: Signed tokens with claims for tenant and user
3. **OAuth**: OAuth2/OIDC tokens validated against provider
4. **Database lookup**: API key -> database query for user info
5. **Service-to-service**: mTLS certificates or shared secrets

## A2A Protocol Endpoints

Your server automatically exposes these A2A-compliant endpoints:

### Core Protocol Methods

- **`POST /message/send`** - Send messages to the agent (JSON-RPC)
- **`POST /message/stream`** - Send messages with streaming responses (SSE)
- **`POST /tasks/get`** - Get task status and results
- **`POST /tasks/cancel`** - Cancel running tasks (not yet implemented)
- **`POST /tasks/resubscribe`** - Resubscribe to task updates (not yet implemented)

### Discovery & Health

- **`GET /.well-known/agent-card.json`** - Agent capability discovery (public)
- **`POST /agent/getAuthenticatedExtendedCard`** - Extended card for authenticated users
- **`GET /health`** - Health check endpoint

### Request Format (JSON-RPC 2.0)

All protocol methods use JSON-RPC 2.0 format:

```json
{
  "jsonrpc": "2.0",
  "method": "message/send",
  "params": {
    "message": {
      "role": "user",
      "parts": [{"kind": "text", "text": "Hello!"}]
    }
  },
  "id": 1
}
```

### Streaming (Server-Sent Events)

The `/message/stream` endpoint returns SSE events:

```bash
curl -X POST http://localhost:3000/message/stream \
  -H 'Authorization: Bearer your-token' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"message/stream","params":{...},"id":1}' \
  --no-buffer
```

Events come as:
```
data: {"jsonrpc":"2.0","result":{"kind":"task-status-update",...},"id":1}

data: {"jsonrpc":"2.0","result":{"kind":"task-completed",...},"id":1}
```

## Agent Card Configuration

Agent cards describe your agent's capabilities and are served at `/.well-known/agent-card.json`. The AgentCard is now integrated directly into the Agent and can be configured using the fluent builder API:

### Basic Agent Card Configuration

```rust
let agent = Agent::builder(
        "You are a helpful AI assistant.",
        llm,
    )
    .with_card(|c| c
        .with_name("My AI Agent")
        .with_description("A powerful AI assistant")
        .with_version("1.0.0")
        .with_url("https://my-agent.example.com")
        .with_streaming(true)
        .with_push_notifications(false)
        .with_state_transition_history(false)
        .add_skill_with("general-help", "General Assistance", |skill| {
            skill.with_description("Helps with questions and tasks")
                .add_tag("general")
                .add_tag("help")
                .add_example("How do I...?")
        })
        .with_security_schemes(std::collections::HashMap::from([
            ("bearerAuth".to_string(), a2a_types::SecurityScheme::Http(
                a2a_types::HTTPAuthSecurityScheme {
                    scheme_type: "http".to_string(),
                    scheme: "bearer".to_string(),
                    bearer_format: Some("JWT".to_string()),
                    description: Some("JWT authentication".to_string()),
                }
            )),
        ]))
});

let server = A2AServer::builder(agent)
    .build();
```

### Server-Specific Card Configuration

You can also override AgentCard settings specifically for the server (useful for setting production URLs):

```rust
let server = A2AServer::builder(agent)
    .with_agent_card_config(|card| {
        card.with_url("https://production-api.example.com")
            .with_documentation_url("https://docs.example.com")
    })
    .build();
```

### Default Behavior

If no AgentCard configuration is provided:
- Agent name and description are used from the Agent
- Version defaults to empty string (server sets "0.1.0" if empty)
- URL defaults to empty string (server sets "http://localhost:3000" if empty)
- Streaming is automatically enabled for servers
- Basic capabilities are set with sensible defaults

## Advanced Configuration

### Custom Middleware

You can add custom Axum middleware:

```rust
let server = A2AServer::builder(agent)
    .with_auth(MyAuth)
    .with_agent_card_config(|card| {
        card.with_url("https://api.example.com")
            .with_version("2.0.0")
    })
    .build();

let app = server.into_router()
    .layer(tower_http::trace::TraceLayer::new_for_http())
    .layer(tower_http::cors::CorsLayer::permissive())
    .layer(tower_http::timeout::TimeoutLayer::new(std::time::Duration::from_secs(30)));

axum::serve(listener, app).await?;
```

### Custom Router Integration

Integrate with existing Axum applications:

```rust
let a2a_server = A2AServer::builder(agent).build();
let a2a_routes = a2a_server.into_router();

let app = Router::new()
    .route("/", get(|| async { "Hello World!" }))
    .nest("/a2a", a2a_routes)  // Mount A2A endpoints under /a2a
    .route("/metrics", get(metrics_handler));
```

## Production Deployment

### Environment Variables

```bash
# Server configuration
BIND_ADDRESS=0.0.0.0:3000
LOG_LEVEL=info

# Authentication
JWT_SECRET=your-secret-key
API_KEYS=key1,key2,key3

# Agent configuration
AGENT_NAME="Production Agent"
AGENT_VERSION=1.0.0
OPENAI_API_KEY=your-openai-key
```

### Docker Example

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/my-agent /usr/local/bin/
EXPOSE 3000
CMD ["my-agent"]
```

### Health Checks

```bash
# Docker health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Kubernetes liveness probe
livenessProbe:
  httpGet:
    path: /health
    port: 3000
  initialDelaySeconds: 30
  periodSeconds: 10
```

### Load Balancing

The server is stateless and can be load balanced. Session state is managed within individual agent instances, so sticky sessions are not required for basic operations.

## Error Handling

The server returns proper HTTP status codes and JSON-RPC error responses:

```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32602,
    "message": "Invalid params: Missing required field"
  },
  "id": 1
}
```

Common error codes:
- `-32700`: Parse error
- `-32600`: Invalid request  
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32603`: Internal error
- `401`: Authentication required
- `403`: Insufficient permissions

## Complete Examples

See the `/examples` directory in the `radkit-axum` crate:

- **`basic_server.rs`** - Simple API key authentication
- **`jwt_server.rs`** - JWT-based authentication with full agent card

Run examples:

```bash
cargo run --example basic_server
cargo run --example jwt_server
```

## A2A Protocol Resources

- **A2A Specification**: https://a2a.dev/
- **Agent Card Schema**: Discovery format for agent capabilities
- **JSON-RPC 2.0**: https://www.jsonrpc.org/specification
- **Server-Sent Events**: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events

The Radkit Axum server provides a complete, production-ready implementation of the A2A protocol, making your agents discoverable and interoperable with the broader AI agent ecosystem.