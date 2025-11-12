//! Toolset abstractions for grouping related tools.
//!
//! This module provides the [`BaseToolset`] trait and implementations for organizing
//! and composing collections of tools.
//!
//! # Overview
//!
//! - [`BaseToolset`]: Trait for tool collections with lifecycle management
//! - [`SimpleToolset`]: Basic in-memory collection of tools
//! - [`CombinedToolset`]: Composes two toolsets into one
//!
//! # Examples
//!
//! ```ignore
//! use radkit::tools::{SimpleToolset, FunctionTool, ToolResult};
//! use serde_json::json;
//!
//! // Create individual tools
//! let weather_tool = Box::new(FunctionTool::new(
//!     "get_weather",
//!     "Get weather info",
//!     |args, _| Box::pin(async { ToolResult::success(json!({"temp": 72})) })
//! ));
//!
//! // Create a toolset
//! let toolset = SimpleToolset::new(vec![weather_tool])
//!     .with_tool(another_tool);
//! ```

use std::sync::Arc;

use super::base_tool::BaseTool;
use crate::{MaybeSend, MaybeSync};

/// Base trait for toolsets - collections of related tools.
///
/// Toolsets group tools together and manage their lifecycle. Implementations
/// can provide tools from various sources (in-memory, remote MCP servers, etc.).
///
/// # Lifecycle
///
/// The [`close`](BaseToolset::close) method should be called when the toolset
/// is no longer needed to release resources like network connections or file handles.
///
/// # Thread Safety
///
/// Toolsets must be `Send + Sync` (via `MaybeSend + MaybeSync`) to support
/// concurrent access across async tasks.
#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
pub trait BaseToolset: MaybeSend + MaybeSync {
    /// Returns references to all tools in the toolset.
    ///
    /// # Performance Note
    ///
    /// This method returns references to tools, avoiding cloning the tools themselves.
    /// However, it does allocate a Vec to hold the references. The returned references
    /// are valid for the lifetime of the toolset.
    async fn get_tools(&self) -> Vec<&dyn BaseTool>;

    /// Performs cleanup and releases resources held by the toolset.
    ///
    /// This should be called when the toolset is no longer needed. For toolsets
    /// connected to external services (e.g., MCP servers), this closes connections.
    /// For simple in-memory toolsets, this is typically a no-op.
    ///
    /// Not calling `close()` may leak resources but won't cause undefined behavior.
    async fn close(&self);
}

/// Default implementation of `BaseToolset` for simple collections of tools
#[derive(Default)]
pub struct SimpleToolset {
    tools: Vec<Box<dyn BaseTool>>,
}

impl SimpleToolset {
    pub fn new<T>(tools: T) -> Self
    where
        T: IntoIterator<Item = Box<dyn BaseTool>>,
    {
        Self {
            tools: tools.into_iter().collect(),
        }
    }

    /// Add a single tool to this toolset.
    pub fn add_tool(&mut self, tool: Box<dyn BaseTool>) {
        self.tools.push(tool);
    }

    /// Extend the toolset with additional tools.
    pub fn add_tools<T>(&mut self, tools: T)
    where
        T: IntoIterator<Item = Box<dyn BaseTool>>,
    {
        self.tools.extend(tools);
    }

    /// Builder-style helper to add a tool while consuming the toolset.
    pub fn with_tool<U>(mut self, tool: U) -> Self
    where
        U: BaseTool + 'static,
    {
        self.add_tool(Box::new(tool));
        self
    }

    /// Builder-style helper to add multiple tools while consuming the toolset.
    pub fn with_tools<I, U>(mut self, tools: I) -> Self
    where
        I: IntoIterator<Item = U>,
        U: BaseTool + 'static,
    {
        for tool in tools {
            self.add_tool(Box::new(tool));
        }
        self
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseToolset for SimpleToolset {
    async fn get_tools(&self) -> Vec<&dyn BaseTool> {
        self.tools.iter().map(|b| b.as_ref()).collect()
    }

    async fn close(&self) {
        // Simple toolset doesn't need cleanup
    }
}

/// Combines two toolsets into a single toolset
///
/// This allows composing multiple toolsets together, enabling patterns like:
/// - Combining multiple MCP toolsets
/// - Combining MCP toolset with built-in tools
/// - Chaining multiple `CombinedToolsets` for complex compositions
///
/// # Tool Name Collisions
///
/// If both toolsets contain tools with the same name, both tools will be present
/// in the returned list from `get_tools()`. When used with `LlmWorker`, the last
/// tool added (from the right toolset) will override earlier ones with the same name.
pub struct CombinedToolset {
    left: Arc<dyn BaseToolset>,
    right: Arc<dyn BaseToolset>,
}

impl CombinedToolset {
    /// Create a new `CombinedToolset` from two toolsets
    ///
    /// # Example
    /// ```no_run
    /// use radkit::tools::{SimpleToolset, CombinedToolset};
    /// use std::sync::Arc;
    ///
    /// let mcp1 = Arc::new(SimpleToolset::new(vec![])); // Simplified example
    /// let mcp2 = Arc::new(SimpleToolset::new(vec![]));
    ///
    /// // Combine two MCP toolsets
    /// let combined = CombinedToolset::new(mcp1, mcp2);
    /// ```
    pub fn new(left: Arc<dyn BaseToolset>, right: Arc<dyn BaseToolset>) -> Self {
        Self { left, right }
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseToolset for CombinedToolset {
    async fn get_tools(&self) -> Vec<&dyn BaseTool> {
        let mut all_tools = self.left.get_tools().await;
        all_tools.extend(self.right.get_tools().await);
        all_tools
    }

    async fn close(&self) {
        // Close both toolsets
        self.left.close().await;
        self.right.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::function_tool::FunctionTool;
    use serde_json::json;

    fn build_tool(name: &str) -> Box<dyn BaseTool> {
        Box::new(FunctionTool::new(name, "test tool", |_, _| {
            Box::pin(async { crate::tools::ToolResult::success(json!(null)) })
        }))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn simple_toolset_returns_tools() {
        let toolset = SimpleToolset::new(vec![build_tool("alpha")]);

        let tools = toolset.get_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "alpha");

        // Closing should succeed even though it is a no-op
        toolset.close().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn combined_toolset_aggregates_tools() {
        let left = Arc::new(SimpleToolset::new(vec![build_tool("left")]));
        let right = Arc::new(SimpleToolset::new(vec![build_tool("right")]));

        let combined = CombinedToolset::new(left, right);
        let tools = combined.get_tools().await;

        let names: Vec<_> = tools.iter().map(|tool| tool.name().to_string()).collect();
        assert_eq!(names, vec!["left".to_string(), "right".to_string()]);

        combined.close().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn combined_toolset_handles_tool_name_clashes() {
        let left = Arc::new(SimpleToolset::new(vec![build_tool("clash")]));
        let right = Arc::new(SimpleToolset::new(vec![build_tool("clash")]));

        let combined = CombinedToolset::new(left, right);
        let tools = combined.get_tools().await;

        let names: Vec<_> = tools.iter().map(|tool| tool.name().to_string()).collect();
        // Expect both tools to be present, as the current implementation does not de-duplicate
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"clash".to_string()));

        combined.close().await;
    }
}
