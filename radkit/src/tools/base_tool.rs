//! Base tool trait for agent capabilities.
//!
//! This module defines the [`BaseTool`] trait, which is the fundamental interface
//! that all tools must implement to be usable by agents.
//!
//! # Overview
//!
//! Tools are callable functions that extend agent capabilities. Each tool must:
//! - Have a unique name and human-readable description
//! - Declare its parameter schema as a JSON Schema
//! - Implement async execution with proper error handling
//!
//! # Thread Safety
//!
//! All tools must be `Send + Sync` (or `MaybeSend + MaybeSync` for WASM compatibility)
//! to support concurrent execution in async contexts.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::tools::{BaseTool, FunctionDeclaration, ToolResult, ToolContext};
//! use serde_json::{json, Value};
//! use std::collections::HashMap;
//!
//! struct WeatherTool;
//!
//! #[async_trait]
//! impl BaseTool for WeatherTool {
//!     fn name(&self) -> &str {
//!         "get_weather"
//!     }
//!
//!     fn description(&self) -> &str {
//!         "Get current weather for a location"
//!     }
//!
//!     fn declaration(&self) -> FunctionDeclaration {
//!         FunctionDeclaration::new(
//!             self.name(),
//!             self.description(),
//!             json!({"type": "object", "properties": {"location": {"type": "string"}}})
//!         )
//!     }
//!
//!     async fn run_async(&self, args: HashMap<String, Value>, _ctx: &ToolContext<'_>) -> ToolResult {
//!         let location = args.get("location").and_then(|v| v.as_str()).unwrap_or("Unknown");
//!         ToolResult::success(json!({"temp": 72, "location": location}))
//!     }
//! }
//! ```

use serde_json::Value;
use std::collections::HashMap;

use crate::tools::tool::{FunctionDeclaration, ToolResult};
use crate::tools::ToolContext;
use crate::{MaybeSend, MaybeSync};

/// Core trait for tools that can be invoked by an agent.
///
/// This trait defines the interface that all tools must implement. Tools are functions
/// that agents can call to interact with external systems, perform computations, or
/// access data.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` (via `MaybeSend + MaybeSync`) to support
/// concurrent execution across async tasks.
///
/// # Error Handling
///
/// The [`run_async`](BaseTool::run_async) method returns a [`ToolResult`] which can
/// represent either success or failure. Tools should catch errors and convert them
/// to [`ToolResult::error`] rather than panicking.
#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
pub trait BaseTool: MaybeSend + MaybeSync {
    /// Returns the unique identifier for this tool.
    ///
    /// The name should be stable, unique within a toolset, and use snake_case
    /// (e.g., "get_weather", "send_email").
    fn name(&self) -> &str;

    /// Returns a human-readable description of what the tool does.
    ///
    /// This description helps the LLM understand when to use the tool.
    /// Be concise but specific about the tool's purpose and behavior.
    fn description(&self) -> &str;

    /// Returns the function declaration for this tool.
    ///
    /// The declaration includes the tool's name, description, and JSON Schema
    /// for parameters. This tells the LLM what arguments the tool accepts.
    fn declaration(&self) -> FunctionDeclaration;

    /// Executes the tool with the provided arguments and context.
    ///
    /// # Arguments
    ///
    /// * `args` - Key-value map of arguments (validated against the parameter schema by the LLM)
    /// * `context` - Execution context providing access to state and configuration
    ///
    /// # Returns
    ///
    /// Returns a [`ToolResult`] indicating success or failure. Use:
    /// - [`ToolResult::success(data)`] for successful execution
    /// - [`ToolResult::error(message)`] for errors
    ///
    /// # Examples
    ///
    /// ```ignore
    /// async fn run_async(&self, args: HashMap<String, Value>, ctx: &ToolContext<'_>) -> ToolResult {
    ///     match self.fetch_data(&args) {
    ///         Ok(data) => ToolResult::success(json!(data)),
    ///         Err(e) => ToolResult::error(format!("Failed to fetch data: {}", e)),
    ///     }
    /// }
    /// ```
    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult;
}
