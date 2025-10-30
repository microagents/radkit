# 05: Runtime Architecture

This document provides a detailed specification of the `Runtime` and its associated services. It builds upon the high-level concepts introduced in `01_library_architecture.md` with the concrete implementation details and design rationale that we have established.

## Error Handling

All runtime services use the unified `AgentError` type defined in `radkit/src/errors.rs`. Throughout this specification:
- `AgentResult<T>` is a type alias for `Result<T, AgentError>`
- Services return specific `AgentError` variants to indicate different error conditions
- Common variants include `AgentError::Internal` for service failures, `AgentError::TaskNotFound`, `AgentError::SessionNotFound`, etc.

This unified error type ensures consistent error handling across the entire radkit ecosystem and provides proper HTTP status code mapping when used with the runtime feature.

## 1. The `Runtime` as a Service Hub

The `Runtime` is the heart of the `radkit` execution environment. It acts as a centralized **service hub** or dependency injection container. Instead of passing numerous service handles to every function, skill handlers receive a single, unified `&dyn Runtime` reference. This design keeps the agent logic clean and decoupled from the underlying infrastructure.

The primary responsibilities of the `Runtime` are:
- To provide access to essential platform services like authentication, memory, and session management.
- To abstract away the underlying implementation of these services, allowing agent code to be portable between local development and the cloud.

### 1.1. The `Runtime` Trait

All runtime implementations, whether for local development or cloud deployment, must implement the `Runtime` trait. This ensures a consistent interface for agent skills, making them portable across different environments.

```rust
pub trait Runtime: MaybeSend + MaybeSync {
    /// Returns the authentication service.
    fn auth_service(&self) -> Arc<dyn AuthService>;

    /// Returns the task manager for persisting and retrieving tasks and events.
    fn task_manager(&self) -> Arc<dyn TaskManager>;

    /// Returns the memory service for storing and retrieving data.
    fn memory_service(&self) -> Arc<dyn MemoryService>;

    /// Returns the logging service for structured logging.
    fn logging_service(&self) -> Arc<dyn LoggingService>;

    /// Returns the default LLM for this runtime.
    fn get_default_llm(&self) -> Arc<dyn BaseLlm>;
}
```

## 2. Core Services

The `Runtime` provides a set of core services, each represented by a trait. These services are designed with multi-tenancy and security as first-class citizens.

### 2.1. `AuthService`: Authentication and Tenancy

**Purpose**: The `AuthService` is responsible for identifying the current user and tenant for any given request. This is fundamental for security and multi-tenancy, ensuring that one user or application cannot access another's data.

**Definition**:
```rust
pub trait AuthService: MaybeSend + MaybeSync {
    /// Returns the current authentication context.
    fn get_auth_context(&self) -> AuthContext;
}

/// Holds authentication and tenancy information for the current execution.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub app_name: String,
    pub user_name: String,
}
```
The `AuthContext` struct is a simple data container that is passed to other services, which use it to namespace their operations, guaranteeing data isolation.

### 2.2. `MemoryService`: Persistent, Tenant-Aware Storage

**Purpose**: The `MemoryService` provides a key-value storage mechanism. This is crucial for skills that need to persist state, cache results, or build a knowledge base for Retrieval-Augmented Generation (RAG).

**Design Rationale**: To support multi-tenancy, all `MemoryService` operations are explicitly namespaced by the `AuthContext`. This is a critical design choice that guarantees data isolation at the service level. A skill developer does not need to worry about manually prefixing keys to avoid collisions; they simply provide the context, and the service handles the data separation.

**Definition**:
```rust
#[async_trait]
pub trait MemoryService: MaybeSend + MaybeSync {
    /// Stores a JSON value under the given key, namespaced by the auth context.
    async fn save_serialized(
        &self,
        auth_ctx: &AuthContext,
        key: &str,
        value: serde_json::Value,
    ) -> AgentResult<()>;

    /// Loads a JSON value for the given key, namespaced by the auth context.
    async fn load_serialized(
        &self,
        auth_ctx: &AuthContext,
        key: &str,
    ) -> AgentResult<Option<serde_json::Value>>;
}
```

### 2.3. `TaskManager`: Task and Event Persistence

