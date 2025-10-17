use async_trait::async_trait;
use std::sync::Arc;

use super::base_tool::BaseTool;
use crate::events::ExecutionContext;

/// Base trait for toolsets - collections of related tools.
/// Similar to Python ADK's BaseToolset but adapted for Rust patterns.
#[async_trait]
pub trait BaseToolset: Send + Sync {
    /// Returns all tools in the toolset.
    async fn get_tools(&self) -> Vec<Arc<dyn BaseTool>>;

    /// Convert this toolset to a list of `ToolProviderConfig` for agent definition.
    fn to_tool_provider_configs(&self) -> Vec<crate::config::ToolProviderConfig>;

    /// Performs cleanup and releases resources held by the toolset.
    /// Called when the toolset is no longer needed.
    async fn close(&self);

    /// Processes the outgoing LLM request for this toolset.
    /// Called before individual tools process the LLM request.
    /// Can be used for toolset-level request modifications.
    async fn process_llm_request(&self, _context: &ExecutionContext) {
        // Default implementation does nothing
    }
}

/// Default implementation of BaseToolset for simple collections of tools
pub struct SimpleToolset {
    tools: Vec<Arc<dyn BaseTool>>,
}

impl SimpleToolset {
    pub fn new(tools: Vec<Arc<dyn BaseTool>>) -> Self {
        Self { tools }
    }

    /// Add a single tool to this toolset
    pub fn add_tool(&mut self, tool: Arc<dyn BaseTool>) {
        self.tools.push(tool);
    }

    /// Add multiple tools to this toolset
    pub fn add_tools(&mut self, tools: Vec<Arc<dyn BaseTool>>) {
        self.tools.extend(tools);
    }

    /// Builder pattern for adding a tool
    pub fn with_tool(mut self, tool: Arc<dyn BaseTool>) -> Self {
        self.tools.push(tool);
        self
    }

    /// Builder pattern for adding multiple tools
    pub fn with_tools(mut self, tools: Vec<Arc<dyn BaseTool>>) -> Self {
        self.tools.extend(tools);
        self
    }
}

#[async_trait]
impl BaseToolset for SimpleToolset {
    async fn get_tools(&self) -> Vec<Arc<dyn BaseTool>> {
        self.tools.clone()
    }

