# 06: Serverless WASM Deployment

This document specifies how the `radkit` architecture supports deploying agents to serverless WebAssembly (WASM) environments, such as Cloudflare Workers, Vercel Edge Functions, or other Function-as-a-Service (FaaS) platforms that run WASM.

## 1. The Goal: Stateless Execution, Persistent State

The serverless model assumes a stateless execution environment. The platform receives an HTTP request, instantiates the WASM module to handle it, and then tears it down. For an agent to function in this model, its state (memory, session history) must live outside the WASM module in a persistent storage layer (e.g., a database, Redis, or a cloud storage service).

The `radkit` architecture is explicitly designed to support this separation of concerns.

## 2. Architectural Enabler: The `Runtime` Trait

The key to this capability is the `Runtime` trait. As detailed in `05_runtime.md`, all agent logic (within `SkillHandler` implementations) is written against the `Runtime` trait, not a concrete implementation. A skill simply asks the runtime for a service, like so:

```rust
let memory_service = runtime.memory_service();
memory_service.save(auth_ctx, "some_key", &data).await?;
```

The skill code has no knowledge of whether `memory_service` is writing to an in-memory `DashMap` or a persistent, external database like PlanetScale. This abstraction makes the agent logic completely portable.

## 3. The "Custom Runtime" Pattern for Serverless

A developer targeting a serverless WASM platform would follow the "Custom Runtime" pattern. They would not use the feature-gated `DefaultRuntime`, but would instead create their own runtime tailored to the serverless environment.

Here is a conceptual walkthrough:

### Step 1: Define Persistent Services

First, the developer implements the core service traits (`MemoryService`, `TaskManager`, etc.) with logic that connects to their chosen external persistence layer. Since the WASM module can make outbound HTTP requests, it can interact with any database that has an HTTP API.

```rust
// In the developer's own agent crate
use radkit::runtime::{services::*, context::AuthContext};
use std::sync::Arc;

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
            .await
            .map_err(|e| AgentError::Internal {
                component: "memory_service".to_string(),
                reason: e.to_string()
            })?;

        Ok(())
    }

    async fn load_serialized(&self, auth_ctx: &AuthContext, key: &str) -> AgentResult<Option<serde_json::Value>> {
        // Logic to GET the value from the external database API.
        // ...
    }
}
```

### Step 2: Create the Custom `ServerlessRuntime`

Next, the developer bundles their custom services into a new runtime struct.

```rust
use radkit::runtime::Runtime;

pub struct ServerlessRuntime {
    auth_service: Arc<dyn AuthService>,
    memory_service: Arc<dyn MemoryService>,
    // ... other services
}

impl ServerlessRuntime {
    pub fn new() -> Self {
        // This function would initialize the HTTP clients and connection
        // strings needed by the persistent services.
        Self {
            auth_service: Arc::new(services::DefaultAuthService::new()), // Or a custom auth service
            memory_service: Arc::new(PersistentMemoryService { /* ... */ }),
            // ...
        }
    }
}

// Finally, implement the main Runtime trait
impl Runtime for ServerlessRuntime {
    fn auth_service(&self) -> Arc<dyn AuthService> { self.auth_service.clone() }
    fn memory_service(&self) -> Arc<dyn MemoryService> { self.memory_service.clone() }
    // ...
}
```

### Step 3: Write the Serverless Entrypoint

Serverless WASM platforms do not call a `main` function. Instead, they invoke a specific exported function to handle each request. The developer writes this function, which acts as the bridge between the platform and the `radkit` agent.

This entrypoint function effectively becomes a lightweight version of the `axum` server logic from `DefaultRuntime`.

```rust
use radkit::agent::AgentDefinition;
use radkit::models::Content;

// Assume the platform provides `IncomingRequest` and `OutgoingResponse` types.

#[no_mangle] // Export the function from the WASM module
pub async fn handle_request(request: IncomingRequest) -> OutgoingResponse {
    // 1. Instantiate the custom runtime and agent definitions.
    let runtime = Arc::new(ServerlessRuntime::new());
    let agents = Arc::new(configure_agents()); // User-defined function to get all AgentDefinitions

    // 2. Parse the incoming A2A request from the HTTP body.
    let a2a_request: a2a_types::A2ARequest = match serde_json::from_slice(&request.body) {
        Ok(req) => req,
        Err(e) => return OutgoingResponse::new(400, e.to_string()),
    };

    // 3. Replicate the logic from the `create_task_handler`:
    let a2a_message = match a2a_request.payload {
        a2a_types::A2ARequestPayload::SendMessage { params } => params.message,
        _ => return OutgoingResponse::new(400, "Unsupported method".to_string()),
    };

    // (Simplified logic: find agent/skill, create contexts, etc.)
    let skill_id = a2a_message.parts.first().unwrap().as_text().unwrap_or_default();
    let skill_handler = agents.first().unwrap().skills.iter().find(|s| s.id == skill_id).unwrap().handler.clone();
    
    let auth_context = runtime.auth().get_auth_context();
    let mut context = radkit::runtime::context::Context::new(auth_context);
    let mut task_context = radkit::runtime::context::TaskContext::new().unwrap();

    let content: Content = a2a_message.into();

    // 4. Execute the skill handler.
    let result = skill_handler.on_request(&mut task_context, &mut context, &*runtime, content).await;

    // 5. Serialize the result and return it as an HTTP response.
    match result {
        Ok(res) => {
            let response_task: a2a_types::Task = res.into();
            let body = serde_json::to_vec(&response_task).unwrap();
            OutgoingResponse::new(200, body)
        }
        Err(e) => OutgoingResponse::new(500, e.to_string()),
    }
}

// Helper function defined by the developer
fn configure_agents() -> Vec<AgentDefinition> {
    // ... returns the agent definitions for the project
}
```

## Conclusion

The `radkit` architecture fully supports serverless WASM deployments. The core design principle of separating agent logic from the runtime environment allows developers to create custom runtimes that integrate with any backend infrastructure. This provides maximum flexibility, allowing the same agent and skill code to be tested locally with the `DefaultRuntime` and then deployed to a high-scale, persistent serverless environment with a `CustomRuntime`.