**Purpose**: The `TaskManager` acts as a stateful Data Access Object (DAO) for persisting and retrieving all data related to agent interactions. It is responsible for storing `Task` state and the sequence of `TaskEvent`s that occur during a task's lifecycle. This persistence is essential for managing multi-turn conversations, resuming long-running operations, and for auditing and observability.

**Design Rationale**:
- **Auth-Scoped**: All methods are explicitly namespaced by the `AuthContext`. This is a critical security feature that guarantees data isolation between different users and applications (tenants) at the service layer.
- **Protocol-Aligned**: The service's design mirrors the A2A protocol. It treats a "session" as a `contextId` for filtering tasks, rather than a concrete container object. This aligns with the A2A `tasks/list` method and ensures scalability.
- **Separation of Concerns**: The state of a `Task` is stored separately from its `TaskEvent` log. This is an optimization to avoid loading a potentially long list of events every time the task's current status is needed.

#### Data Structures

```rust
/// Represents the state of a single task, mirroring the A2A Task object.
pub struct Task {
    pub id: String,
    pub session_id: String, // Corresponds to A2A contextId
    pub status: a2a_types::TaskStatus,
    pub artifacts: Vec<a2a_types::Artifact>,
}

/// Represents a significant event that occurred during a task's lifecycle.
/// This enum can be converted from and to the various A2A event types.
pub enum TaskEvent {
    StatusUpdate(a2a_types::TaskStatusUpdateEvent),
    ArtifactUpdate(a2a_types::TaskArtifactUpdateEvent),
    Message(a2a_types::Message),
}

/// Filter for listing tasks, enabling pagination.
pub struct ListTasksFilter<'a> {
    pub session_id: Option<&'a str>,
    pub page_size: Option<u32>,
    pub page_token: Option<&'a str>,
}

/// Represents a paginated result set.
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}
```

#### Service Definition

```rust
#[async_trait]
pub trait TaskManager: MaybeSend + MaybeSync {
    /// Retrieves a single task by its ID.
    async fn get_task(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
    ) -> AgentResult<Option<Task>>;

    /// Lists tasks for the given user/app, with optional filtering and pagination.
    /// This is the primary method for retrieving tasks associated with a "session" (contextId).
    async fn list_tasks(
        &self,
        auth_ctx: &AuthContext,
        filter: &ListTasksFilter,
    ) -> AgentResult<PaginatedResult<Task>>;

    /// Stores or updates a task's state. This is an upsert operation.
    async fn save_task(
        &self,
        auth_ctx: &AuthContext,
        task: &Task,
    ) -> AgentResult<()>;

    /// Appends an event to a task's history.
    async fn add_task_event(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
        event: &TaskEvent,
    ) -> AgentResult<()>;

    /// Retrieves all events for a specific task, ordered chronologically.
    async fn get_task_events(
        &self,
        auth_ctx: &AuthContext,
        task_id: &str,
    ) -> AgentResult<Vec<TaskEvent>>;
}
```

#### Task Reconstruction

It is important to note the separation between how a task is stored and how it is presented to an A2A client. The `Task` struct defined here represents the lean, persisted state of the task. The A2A protocol's `Task` object also includes a `history` field, which is an array of `Message`s.

The `radkit` framework is responsible for reconstructing the full A2A `Task` object on demand. When an A2A client requests a task, the framework will:
1.  Call `task_manager.get_task(...)` to retrieve the core task state.
2.  Call `task_manager.get_task_events(...)` to retrieve the full event log for that task.
3.  Filter the event log to extract only the `TaskEvent::Message` variants.
4.  Assemble the final A2A `Task` object, populating its `history` field with the extracted messages before sending it to the client.

This event sourcing approach is highly scalable, as it avoids storing and re-writing a potentially large and growing history array with every small status update. The event log serves as an efficient, append-only audit trail.

### 2.4. `LoggingService`: Observability

**Purpose**: Provides a structured way for skills and the runtime itself to emit logs. In the `DefaultRuntime`, these might just print to the console. In the cloud, this service streams logs to the observability UI.

**Definition**:
```rust
pub trait LoggingService: MaybeSend + MaybeSync {
    fn log(&self, level: LogLevel, message: &str);
}
```

## 3. Runtime Implementations

The `radkit` ecosystem provides different implementations of the `Runtime` trait tailored for different environments.

### 3.1. `DefaultRuntime`: The Unified Local Runtime