    fn to_tool_provider_configs(&self) -> Vec<crate::config::ToolProviderConfig> {
        self.tools
            .iter()
            .filter_map(|tool| tool.to_tool_provider_config())
            .collect()
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
/// - Chaining multiple CombinedToolsets for complex compositions
pub struct CombinedToolset {
    left: Arc<dyn BaseToolset>,
    right: Arc<dyn BaseToolset>,
}

impl CombinedToolset {
    /// Create a new CombinedToolset from two toolsets
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

#[async_trait]
impl BaseToolset for CombinedToolset {
    async fn get_tools(&self) -> Vec<Arc<dyn BaseTool>> {
        let mut all_tools = self.left.get_tools().await;
        all_tools.extend(self.right.get_tools().await);
        all_tools
    }

    fn to_tool_provider_configs(&self) -> Vec<crate::config::ToolProviderConfig> {
        let mut configs = self.left.to_tool_provider_configs();
        configs.extend(self.right.to_tool_provider_configs());
        configs
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
    use crate::tools::{
        ToolContext,
        base_tool::{BaseTool, FunctionDeclaration, ToolResult},
    };

    // Mock tool for testing
    #[derive(Debug)]
    struct MockTool {
        name: String,
        description: String,
    }

    impl MockTool {
        fn new(name: &str, description: &str) -> Arc<Self> {
            Arc::new(Self {
                name: name.to_string(),
                description: description.to_string(),
            })
        }
    }

    #[async_trait]
    impl BaseTool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn get_declaration(&self) -> Option<FunctionDeclaration> {
            Some(FunctionDeclaration {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: serde_json::json!({}),
            })
        }

        async fn run_async(
            &self,
            _args: std::collections::HashMap<String, serde_json::Value>,
            _context: &ToolContext<'_>,
        ) -> ToolResult {
            ToolResult::success(serde_json::Value::Null)
        }
    }

    #[tokio::test]
    async fn test_simple_toolset() {
        let tool1 = MockTool::new("tool1", "First tool");
        let tool2 = MockTool::new("tool2", "Second tool");
        let toolset = SimpleToolset::new(vec![tool1, tool2]);

        let tools = toolset.get_tools().await;
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name(), "tool1");
        assert_eq!(tools[1].name(), "tool2");
    }

    #[tokio::test]
    async fn test_simple_toolset_builder_pattern() {
        let tool1 = MockTool::new("tool1", "First");
        let tool2 = MockTool::new("tool2", "Second");
        let tool3 = MockTool::new("tool3", "Third");

        let toolset = SimpleToolset::new(vec![])
            .with_tool(tool1)
            .with_tools(vec![tool2, tool3]);

        let tools = toolset.get_tools().await;
        assert_eq!(tools.len(), 3);
    }

    #[tokio::test]
    async fn test_combined_toolset() {
        let base_tool1 = MockTool::new("base1", "Base tool 1");
        let base_tool2 = MockTool::new("base2", "Base tool 2");
        let base_toolset = Arc::new(SimpleToolset::new(vec![base_tool1, base_tool2]));

        let additional_tool1 = MockTool::new("add1", "Additional tool 1");
        let additional_toolset = Arc::new(SimpleToolset::new(vec![additional_tool1]));
        let combined = CombinedToolset::new(base_toolset, additional_toolset);

        let tools = combined.get_tools().await;
        assert_eq!(tools.len(), 3);

        let names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
        assert!(names.contains(&"base1".to_string()));
        assert!(names.contains(&"base2".to_string()));
        assert!(names.contains(&"add1".to_string()));
    }

    #[tokio::test]
    async fn test_empty_toolset_behavior() {
        let empty_toolset = SimpleToolset::new(vec![]);

        let tools = empty_toolset.get_tools().await;
        assert_eq!(tools.len(), 0);

        // Test adding tools to empty toolset
        let mut mutable_toolset = empty_toolset;
        let new_tool = MockTool::new("added_tool", "Tool added later");
        mutable_toolset.add_tool(new_tool);

        let tools = mutable_toolset.get_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "added_tool");
    }

    #[tokio::test]
    async fn test_combined_toolset_with_empty_base() {
        let empty_base = Arc::new(SimpleToolset::new(vec![]));
        let additional_tool = MockTool::new("only_additional", "The only tool");
        let additional_toolset = Arc::new(SimpleToolset::new(vec![additional_tool]));

        let combined = CombinedToolset::new(empty_base, additional_toolset);
        let tools = combined.get_tools().await;

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "only_additional");
    }

    #[tokio::test]
    async fn test_combined_toolset_with_no_additional() {
        let base_tool = MockTool::new("base_only", "Only base tool");
        let base_toolset = Arc::new(SimpleToolset::new(vec![base_tool]));
        let empty_toolset = Arc::new(SimpleToolset::new(vec![]));

        let combined = CombinedToolset::new(base_toolset, empty_toolset);
        let tools = combined.get_tools().await;

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "base_only");
    }

    #[tokio::test]
    async fn test_toolset_close_cleanup() {
        let tool = MockTool::new("cleanup_tool", "Tool for cleanup test");
        let toolset = SimpleToolset::new(vec![tool]);

        // Close should not panic and should complete successfully
        toolset.close().await;

        // Tools should still be accessible after close (for simple toolset)
        let tools = toolset.get_tools().await;
        assert_eq!(tools.len(), 1);
    }

    #[tokio::test]
    async fn test_toolset_thread_safety() {
        use std::sync::Arc;
        use tokio::task::JoinSet;

        let tools: Vec<Arc<dyn BaseTool>> = (0..10)
            .map(|i| {
                MockTool::new(&format!("tool_{}", i), &format!("Tool {}", i)) as Arc<dyn BaseTool>
            })
            .collect();

        let toolset = Arc::new(SimpleToolset::new(tools));
        let mut join_set = JoinSet::new();

        // Spawn multiple concurrent tasks accessing the toolset
        for _ in 0..5 {
            let toolset_clone = Arc::clone(&toolset);
            join_set.spawn(async move {
                let tools = toolset_clone.get_tools().await;
                tools.len()
            });
        }

        // All tasks should complete successfully
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }

        assert_eq!(results.len(), 5);
        for result in results {
            assert_eq!(result, 10);
        }
    }
}
