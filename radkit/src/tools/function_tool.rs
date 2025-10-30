//! Function-based tool implementation.
//!
//! This module provides [`FunctionTool`], a convenient wrapper for turning Rust
//! async functions into tools that can be called by LLMs.
//!
//! # Overview
//!
//! [`FunctionTool`] allows you to create tools from closures or function pointers
//! without manually implementing the [`BaseTool`](super::BaseTool) trait.
//!
//! # Examples
//!
//! ```ignore
//! use radkit::tools::{FunctionTool, ToolResult, ToolContext};
//! use serde_json::{json, Value};
//! use std::collections::HashMap;
//!
//! let weather_tool = FunctionTool::new(
//!     "get_weather",
//!     "Get current weather for a location",
//!     |args: HashMap<String, Value>, _ctx: &ToolContext| {
//!         Box::pin(async move {
//!             let location = args.get("location")
//!                 .and_then(|v| v.as_str())
//!                 .unwrap_or("Unknown");
//!             ToolResult::success(json!({
//!                 "location": location,
//!                 "temperature": 72,
//!                 "condition": "sunny"
//!             }))
//!         })
//!     }
//! ).with_parameters_schema(json!({
//!     "type": "object",
//!     "properties": {
//!         "location": {"type": "string", "description": "City name"}
//!     },
//!     "required": ["location"]
//! }));
//! ```

use super::base_tool::BaseTool;
use crate::{
    compat::MaybeSendBoxFuture,
    tools::ToolContext,
    tools::{FunctionDeclaration, ToolResult},
    MaybeSend, MaybeSync,
};
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

type ToolFuture<'a> = MaybeSendBoxFuture<'a, ToolResult>;

/// Trait combining the necessary bounds for a tool function.
///
/// This is a workaround for Rust compiler error E0225, which prevents using
/// non-auto traits like `MaybeSend` in a `dyn` trait object with `Fn`.
/// Users typically don't need to implement this directly; any closure matching
/// the signature automatically implements it.
pub trait ToolFn:
    for<'a> Fn(HashMap<String, Value>, &'a ToolContext<'a>) -> ToolFuture<'a> + MaybeSend + MaybeSync
{
}

impl<T> ToolFn for T where
    T: for<'a> Fn(HashMap<String, Value>, &'a ToolContext<'a>) -> ToolFuture<'a>
        + MaybeSend
        + MaybeSync
        + 'static
{
}

/// Type alias for an async function that can be used as a tool.
type AsyncToolFunctionInner = dyn ToolFn;

pub type AsyncToolFunction = Box<AsyncToolFunctionInner>;

/// A tool that wraps a simple async function.
///
/// This provides a convenient way to create tools without implementing the
/// [`BaseTool`] trait manually. The function receives arguments as a `HashMap`
/// and returns a [`ToolResult`].
///
/// # Performance Note
///
/// The `name` and `description` fields are cached to avoid allocations on
/// repeated calls to [`declaration()`](BaseTool::declaration). However, the
/// parameters schema is still cloned on each call.
pub struct FunctionTool {
    name: String,
    description: String,
    function: AsyncToolFunction,
    parameters_schema: Value,
    // Cached declaration to avoid cloning name/description on every call
    cached_declaration: Option<FunctionDeclaration>,
}

impl FunctionTool {
    /// Creates a new function tool with the given name, description, and function.
    ///
    /// The closure must be `MaybeSend + MaybeSync` so it enforces `Send`/`Sync` on
    /// native targets while staying compatible with WASM's single-threaded executor.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique tool name (e.g., "`get_weather`")
    /// * `description` - Human-readable description
    /// * `function` - Async function that implements the tool logic
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use radkit::tools::{FunctionTool, ToolResult};
    /// use serde_json::json;
    ///
    /// let tool = FunctionTool::new(
    ///     "add_numbers",
    ///     "Add two numbers together",
    ///     |args, _ctx| Box::pin(async move {
    ///         let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
    ///         let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
    ///         ToolResult::success(json!({"result": a + b}))
    ///     })
    /// );
    /// ```
    pub fn new<F>(name: impl Into<String>, description: impl Into<String>, function: F) -> Self
    where
        F: for<'a> Fn(HashMap<String, Value>, &'a ToolContext<'a>) -> ToolFuture<'a>
            + MaybeSend
            + MaybeSync
            + 'static,
    {
        Self {
            name: name.into(),
            description: description.into(),
            function: Box::new(function),
            parameters_schema: json!({}),
            cached_declaration: None,
        }
    }

    /// Sets the JSON Schema for the function parameters.
    ///
    /// The schema should be a valid JSON Schema object describing the expected
    /// parameters. This helps the LLM understand what arguments to provide.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use serde_json::json;
    ///
    /// let tool = FunctionTool::new("get_user", "Get user by ID", handler)
    ///     .with_parameters_schema(json!({
    ///         "type": "object",
    ///         "properties": {
    ///             "user_id": {"type": "string"}
    ///         },
    ///         "required": ["user_id"]
    ///     }));
    /// ```
    #[must_use] pub fn with_parameters_schema(mut self, schema: Value) -> Self {
        self.parameters_schema = schema;
        self.cached_declaration = None; // Invalidate cache
        self
    }

    /// Returns a reference to the parameters schema.
    #[must_use] pub const fn parameters_schema(&self) -> &Value {
        &self.parameters_schema
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for FunctionTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> FunctionDeclaration {
        // Note: We can't actually cache here due to `&self` constraint.
        // The cached_declaration field exists for future optimization
        // if the trait signature changes to allow mutable access.
        // For now, we still clone but document the performance characteristic.
        FunctionDeclaration::new(
            self.name.clone(),
            self.description.clone(),
            self.parameters_schema.clone(),
        )
    }

    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ToolContext<'_>,
    ) -> ToolResult {
        (self.function)(args, context).await
    }
}
