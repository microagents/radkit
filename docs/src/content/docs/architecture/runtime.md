---
title: Runtime
description: Learn about Radkit's AgentRuntime trait and how it provides service abstraction for agents.
---



The core of Radkit's architecture is the `AgentRuntime` trait. It acts as a centralized **service hub** or dependency injection container for your agent. Instead of passing many different service handles (like a database connector, a logger, etc.) to every function, your `SkillHandler`s receive a single, unified reference: `&dyn AgentRuntime`.

This design keeps your agent logic clean, decoupled from infrastructure, and highly portable.

## The `AgentRuntime` Trait

All runtime environments, whether for local development or cloud deployment, must implement the `AgentRuntime` trait. This guarantees a consistent interface for your skills.

```rust
pub trait AgentRuntime {
    fn auth(&self) -> Arc<dyn AuthService>;
    fn memory(&self) -> Arc<dyn MemoryService>;
    fn logging(&self) -> Arc<dyn LoggingService>;
    fn default_llm(&self) -> Arc<dyn BaseLlm>;
}
```

Your skill code is written against this abstraction, not a concrete implementation. A skill simply asks the runtime for a service:

```rust
let memory = runtime.memory();
memory.save(auth_ctx, "some_key", &data).await?;
```

The skill doesn't know or care if `memory_service` is writing to an in-memory map for local testing or a massive, persistent cloud database.

## Core Services

`AgentRuntime` provides access to a set of essential services, each defined by its own trait.

-   **`TaskManager`**: A Data Access Object (DAO) for persisting and retrieving the state of A2A `Task`s and their event histories. This is crucial for multi-turn conversations and auditing. The core runtime uses it internally so skill authors don't have to.
-   **`MemoryService`**: Provides a persistent, tenant-aware key-value store. This is ideal for giving your agent long-term memory or for implementing Retrieval-Augmented Generation (RAG).
-   **`LoggingService`**: A structured logging interface that streams logs to the console during local development or to a cloud observability UI in production.
-   **`AuthService`**: Identifies the current user and tenant, ensuring that all other services are properly namespaced and data is kept secure.

## Provided Runtimes

Radkit provides a `RuntimeBuilder` plus a concrete `Runtime` implementation for local development, and the architecture allows for custom runtimes for production or specialized environments.

### `Runtime` (native implementation)

Included with the `runtime` feature flag, the builder returns an out-of-the-box runtime that works on both native and WASM targets. It provides simple, in-memory versions of all the core services, allowing you to get up and running instantly with no configuration.

```rust
use radkit::models::providers::OpenRouterLlm;
use radkit::runtime::Runtime;

# fn configure_agent() -> radkit::agent::AgentDefinition {
#     radkit::agent::Agent::builder()
#         .with_id("hr-agent")
#         .with_name("HR Agent")
#         .build()
# }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let llm = OpenRouterLlm::from_env("anthropic/claude-3.5-sonnet")?;
    Runtime::builder(configure_agent(), llm)
        .build()
        .serve("127.0.0.1:8080")
        .await?;
    Ok(())
}
```

The runtime hosts **exactly one agent definition** per process. When `.serve(..)` is called it automatically exposes the A2A HTTP surface at the root of the bound address:

- `/.well-known/agent-card.json` – serves the agent card
- `/rpc` – JSON-RPC entry point for `message/send`, `message/stream`, `tasks/resubscribe`, etc.
- `/message:stream` – streaming API for Server-Sent Events
- `/tasks/{task_id}/subscribe` – SSE resubscribe endpoint that mirrors the A2A spec

If the optional `dev-ui` feature is enabled, the same server also mounts `/ui/*` routes and serves the React console for the single configured agent.
