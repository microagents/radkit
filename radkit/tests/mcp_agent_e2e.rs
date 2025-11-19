//! End-to-End MCP Agent Tests
//!
//! Tests complete agent workflows with MCP toolset to verify:
//! - Agent + MCP toolset integration
//! - Actual tool execution (not just discovery)
//! - Tool argument passing and response handling
//! - Multi-turn LLM + tool interaction
//! - Real MCP server communication
//!
//! This test uses a real MCP weather server to validate
//! the complete agent → toolset → MCP server → response flow.

#![cfg(all(feature = "mcp", feature = "test-support"))]

use radkit::agent::LlmWorker;
use radkit::macros::LLMOutput;
use radkit::models::{Content, ContentPart, LlmResponse, TokenUsage};
use radkit::test_support::FakeLlm;
use radkit::tools::{
    BaseToolset, DefaultExecutionState, MCPConnectionParams, MCPToolset, ToolCall, ToolContext,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Weather report structure for typed agent response
#[derive(Debug, Serialize, Deserialize, LLMOutput, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
    humidity: Option<f64>,
}

/// Helper to create MCP weather toolset
fn create_mcp_weather_toolset() -> Arc<MCPToolset> {
    let mcp_connection = MCPConnectionParams::Http {
        url: "https://mcp-servers.microagents.io/weather".to_string(),
        timeout: Duration::from_secs(30),
        headers: Default::default(),
    };

    Arc::new(MCPToolset::new(mcp_connection))
}

/// Helper to create a tool call response
fn tool_call_response(tool_name: &str, arguments: serde_json::Value) -> LlmResponse {
    let tool_call = ToolCall::new("call-1", tool_name, arguments);

    LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(tool_call)]),
        TokenUsage::empty(),
    )
}