The `DefaultRuntime` is the out-of-the-box, unified runtime designed for local development and testing on **both native and WASM targets**. It requires no configuration and provides a complete, self-contained environment by using conditional compilation to adapt its services to the target architecture.

#### Feature Flag: `runtime`

To keep the core `radkit` library lean, the `DefaultRuntime` and all of its associated components (including the `axum` web server and in-memory services) are optional. They are only compiled if the `runtime` feature is enabled in your `Cargo.toml`:

```toml
[dependencies]
# For core features only
radkit = "0.1.0"

# To enable the full agent server environment
radkit = { version = "0.1.0", features = ["runtime"] }
```

This allows developers who only need core functionalities like `LlmFunction` or who are building a completely custom runtime to avoid compiling unnecessary dependencies.

#### Composition & Definition

The `DefaultRuntime` is composed of `Arc`s for each of its services, allowing it to be shared safely across concurrent tasks in the web server.

- **`auth_service`**: `Arc<StaticAuthService>` - A simple implementation that returns a static context (e.g., `app_name: "default-app"`, `user_name: "default-user"`).
- **`memory_service`**: `Arc<InMemoryMemoryService>` - A thread-safe, in-memory key-value store.
- **`task_manager`**: `Arc<InMemoryTaskManager>` - An in-memory task and event persistence layer.
- **`logging_service`**: `Arc<ConsoleLoggingService>` - A simple console logger that prints to stderr.
- **`base_llm`**: `Arc<dyn BaseLlm>` - The default LLM provider for this runtime, provided during construction.
- **`negotiator`**: `Arc<DefaultNegotiator>` - The negotiator for handling pre-task conversations, initialized with the base LLM.

```rust
#[cfg(feature = "runtime")]
#[derive(Clone)]
pub struct DefaultRuntime {
    auth_service: Arc<dyn AuthService>,
    task_manager: Arc<dyn TaskManager>,
    memory_service: Arc<dyn MemoryService>,
    logging_service: Arc<dyn LoggingService>,
    base_llm: Arc<dyn BaseLlm>,
    negotiator: Arc<dyn Negotiator>,
    agents: Arc<Vec<crate::agent::AgentDefinition>>,
}

#[cfg(feature = "runtime")]
impl DefaultRuntime {
    /// Creates a new default runtime instance.
    ///
    /// # Arguments
    ///
    /// * `llm` - The LLM provider to use as the default for this runtime
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::models::providers::AnthropicLlm;
    ///
    /// let llm = AnthropicLlm::from_env("claude-sonnet-4")?;
    /// let runtime = DefaultRuntime::new(llm)
    ///     .agents(vec![my_agent])
    ///     .serve("127.0.0.1:8080").await?;
    /// ```
    pub fn new(llm: impl BaseLlm + 'static) -> Self {
        let base_llm: Arc<dyn BaseLlm> = Arc::new(llm);

        Self {
            auth_service: Arc::new(StaticAuthService::default()),
            task_manager: Arc::new(InMemoryTaskManager::new()),
            memory_service: Arc::new(InMemoryMemoryService::new()),
            logging_service: Arc::new(ConsoleLoggingService),
            base_llm: base_llm.clone(),
            negotiator: Arc::new(DefaultNegotiator::new(base_llm)),
            agents: Arc::new(Vec::new()),
        }
    }
    // ...
}
```


**Conditional Compilation & Thread Safety**: A key feature of the `DefaultRuntime` is its ability to adapt. 
- **On native targets**, it supports concurrent execution. Its in-memory services are built using concurrent data structures. For example, `InMemoryMemoryService` uses a `dashmap::DashMap` internally, which provides high-performance, sharded locking for concurrent reads and writes.
- **On `wasm32-wasip1` targets**, where there is no multi-threading, it compiles to a single-threaded version of these services (e.g., using `std::cell::RefCell` instead of `DashMap`) to maintain API compatibility without the overhead of thread-safe primitives.

This unified approach simplifies the codebase and ensures a consistent developer experience regardless of the final deployment target.

### 3.2. Other Runtimes

- **Cloud Runtime**: The managed cloud platform provides its own production-grade, persistent, and scalable implementations of all runtime services. When an agent is deployed, it runs against this cloud runtime automatically, gaining access to hosted databases, logging infrastructure, and more.
- **`CustomRuntime` (User-Defined)**: Advanced users can implement the `Runtime` trait themselves to integrate with their own infrastructure (e.g., their own databases, logging systems, or vector stores).

## 4. Module Organization

The runtime module is organized into service-based submodules to support extensibility and clarity. This organization separates **user-facing services** from **core framework internals**.

### 4.1. Module Structure

```
radkit/src/runtime/
├── mod.rs                    # Runtime trait + DefaultRuntime + public re-exports
├── context.rs                # AuthContext, Context, TaskContext
├── web.rs                    # Web server handlers (native-only)
│
├── auth/                     # 🔵 USER-FACING: Authentication service
│   ├── mod.rs                # AuthService trait
│   └── static_auth.rs        # StaticAuthService implementation
│
├── memory/                   # 🔵 USER-FACING: Storage service
│   ├── mod.rs                # MemoryService + MemoryServiceExt traits
│   └── in_memory.rs          # InMemoryMemoryService (native + WASM)
│
├── logging/                  # 🔵 USER-FACING: Logging service
│   ├── mod.rs                # LoggingService trait + LogLevel
│   └── console.rs            # ConsoleLoggingService
│
├── task_manager/             # 🔵 USER-FACING: Task persistence
│   ├── mod.rs                # TaskManager trait + data types
│   └── in_memory.rs          # InMemoryTaskManager (native + WASM)
│
└── core/                     # 🟠 CORE: Advanced/framework internals
    ├── mod.rs                # Core module re-exports
    ├── execution.rs          # Task execution logic (native-only)
    ├── wasm.rs               # WASM utilities
    └── negotiator/           # Message negotiation
        ├── mod.rs            # Negotiator trait
        └── default.rs        # DefaultNegotiator
