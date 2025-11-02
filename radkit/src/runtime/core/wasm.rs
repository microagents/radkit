//! WASM runtime implementation.
//!
//! This module provides [`WasmRuntime`], a runtime implementation specifically
//! designed for WebAssembly targets. It provides a single-threaded execution
//! environment suitable for WASM deployments.
//!
//! # Overview
//!
//! - [`WasmRuntime`]: Runtime implementation for wasm32 targets
//!
//! # Examples
//!
//! ```ignore
//! use radkit::runtime::WasmRuntime;
//!
//! // Create and configure WASM runtime
//! let runtime = WasmRuntime::new()
//!     .agents(vec![my_agent])
//!     .serve().await?;
//! ```

use crate::agent::AgentDefinition;
use crate::errors::{AgentError, AgentResult};

/// Runtime implementation tailored for wasm32 targets.
///
/// This runtime provides agent execution in WebAssembly environments.
/// Native builds should use [`DefaultRuntime`] instead. The `WasmRuntime`
/// is designed for single-threaded WASM execution and can be hosted
/// in WASM-compatible environments like browsers or edge runtimes.
///
/// [`DefaultRuntime`]: super::DefaultRuntime
pub struct WasmRuntime {
    agents: Vec<AgentDefinition>,
}

impl WasmRuntime {
    /// Constructs a new, empty runtime.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runtime = WasmRuntime::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self { agents: Vec::new() }
    }

    /// Attaches agent definitions to be exposed by this runtime.
    ///
    /// This method consumes the runtime and returns it, allowing for
    /// method chaining in the builder pattern.
    ///
    /// # Arguments
    ///
    /// * `agents` - Vector of agent definitions to deploy
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let runtime = WasmRuntime::new()
    ///     .agents(vec![agent1, agent2]);
    /// ```
    #[must_use]
    pub fn agents(mut self, agents: Vec<AgentDefinition>) -> Self {
        self.agents = agents;
        self
    }

    /// Returns a slice of configured agent definitions.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// for agent in runtime.agent_definitions() {
    ///     println!("Agent: {}", agent.name());
    /// }
    /// ```
    #[must_use]
    pub fn agent_definitions(&self) -> &[AgentDefinition] {
        &self.agents
    }

    /// Starts serving the agents (WASM targets only).
    ///
    /// This method starts the WASM runtime and begins serving the configured
    /// agents. It runs until the runtime is stopped.
    ///
    /// # Platform Support
    ///
    /// This method is only available on `wasm32` targets. On native targets,
    /// it returns an error indicating the operation is not supported.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::NotImplemented`] on wasm32 until the implementation
    /// is complete. Returns [`AgentError::InvalidConfiguration`] on non-wasm32
    /// targets.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// WasmRuntime::new()
    ///     .agents(vec![my_agent])
    ///     .serve().await?;
    /// ```
    #[cfg(all(target_os = "wasi", target_env = "p1"))]
    pub async fn serve(self) -> AgentResult<()> {
        // Store agents for future use
        let _agents = self.agents;
        Err(AgentError::NotImplemented {
            feature: "WASM runtime wiring".to_string(),
        })
    }

    /// Serve is only meaningful for wasm32 builds.
    ///
    /// On non-wasm32 targets, this method returns an error indicating that
    /// `WasmRuntime` should not be used. Use [`DefaultRuntime`] instead.
    ///
    /// [`DefaultRuntime`]: super::DefaultRuntime
    #[cfg(not(all(target_os = "wasi", target_env = "p1")))]
    pub async fn serve(self) -> AgentResult<()> {
        Err(AgentError::InvalidConfiguration {
            field: "runtime".to_string(),
            reason: "WasmRuntime::serve is only available on wasm32 targets. Use DefaultRuntime for native targets.".to_string(),
        })
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new()
    }
}