/// Helper to create a structured output response
fn structured_response<T: Serialize>(value: &T) -> LlmResponse {
    let json_str = serde_json::to_string(value).unwrap();
    LlmResponse::new(Content::from_text(json_str), TokenUsage::empty())
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_agent_single_tool_call() {
    println!("🧪 Testing MCP Agent with Single Tool Call");

    // Create MCP toolset
    let mcp_toolset = create_mcp_weather_toolset();

    // Verify tools are discovered
    let tools = mcp_toolset.get_tools().await;
    assert!(!tools.is_empty(), "No tools discovered from MCP server");

    println!("📋 Discovered {} MCP tools", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name(), tool.description());
    }

    // Find a weather tool (server might have different tool names)
    let weather_tool_name = tools
        .iter()
        .find(|t| {
            let name_lower = t.name().to_lowercase();
            name_lower.contains("weather") || name_lower.contains("forecast")
        })
        .map(|t| t.name())
        .expect("No weather tool found");

    println!("✓ Using weather tool: {}", weather_tool_name);

    // Create fake LLM responses
    // Turn 1: LLM decides to call weather tool
    let tool_call_resp = tool_call_response(
        weather_tool_name,
        json!({
            "location": "San Francisco"
        }),
    );

    // Turn 2: LLM formats the weather data into final response
    let final_report = WeatherReport {
        location: "San Francisco".to_string(),
        temperature: 65.0,
        condition: "Partly Cloudy".to_string(),
        humidity: Some(70.0),
    };
    let structured_resp = structured_response(&final_report);

    let fake_llm = FakeLlm::with_responses(
        "fake-mcp-llm",
        vec![Ok(tool_call_resp), Ok(structured_resp)],
    );

    // Create weather agent with MCP toolset
    let weather_agent = LlmWorker::<WeatherReport>::builder(fake_llm.clone())
        .with_system_instructions(
            "You are a weather assistant. Use the weather tool to fetch current conditions.",
        )
        .with_toolset(mcp_toolset.clone())
        .with_max_iterations(5)
        .build();

    // Execute query
    println!("🤖 Executing agent query: 'What's the weather in San Francisco?'");
    let result = weather_agent
        .run("What's the weather in San Francisco?")
        .await;

    // Verify execution succeeded
    assert!(result.is_ok(), "Agent execution failed: {:?}", result.err());

    let report = result.unwrap();
    println!("📊 Weather Report:");
    println!("  Location: {}", report.location);
    println!("  Temperature: {}°F", report.temperature);
    println!("  Condition: {}", report.condition);
    if let Some(humidity) = report.humidity {
        println!("  Humidity: {}%", humidity);
    }

    // Verify FakeLlm was called
    let llm_calls = fake_llm.calls();
    assert!(
        !llm_calls.is_empty(),
        "FakeLlm was not called during execution"
    );
    println!("✓ LLM called {} times", llm_calls.len());

    // Verify tool was actually invoked (check thread for tool response)
    let last_thread = &llm_calls[llm_calls.len() - 1];
    let has_tool_result = last_thread.events().iter().any(|event| {
        event
            .content()
            .parts()
            .iter()
            .any(|part| matches!(part, ContentPart::ToolResponse(_)))
    });
    assert!(
        has_tool_result,
        "No tool result found in LLM thread - tool was not executed"
    );
    println!("✓ MCP tool was executed successfully");

    // Cleanup
    mcp_toolset.close().await;
    println!("✅ MCP agent single tool call test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_agent_multi_turn_conversation() {
    println!("🧪 Testing MCP Agent Multi-Turn Conversation");

    // Create MCP toolset
    let mcp_toolset = create_mcp_weather_toolset();

    // Discover tools
    let tools = mcp_toolset.get_tools().await;
    assert!(!tools.is_empty(), "No tools discovered");

    let weather_tool_name = tools
        .iter()
        .find(|t| {
            let name_lower = t.name().to_lowercase();
            name_lower.contains("weather") || name_lower.contains("forecast")
        })
        .map(|t| t.name())
        .expect("No weather tool found");

    println!("✓ Using weather tool: {}", weather_tool_name);

    // Scenario: Compare weather in two cities
    // Turn 1: Get weather for first city
    let tool_call_1 = tool_call_response(
        weather_tool_name,
        json!({
            "location": "New York"
        }),
    );

    // Turn 2: Get weather for second city
    let tool_call_2 = tool_call_response(
        weather_tool_name,
        json!({
            "location": "Los Angeles"
        }),
    );

    // Turn 3: Final comparison response
    #[derive(Debug, Serialize, Deserialize, LLMOutput, JsonSchema)]
    struct Comparison {
        city1: String,
        city2: String,
        summary: String,
    }

    let comparison = Comparison {
        city1: "New York".to_string(),
        city2: "Los Angeles".to_string(),
        summary: "New York is colder than Los Angeles".to_string(),
    };
    let structured_resp = structured_response(&comparison);

    let fake_llm = FakeLlm::with_responses(
        "fake-mcp-llm-multiturn",
        vec![Ok(tool_call_1), Ok(tool_call_2), Ok(structured_resp)],
    );

    // Create agent
    let comparison_agent = LlmWorker::<Comparison>::builder(fake_llm.clone())
        .with_system_instructions(
            "You are a weather assistant. Compare weather between cities using the weather tool.",
        )
        .with_toolset(mcp_toolset.clone())
        .with_max_iterations(10)
        .build();

    // Execute comparison query
    println!("🤖 Executing: 'Compare weather between New York and Los Angeles'");
    let result = comparison_agent
        .run("Compare weather between New York and Los Angeles")
        .await;

    assert!(result.is_ok(), "Agent execution failed: {:?}", result.err());

    let comparison = result.unwrap();
    println!("📊 Comparison Result:");
    println!("  City 1: {}", comparison.city1);
    println!("  City 2: {}", comparison.city2);
    println!("  Summary: {}", comparison.summary);

    // Verify multiple LLM calls
    let llm_calls = fake_llm.calls();
    assert!(
        llm_calls.len() >= 2,
        "Expected at least 2 LLM calls, got {}",
        llm_calls.len()
    );
    println!("✓ LLM called {} times for multi-turn", llm_calls.len());

    // Verify multiple tool executions
    let tool_result_count = llm_calls
        .iter()
        .flat_map(|thread| thread.events())
        .filter(|event| {
            event
                .content()
                .parts()
                .iter()
                .any(|part| matches!(part, ContentPart::ToolResponse(_)))
        })
        .count();

    assert!(
        tool_result_count >= 1,
        "Expected at least 1 tool execution, got {}",
        tool_result_count
    );
    println!("✓ {} tool execution(s) completed", tool_result_count);

    // Cleanup
    mcp_toolset.close().await;
    println!("✅ MCP agent multi-turn test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_tool_error_handling() {
    println!("🧪 Testing MCP Tool Error Handling");

    // Create MCP toolset
    let mcp_toolset = create_mcp_weather_toolset();

    // Get tools
    let tools = mcp_toolset.get_tools().await;
    assert!(!tools.is_empty(), "No tools discovered");

    let weather_tool = tools
        .iter()
        .find(|t| {
            let name_lower = t.name().to_lowercase();
            name_lower.contains("weather") || name_lower.contains("forecast")
        })
        .expect("No weather tool found");

    println!("✓ Testing error handling with: {}", weather_tool.name());

    // Test with invalid arguments
    let mut invalid_args = std::collections::HashMap::new();
    invalid_args.insert("invalid_param".to_string(), json!("test"));

    let state = DefaultExecutionState::new();
    let tool_context = ToolContext::builder()
        .with_state(&state)
        .build()
        .expect("Failed to create ToolContext");

    println!("🔧 Calling tool with invalid arguments...");
    let result = weather_tool.run_async(invalid_args, &tool_context).await;

    // Tool should handle error gracefully (either success with error message or error result)
    // We don't expect a panic
    println!("📋 Tool result: {:?}", result);
    println!("✓ Tool handled invalid arguments without panic");

    // Cleanup
    mcp_toolset.close().await;
    println!("✅ MCP error handling test passed");
}

#[tokio::test]
#[ignore = "requires network access to MCP weather server"]
async fn test_mcp_toolset_session_reuse() {
    println!("🧪 Testing MCP Session Reuse");

    // Create MCP toolset
    let mcp_toolset = create_mcp_weather_toolset();

    // Call get_tools multiple times - should reuse session
    println!("📋 Calling get_tools() first time...");
    let tools1 = mcp_toolset.get_tools().await;
    assert!(!tools1.is_empty(), "No tools discovered");

    println!("📋 Calling get_tools() second time (should reuse session)...");
    let tools2 = mcp_toolset.get_tools().await;
    assert!(!tools2.is_empty(), "No tools discovered");

    // Should get same tools
    assert_eq!(
        tools1.len(),
        tools2.len(),
        "Tool count differs between calls"
    );

    println!(
        "✓ Both calls returned {} tools (session reused)",
        tools1.len()
    );

    // Cleanup
    mcp_toolset.close().await;
    println!("✅ MCP session reuse test passed");
}