```

### 4.2. Import Patterns

#### For User-Facing Services

Services are organized by functionality with clear import paths:

```rust
// Import service traits
use radkit::runtime::{AuthService, MemoryService, LoggingService, TaskManager};

// Import specific implementations
use radkit::runtime::auth::StaticAuthService;
use radkit::runtime::memory::InMemoryMemoryService;
use radkit::runtime::logging::ConsoleLoggingService;
use radkit::runtime::task_manager::InMemoryTaskManager;

// Convenience re-exports (when runtime feature is enabled)
use radkit::runtime::{
    StaticAuthService,
    InMemoryMemoryService,
    ConsoleLoggingService,
    InMemoryTaskManager,
};
```

#### For Core Components (Advanced Users)

Framework internals are grouped under `core`:

```rust
// Core negotiation logic
use radkit::runtime::core::negotiator::{Negotiator, DefaultNegotiator};

// Execution logic (native-only)
#[cfg(not(all(target_os = "wasi", target_env = "p1")))]
use radkit::runtime::core::execution::RequestExecutor;
```

### 4.3. Adding New Implementations

The module structure is designed to scale. To add a new service implementation:

**Example: Adding Redis-backed memory service**

1. Create `radkit/src/runtime/memory/redis.rs`
2. Implement the `MemoryService` trait
3. Export from `radkit/src/runtime/memory/mod.rs`:
   ```rust
   pub mod redis;
   pub use redis::RedisMemoryService;
   ```
4. Users can now import: `use radkit::runtime::memory::RedisMemoryService;`

**Example: Adding Keycloak authentication**

1. Create `radkit/src/runtime/auth/keycloak.rs`
2. Implement the `AuthService` trait
3. Export from `radkit/src/runtime/auth/mod.rs`:
   ```rust
   pub mod keycloak;
   pub use keycloak::KeycloakAuthService;
   ```
4. Users can now import: `use radkit::runtime::auth::KeycloakAuthService;`

This pattern maintains consistency across all services while supporting multiple implementations per service type.

### 4.4. Cross-Platform Considerations

The runtime module maintains separate implementations for native and WASM targets:

- **Native implementations** (`#[cfg(not(all(target_os = "wasi", target_env = "p1")))]`)
  - Use `DashMap` for thread-safe concurrent access
  - Include web server components (`axum`, `tower-http`)
  - Support full `DefaultRuntime` with `.serve()` method

- **WASM implementations** (`#[cfg(all(target_os = "wasi", target_env = "p1"))]`)
  - Use `RefCell` for single-threaded access
  - Exclude web server dependencies
  - Support `DefaultRuntime` with services only (`.serve()` returns `NotImplemented`)

Both targets share the same trait interfaces, ensuring portability of agent code.
