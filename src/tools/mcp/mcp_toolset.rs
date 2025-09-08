use super::{MCPConnectionParams, MCPSessionManager, MCPTool};
use crate::errors::{AgentError, AgentResult};
use crate::tools::{BaseTool, BaseToolset};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info};

/// Filter for selecting which MCP tools to include
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPToolFilter {
    /// Include all tools
    All,
    /// Include only tools with these exact names
    Include(Vec<String>),
    /// Exclude tools with these names
    Exclude(Vec<String>),
}

impl MCPToolFilter {
    /// Check if a tool matches this filter
    pub fn matches(&self, tool_name: &str) -> bool {
        match self {
            MCPToolFilter::All => true,
            MCPToolFilter::Include(names) => names.contains(&tool_name.to_string()),
            MCPToolFilter::Exclude(names) => !names.contains(&tool_name.to_string()),
        }
    }
}

/// MCPToolset that lazily discovers tools and manages connections
pub struct MCPToolset {
    session_manager: Arc<MCPSessionManager>,
    tool_filter: Option<MCPToolFilter>,
}

impl MCPToolset {
    /// Create a new MCPToolset with the given connection parameters
    pub fn new(connection_params: MCPConnectionParams) -> Self {
        Self {
            session_manager: Arc::new(MCPSessionManager::new(connection_params)),
            tool_filter: None,
        }
    }

    /// Add a tool filter to limit which tools are exposed
    pub fn with_filter(mut self, filter: MCPToolFilter) -> Self {
        self.tool_filter = Some(filter);
        self
    }

    /// Test the connection to the MCP server
    pub async fn test_connection(&self) -> AgentResult<()> {
        info!("Testing MCP connection");
        let session = self.session_manager.create_session(None).await?;

        // Try to list tools as a connection test
        match session.list_all_tools().await {
            Ok(_) => {
                info!("MCP connection test successful");
                Ok(())
            }
            Err(e) => {
                error!("MCP connection test failed: {:?}", e);
                Err(AgentError::ToolSetupFailed {
                    tool_name: "mcp_connection_test".to_string(),
                    reason: format!("MCP connection failed: {e:?}"),
                })
            }
        }
    }
}

#[async_trait]
impl BaseToolset for MCPToolset {
    async fn get_tools(&self) -> Vec<Arc<dyn BaseTool>> {
        debug!("Discovering MCP tools");

        // Create session for tool discovery
        let session = match self.session_manager.create_session(None).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to create MCP session for tool discovery: {}", e);
                return Vec::new();
            }
        };

        // Discover tools from server
        let tools_response = match session.list_all_tools().await {
            Ok(response) => response,
            Err(e) => {
                error!("Failed to list MCP tools: {:?}", e);
                return Vec::new();
            }
        };

        info!("Discovered {} MCP tools", tools_response.len());

        // Convert to Radkit tools
        let mut radkit_tools = Vec::new();
        for mcp_tool in tools_response {
            // Apply filter if configured
            if let Some(ref filter) = self.tool_filter {
                if !filter.matches(&mcp_tool.name) {
                    debug!("Filtering out tool: {}", mcp_tool.name);
                    continue;
                }
            }

            let tool = Arc::new(MCPTool::new(
                mcp_tool.name.to_string(),
                mcp_tool
                    .description
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                serde_json::Value::Object((*mcp_tool.input_schema).clone()),
                Arc::clone(&self.session_manager),
            ));

            debug!("Added MCP tool: {}", tool.name());
            radkit_tools.push(tool as Arc<dyn BaseTool>);
        }

        info!(
            "Created {} Radkit tools from MCP server",
            radkit_tools.len()
        );
        radkit_tools
    }

    async fn close(&self) {
        info!("Closing MCPToolset");
        self.session_manager.close().await;
    }

    async fn process_llm_request(&self, _context: &crate::events::ExecutionContext) {
        // MCPToolset doesn't need special LLM request processing
        // Tools are discovered dynamically and passed to the LLM
    }
}

impl std::fmt::Debug for MCPToolset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MCPToolset")
            .field("tool_filter", &self.tool_filter)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_tool_filter_all() {
        let filter = MCPToolFilter::All;
        assert!(filter.matches("any_tool"));
        assert!(filter.matches("another_tool"));
    }

    #[test]
    fn test_tool_filter_include() {
        let filter = MCPToolFilter::Include(vec!["tool1".to_string(), "tool2".to_string()]);
        assert!(filter.matches("tool1"));
        assert!(filter.matches("tool2"));
        assert!(!filter.matches("tool3"));
    }

    #[test]
    fn test_tool_filter_exclude() {
        let filter = MCPToolFilter::Exclude(vec!["bad_tool".to_string()]);
        assert!(filter.matches("good_tool"));
        assert!(!filter.matches("bad_tool"));
    }

    #[test]
    fn test_mcp_toolset_creation() {
        let params = MCPConnectionParams::Stdio {
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            env: HashMap::new(),
            timeout: std::time::Duration::from_secs(5),
        };

        let toolset = MCPToolset::new(params);
        assert!(toolset.tool_filter.is_none());

        let toolset = toolset.with_filter(MCPToolFilter::All);
        assert!(matches!(toolset.tool_filter, Some(MCPToolFilter::All)));
    }
}
