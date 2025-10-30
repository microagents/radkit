#![allow(clippy::needless_update)]

// Core context types
pub mod context;

// User-facing services
pub mod auth;
pub mod logging;
pub mod memory;
pub mod task_manager;

// Core framework components (advanced API)
pub mod core;

// Web server (native only)
#[cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]
pub mod web;

// Re-export service traits for convenience
pub use auth::AuthService;
pub use logging::{LogLevel, LoggingService};
pub use memory::{MemoryService, MemoryServiceExt};
pub use task_manager::{ListTasksFilter, PaginatedResult, Task, TaskEvent, TaskManager};

// Re-export default implementations for convenience
#[cfg(feature = "runtime")]
pub use auth::StaticAuthService;
#[cfg(feature = "runtime")]
pub use core::negotiator::DefaultNegotiator;
#[cfg(feature = "runtime")]
pub use logging::ConsoleLoggingService;
#[cfg(feature = "runtime")]
pub use memory::InMemoryMemoryService;
#[cfg(feature = "runtime")]
pub use task_manager::InMemoryTaskManager;

use crate::{MaybeSend, MaybeSync};
use std::sync::Arc;

#[cfg(feature = "runtime")]
use {
    crate::errors::{AgentError, AgentResult},
    crate::models::BaseLlm,
    crate::runtime::core::event_bus::TaskEventBus,
};

// Conditional imports for the native-only server implementation
#[cfg(all(feature = "runtime", not(all(target_os = "wasi", target_env = "p1"))))]
use {
    axum::{
        routing::{get, post},
        Router,
    },
    tower_http::trace::TraceLayer,
    tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt},
};

/// Core trait for runtime implementations.
///
/// Runtimes provide access to services needed during agent execution,
/// including session management, memory storage, and logging.
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
    #[cfg(feature = "runtime")]
    fn get_default_llm(&self) -> Arc<dyn BaseLlm>;

    /// Returns the negotiator for handling pre-task conversations.
    #[cfg(feature = "runtime")]
    fn negotiator(&self) -> Arc<dyn core::negotiator::Negotiator>;

    /// Returns the task event bus used for streaming.
    #[cfg(feature = "runtime")]
    fn event_bus(&self) -> Arc<TaskEventBus>;

    /// Convenience method for accessing auth service.
    fn auth(&self) -> Arc<dyn AuthService> {
        self.auth_service()
    }

    /// Convenience method for accessing memory service.
    fn memory(&self) -> Arc<dyn MemoryService> {
        self.memory_service()
    }

    /// Convenience method for accessing logging service.
    fn logging(&self) -> Arc<dyn LoggingService> {
        self.logging_service()
    }
}

/// Default runtime implementation for native targets.
///
/// This runtime can be configured with agents and started as a local server.
/// Use the builder pattern: `DefaultRuntime::new().agents(my_agents).serve(addr).await`.
#[cfg(feature = "runtime")]
#[derive(Clone)]
pub struct DefaultRuntime {
    auth_service: Arc<dyn AuthService>,
    task_manager: Arc<dyn TaskManager>,
    memory_service: Arc<dyn MemoryService>,
    logging_service: Arc<dyn LoggingService>,
    base_llm: Arc<dyn BaseLlm>,
    negotiator: Arc<dyn core::negotiator::Negotiator>,
    event_bus: Arc<TaskEventBus>,
    pub(crate) agents: Arc<Vec<crate::agent::AgentDefinition>>,
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
            event_bus: Arc::new(TaskEventBus::new()),
            agents: Arc::new(Vec::new()),
        }
    }

    pub fn agents(mut self, agents: Vec<crate::agent::AgentDefinition>) -> Self {
        self.agents = Arc::new(agents);
        self
    }

    /// Starts the local development server at the given address.
    ///
    /// This is an async method that runs until the server is stopped.
    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    pub async fn serve(self, address: impl AsRef<str>) -> AgentResult<()> {
        // Initialize tracing subscriber for logging
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(
                std::env::var("RUST_LOG")
                    .unwrap_or_else(|_| "radkit=debug,tower_http=debug".into()),
            ))
            .with(tracing_subscriber::fmt::layer())
            .init();

        // Share the runtime state with the handlers
        let shared_runtime = Arc::new(self);

        // Build the Axum router
        let app = Router::new()
            .route(
                "/:agent_id/.well-known/agent-card.json",
                get(web::agent_card_handler),
            )
            .route("/:agent_id/:version/rpc", post(web::json_rpc_handler))
            .route(
                "/:agent_id/:version/message:stream",
                post(web::message_stream_handler),
            )
            .route(
                "/:agent_id/:version/tasks/:task_id:subscribe",
                post(web::task_resubscribe_handler),
            )
            .with_state(shared_runtime)
            .layer(TraceLayer::new_for_http());

        let address = address.as_ref();
        tracing::debug!("starting server on {}", address);

        // Run the server
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|e| AgentError::ServerStartFailed(e.to_string()))?;

        axum::serve(listener, app.into_make_service())
            .await
            .map_err(|e| AgentError::ServerStartFailed(e.to_string()))
    }

    /// Placeholder for WASM targets where serve is not available.
    #[cfg(all(target_os = "wasi", target_env = "p1"))]
    pub async fn serve(self, _address: impl AsRef<str>) -> AgentResult<()> {
        Err(AgentError::NotImplemented {
            feature: "serve is not available for WASM targets".to_string(),
        })
    }
}

#[cfg(feature = "runtime")]
impl Runtime for DefaultRuntime {
    fn auth_service(&self) -> Arc<dyn AuthService> {
        self.auth_service.clone()
    }

    fn task_manager(&self) -> Arc<dyn TaskManager> {
        self.task_manager.clone()
    }

    fn memory_service(&self) -> Arc<dyn MemoryService> {
        self.memory_service.clone()
    }

    fn logging_service(&self) -> Arc<dyn LoggingService> {
        self.logging_service.clone()
    }

    fn get_default_llm(&self) -> Arc<dyn BaseLlm> {
        self.base_llm.clone()
    }

    #[cfg(feature = "runtime")]
    fn negotiator(&self) -> Arc<dyn core::negotiator::Negotiator> {
        self.negotiator.clone()
    }

    #[cfg(feature = "runtime")]
    fn event_bus(&self) -> Arc<TaskEventBus> {
        self.event_bus.clone()
    }
}
