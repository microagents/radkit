---
title: Serverless
description: Deploy Radkit agents to serverless platforms like Cloudflare Workers and Vercel Edge Functions.
---



The `Runtime` abstraction not only allows you to switch between a local in-memory environment and a managed cloud environment, but it also enables you to deploy Radkit agents to **serverless platforms** that support WebAssembly, such as Cloudflare Workers, Vercel Edge Functions, or other FaaS providers.

## The Goal: Stateless Execution, Persistent State

Serverless platforms are typically stateless. An instance of your code is spun up to handle a single incoming request and then torn down. For an agent to function in this model, its state (like conversation history or long-term memory) must be stored in an external, persistent storage layer (e.g., a database, Redis, or a cloud storage service).

Radkit's architecture is explicitly designed for this separation of concerns. Your skill logic is decoupled from the implementation of the runtime services.

## The "Custom Runtime" Pattern for Serverless

To target a serverless WASM platform, you would use the "Custom Runtime" pattern. Instead of using the feature-gated `DefaultRuntime`, you would create your own runtime tailored to the specific serverless environment.

Here’s a conceptual walkthrough:

### 1. Implement Services with External Storage

First, you implement the core service traits (`MemoryService`, `TaskManager`, etc.) with logic that connects to your chosen external persistence layer. Since WASM modules running under WASI can make outbound HTTP requests, they can interact with any database or service that has an HTTP API (like PlanetScale, Upstash, or AWS DynamoDB).

```rust
// A custom implementation of MemoryService that writes to a remote database via HTTP.
pub struct PersistentMemoryService {
    http_client: reqwest::Client,
    database_url: String,
}

#[async_trait::async_trait]
impl MemoryService for PersistentMemoryService {
    async fn save_serialized(
        &self,
        auth_ctx: &AuthContext,
        key: &str,
        value: serde_json::Value,
    ) -> AgentResult<()> {
        let namespaced_key = format!("{}:{}:{}", auth_ctx.app_name, auth_ctx.user_name, key);

        // Logic to POST the value to the external database API.
        self.http_client
            .post(format!("{}/set", self.database_url))
            .json(&serde_json::json!({ "key": namespaced_key, "value": value }))
            .send()
            .await?;

        Ok(())
    }
    // ... implement load_serialized and other methods
}
```

### 2. Create the `ServerlessRuntime`

Next, you bundle your custom services into a new runtime struct that implements the `Runtime` trait.

```rust
pub struct ServerlessRuntime {
    auth_service: Arc<dyn AuthService>,
    memory_service: Arc<dyn MemoryService>,
    // ... other services
}

impl ServerlessRuntime {
    pub fn new() -> Self {
        // This function would initialize the HTTP clients and read
        // connection strings/secrets provided by the serverless environment.
        Self {
            auth_service: Arc::new(DefaultAuthService::new()),
            memory_service: Arc::new(PersistentMemoryService { /* ... */ }),
            // ...
        }
    }
}

impl Runtime for ServerlessRuntime {
    // ... implement the trait methods
}
```

### 3. Write the Serverless Entrypoint

Finally, you write the main entrypoint function that your serverless platform will call for each incoming request. This function replaces the `main` function and the `axum` server from the `DefaultRuntime`. It is responsible for parsing the incoming HTTP request, instantiating your `ServerlessRuntime`, executing the appropriate skill, and returning an HTTP response.

```rust
// The specific function signature depends on the serverless platform.
#[no_mangle]
pub async fn handle_request(request: IncomingRequest) -> OutgoingResponse {
    // 1. Instantiate the custom runtime and agent definitions.
    let runtime = Arc::new(ServerlessRuntime::new());
    let agents = Arc::new(configure_agents()); // Your existing agent definitions

    // 2. Parse the A2A request from the HTTP body.
    let a2a_request: a2a_types::A2ARequest = serde_json::from_slice(&request.body).unwrap();

    // 3. Find the correct agent and skill to execute.
    // (Simplified logic here)
    let skill_handler = find_skill_handler(&agents, &a2a_request);
    
    // 4. Execute the skill handler.
    let result = skill_handler.on_request(/* ... */).await;

    // 5. Serialize the result and return it as an HTTP response.
    let response_body = serde_json::to_vec(&result.unwrap()).unwrap();
    OutgoingResponse::new(200, response_body)
}
```

This pattern provides maximum flexibility, allowing the exact same agent and skill code to be tested locally with `DefaultRuntime` and then deployed to a high-scale, persistent serverless environment with a `CustomRuntime`.