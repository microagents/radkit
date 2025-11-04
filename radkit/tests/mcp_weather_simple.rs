//! Simple MCP Weather Server Tests
//!
//! Tests MCP toolset with a real MCP weather server to verify:
//! - Connection to MCP server (HTTP)
//! - Tool discovery from MCP server
//! - Tool declarations are generated correctly
//!
//! This test uses a real MCP weather server at:
//! https://mcp-servers.microagents.io/weather

#![cfg(feature = "mcp")]

use radkit::tools::{BaseToolset, MCPConnectionParams, MCPToolset};
use std::time::Duration;

/// Helper function to create MCP weather toolset
fn create_mcp_weather_toolset() -> MCPToolset {
    let mcp_connection = MCPConnectionParams::Http {
        url: "https://mcp-servers.microagents.io/weather".to_string(),
        timeout: Duration::from_secs(30),
        headers: Default::default(),
    };

    MCPToolset::new(mcp_connection)
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_connection() {
    println!("🧪 Testing MCP Weather Server Connection");

    let toolset = create_mcp_weather_toolset();

    // Test connection
    let result = toolset.test_connection().await;

    assert!(
        result.is_ok(),
        "Failed to connect to MCP weather server: {:?}",
        result.err()
    );

    println!("✅ MCP connection test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_tool_discovery() {
    println!("🧪 Testing MCP Tool Discovery");

    let toolset = create_mcp_weather_toolset();

    // Get tools from MCP server
    let tools = toolset.get_tools().await;

    // Verify tools were discovered
    assert!(
        !tools.is_empty(),
        "No tools were discovered from MCP weather server"
    );

    println!("📋 Discovered {} tools from MCP server:", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name(), tool.description());
    }

    // Verify at least one weather-related tool exists
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    let has_weather_tool = tool_names
        .iter()
        .any(|name| name.to_lowercase().contains("weather"));

    assert!(
        has_weather_tool,
        "Expected at least one weather-related tool. Found: {:?}",
        tool_names
    );

    println!("✅ MCP tool discovery test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_tool_declarations() {
    println!("🧪 Testing MCP Tool Declarations");

    let toolset = create_mcp_weather_toolset();

    // Get tools
    let tools = toolset.get_tools().await;

    assert!(
        !tools.is_empty(),
        "No tools discovered from MCP server"
    );

    // Verify each tool has a valid declaration
    for tool in tools {
        let declaration = tool.declaration();

        println!("🔍 Checking tool: {}", tool.name());

        // Verify basic declaration properties
        assert_eq!(
            declaration.name(),
            tool.name(),
            "Declaration name doesn't match tool name"
        );

        assert!(
            !declaration.description().is_empty(),
            "Tool '{}' has empty description",
            tool.name()
        );

        // Verify parameters schema is valid JSON
        let params = declaration.parameters();
        assert!(
            params.is_object() || params.is_null(),
            "Tool '{}' parameters must be an object or null, got: {:?}",
            tool.name(),
            params
        );

        println!(
            "  ✓ {}: {} (params: {})",
            declaration.name(),
            declaration.description(),
            if params.is_null() { "none" } else { "present" }
        );
    }

    println!("✅ MCP tool declarations test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_tool_filtering() {
    println!("🧪 Testing MCP Tool Filtering");

    use radkit::tools::MCPToolFilter;

    let mcp_connection = MCPConnectionParams::Http {
        url: "https://mcp-servers.microagents.io/weather".to_string(),
        timeout: Duration::from_secs(30),
        headers: Default::default(),
    };

    // Get all tools first
    let toolset_all = MCPToolset::new(mcp_connection.clone());
    let all_tools = toolset_all.get_tools().await;
    let all_tool_names: Vec<String> = all_tools.iter().map(|t| t.name().to_string()).collect();

    println!("📋 All available tools: {:?}", all_tool_names);

    // Test Include filter
    if let Some(first_tool_name) = all_tool_names.first() {
        let toolset_filtered = MCPToolset::new(mcp_connection.clone())
            .with_filter(MCPToolFilter::Include(vec![first_tool_name.clone()]));

        let filtered_tools = toolset_filtered.get_tools().await;

        assert_eq!(
            filtered_tools.len(),
            1,
            "Include filter should return exactly one tool"
        );
        assert_eq!(filtered_tools[0].name(), first_tool_name);

        println!("✓ Include filter works: {}", first_tool_name);
    }

    // Test Exclude filter
    if all_tool_names.len() > 1 {
        let exclude_name = &all_tool_names[0];
        let toolset_excluded = MCPToolset::new(mcp_connection.clone())
            .with_filter(MCPToolFilter::Exclude(vec![exclude_name.clone()]));

        let excluded_tools = toolset_excluded.get_tools().await;

        assert!(
            !excluded_tools.iter().any(|t| t.name() == exclude_name),
            "Excluded tool should not be present"
        );

        assert_eq!(
            excluded_tools.len(),
            all_tools.len() - 1,
            "Should have one fewer tool after exclusion"
        );

        println!("✓ Exclude filter works: excluded {}", exclude_name);
    }

    println!("✅ MCP tool filtering test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_toolset_cleanup() {
    println!("🧪 Testing MCP Toolset Cleanup");

    let toolset = create_mcp_weather_toolset();

    // Get tools to establish connection
    let tools = toolset.get_tools().await;
    assert!(!tools.is_empty(), "Should have tools");

    // Close the toolset
    toolset.close().await;

    println!("✅ MCP toolset cleanup test passed");
}
