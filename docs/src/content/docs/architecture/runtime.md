---
title: Runtime
description: Learn about Radkit's Runtime trait and how it provides service abstraction for agents.
---



The core of Radkit's architecture is the `Runtime` trait. It acts as a centralized **service hub** or dependency injection container for your agent. Instead of passing many different service handles (like a database connector, a logger, etc.) to every function, your `SkillHandler`s receive a single, unified reference: `&dyn Runtime`.

This design keeps your agent logic clean, decoupled from infrastructure, and highly portable.

## The `Runtime` Trait

All runtime environments, whether for local development or cloud deployment, must implement the `Runtime` trait. This guarantees a consistent interface for your skills.

```rust
pub trait Runtime {
    fn task_manager(&self) -> Arc<dyn TaskManager>;
    fn memory_service(&self) -> Arc<dyn MemoryService>;
    fn logging_service(&self) -> Arc<dyn LoggingService>;
    // ... other services
}
```

Your skill code is written against this abstraction, not a concrete implementation. A skill simply asks the runtime for a service:

```rust
// Inside a SkillHandler method...
let memory_service = runtime.memory_service();
memory_service.save(auth_ctx, "some_key", &data).await?;
```

The skill doesn't know or care if `memory_service` is writing to an in-memory map for local testing or a massive, persistent cloud database.

## Core Services

The `Runtime` provides access to a set of essential services, each defined by its own trait.

-   **`TaskManager`**: A Data Access Object (DAO) for persisting and retrieving the state of A2A `Task`s and their event histories. This is crucial for multi-turn conversations and auditing.
-   **`MemoryService`**: Provides a persistent, tenant-aware key-value store. This is ideal for giving your agent long-term memory or for implementing Retrieval-Augmented Generation (RAG).
-   **`LoggingService`**: A structured logging interface that streams logs to the console during local development or to a cloud observability UI in production.
-   **`AuthService`**: Identifies the current user and tenant, ensuring that all other services are properly namespaced and data is kept secure.

## Provided Runtimes

Radkit provides a `DefaultRuntime` for local development, and the architecture allows for custom runtimes for production or specialized environments.

### `DefaultRuntime`

Included with the `runtime` feature flag, `DefaultRuntime` is an out-of-the-box implementation that works on both native and WASM targets. It provides simple, in-memory versions of all the core services, allowing you to get up and running instantly with no configuration.

### Cloud Runtime

When you deploy your agent to a managed cloud platform, it is automatically run against a production-grade, persistent, and scalable implementation of the `Runtime`. This gives you enterprise-grade features like a hosted vector database and a rich observability UI without changing your agent code.

### Custom Runtimes

You can implement the `Runtime` trait yourself to integrate your agent with your own infrastructure. For example, you could create a `CustomRuntime` that uses your company's existing logging system or connects to your own PostgreSQL database. This is the key to deploying agents in specialized environments, as explained in the **[Serverless](./serverless.md)** guide.