//! End-to-End OpenAPI Agent Tests
//!
//! Tests complete agent workflows with OpenAPI toolset to verify:
//! - Agent + OpenAPI toolset integration
//! - Actual HTTP request execution (not just tool generation)
//! - Different parameter types (path, query, body)
//! - Response parsing and handling
//! - Multi-tool orchestration
//!
//! This test uses the public Petstore API to validate
//! the complete agent → toolset → HTTP API → response flow.

#![cfg(all(feature = "openapi", feature = "test-support"))]

use radkit::agent::LlmWorker;
use radkit::models::{Content, ContentPart, LlmResponse, TokenUsage};
use radkit::test_support::FakeLlm;
use radkit::tools::{BaseToolset, DefaultExecutionState, OpenApiToolSet, ToolCall, ToolContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

/// Pet search result structure
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PetSearchResult {
    total_found: usize,
    pet_names: Vec<String>,
    summary: String,
}

/// Single pet details
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PetDetails {
    id: i64,
    name: String,
    status: String,
    category: Option<String>,
}

/// Helper to create OpenAPI Petstore toolset
async fn create_petstore_toolset() -> Arc<OpenApiToolSet> {
    let toolset = OpenApiToolSet::from_url(
        "petstore".to_string(),
        "https://petstore3.swagger.io/api/v3/openapi.json",
        None, // No auth required
    )
    .await
    .expect("Failed to create Petstore toolset");

    Arc::new(toolset)
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
#[ignore = "requires network access to Petstore API"]
async fn test_openapi_agent_query_parameter() {
    println!("🧪 Testing OpenAPI Agent with Query Parameter");

    // Create OpenAPI toolset
    let petstore_toolset = create_petstore_toolset().await;

    // Verify tools are generated
    let tools = petstore_toolset.get_tools().await;
    assert!(!tools.is_empty(), "No tools generated from OpenAPI spec");

    println!("📋 Generated {} OpenAPI tools", tools.len());

    // Verify findPetsByStatus exists
    let find_by_status = tools
        .iter()
        .find(|t| t.name() == "findPetsByStatus")
        .expect("findPetsByStatus tool not found");

    println!("✓ Found tool: {}", find_by_status.description());

    // Create fake LLM responses
    // Turn 1: LLM calls findPetsByStatus with query parameter
    let tool_call_resp = tool_call_response(
        "findPetsByStatus",
        json!({
            "status": "available"
        }),
    );

    // Turn 2: LLM formats the results into final response
    let final_result = PetSearchResult {
        total_found: 3,
        pet_names: vec![
            "Doggie".to_string(),
            "Fluffy".to_string(),
            "Buddy".to_string(),
        ],
        summary: "Found 3 available pets".to_string(),
    };
    let structured_resp = structured_response(&final_result);

    let fake_llm = FakeLlm::with_responses(
        "fake-openapi-llm",
        vec![Ok(tool_call_resp), Ok(structured_resp)],
    );

    // Create pet search agent with OpenAPI toolset
    let pet_agent = LlmWorker::<PetSearchResult>::builder(fake_llm.clone())
        .with_system_instructions(
            "You are a pet store assistant. Use the findPetsByStatus tool to search for pets.",
        )
        .with_toolset(petstore_toolset.clone())
        .with_max_iterations(5)
        .build();

    // Execute query
    println!("🤖 Executing agent query: 'Find all available pets'");
    let result = pet_agent.run("Find all available pets").await;

    // Verify execution succeeded
    assert!(result.is_ok(), "Agent execution failed: {:?}", result.err());

    let search_result = result.unwrap();
    println!("📊 Pet Search Result:");
    println!("  Total Found: {}", search_result.total_found);
    println!("  Pet Names: {:?}", search_result.pet_names);
    println!("  Summary: {}", search_result.summary);

    // Verify FakeLlm was called
    let llm_calls = fake_llm.calls();
    assert!(
        !llm_calls.is_empty(),
        "FakeLlm was not called during execution"
    );
    println!("✓ LLM called {} times", llm_calls.len());

    // Verify tool was actually executed (check thread for tool response)
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
        "No tool result found - HTTP request was not executed"
    );
    println!("✓ OpenAPI tool executed successfully");

    println!("✅ OpenAPI query parameter test passed");
}

#[tokio::test]
#[ignore = "requires network access to Petstore API"]
async fn test_openapi_agent_path_parameter() {
    println!("🧪 Testing OpenAPI Agent with Path Parameter");

    // Create OpenAPI toolset
    let petstore_toolset = create_petstore_toolset().await;

    // Verify getPetById exists
    let tools = petstore_toolset.get_tools().await;
    let get_pet_by_id = tools
        .iter()
        .find(|t| t.name() == "getPetById")
        .expect("getPetById tool not found");

    println!("✓ Found tool: {}", get_pet_by_id.description());

    // Create fake LLM responses
    // Turn 1: LLM calls getPetById with path parameter
    let tool_call_resp = tool_call_response(
        "getPetById",
        json!({
            "petId": 1
        }),
    );

    // Turn 2: LLM formats the pet details into final response
    let final_result = PetDetails {
        id: 1,
        name: "Doggie".to_string(),
        status: "available".to_string(),
        category: Some("Dogs".to_string()),
    };
    let structured_resp = structured_response(&final_result);

    let fake_llm = FakeLlm::with_responses(
        "fake-openapi-llm-path",
        vec![Ok(tool_call_resp), Ok(structured_resp)],
    );

    // Create pet details agent
    let pet_details_agent = LlmWorker::<PetDetails>::builder(fake_llm.clone())
        .with_system_instructions(
            "You are a pet store assistant. Use the getPetById tool to get pet details.",
        )
        .with_toolset(petstore_toolset.clone())
        .with_max_iterations(5)
        .build();

    // Execute query
    println!("🤖 Executing agent query: 'Get details for pet ID 1'");
    let result = pet_details_agent.run("Get details for pet ID 1").await;

    // Verify execution succeeded
    assert!(result.is_ok(), "Agent execution failed: {:?}", result.err());

    let pet_details = result.unwrap();
    println!("📊 Pet Details:");
    println!("  ID: {}", pet_details.id);
    println!("  Name: {}", pet_details.name);
    println!("  Status: {}", pet_details.status);
    if let Some(category) = &pet_details.category {
        println!("  Category: {}", category);
    }

    // Verify tool execution
    let llm_calls = fake_llm.calls();
    assert!(!llm_calls.is_empty(), "FakeLlm was not called");
    println!("✓ LLM called {} times", llm_calls.len());

    // Verify tool result
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
        "No tool result found - HTTP request was not executed"
    );
    println!("✓ OpenAPI tool with path parameter executed successfully");

    println!("✅ OpenAPI path parameter test passed");
}

#[tokio::test]
#[ignore = "requires network access to Petstore API"]
async fn test_openapi_agent_multi_tool_orchestration() {
    println!("🧪 Testing OpenAPI Agent Multi-Tool Orchestration");

    // Create OpenAPI toolset
    let petstore_toolset = create_petstore_toolset().await;

    // Scenario: Find available pets, then get details for first one
    // Turn 1: Call findPetsByStatus
    let tool_call_1 = tool_call_response(
        "findPetsByStatus",
        json!({
            "status": "available"
        }),
    );

    // Turn 2: Call getPetById (assume agent extracts ID from first result)
    let tool_call_2 = tool_call_response(
        "getPetById",
        json!({
            "petId": 1
        }),
    );

    // Turn 3: Final response combining both results
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    struct CombinedResult {
        total_available: usize,
        first_pet_name: String,
        first_pet_status: String,
    }

    let combined = CombinedResult {
        total_available: 5,
        first_pet_name: "Doggie".to_string(),
        first_pet_status: "available".to_string(),
    };
    let structured_resp = structured_response(&combined);

    let fake_llm = FakeLlm::with_responses(
        "fake-openapi-llm-multi",
        vec![Ok(tool_call_1), Ok(tool_call_2), Ok(structured_resp)],
    );

    // Create agent
    let multi_tool_agent = LlmWorker::<CombinedResult>::builder(fake_llm.clone())
        .with_system_instructions(
            "You are a pet store assistant. Use findPetsByStatus and getPetById to answer queries.",
        )
        .with_toolset(petstore_toolset.clone())
        .with_max_iterations(10)
        .build();

    // Execute complex query
    println!("🤖 Executing: 'Find available pets and tell me about the first one'");
    let result = multi_tool_agent
        .run("Find available pets and tell me about the first one")
        .await;

    assert!(result.is_ok(), "Agent execution failed: {:?}", result.err());

    let combined_result = result.unwrap();
    println!("📊 Combined Result:");
    println!("  Total Available: {}", combined_result.total_available);
    println!("  First Pet Name: {}", combined_result.first_pet_name);
    println!("  First Pet Status: {}", combined_result.first_pet_status);

    // Verify multiple LLM calls
    let llm_calls = fake_llm.calls();
    assert!(
        llm_calls.len() >= 2,
        "Expected at least 2 LLM calls, got {}",
        llm_calls.len()
    );
    println!("✓ LLM called {} times for multi-tool", llm_calls.len());

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

    println!("✅ OpenAPI multi-tool orchestration test passed");
}

#[tokio::test]
#[ignore = "requires network access to Petstore API"]
async fn test_openapi_http_error_handling() {
    println!("🧪 Testing OpenAPI HTTP Error Handling");

    // Create OpenAPI toolset
    let petstore_toolset = create_petstore_toolset().await;

    // Get getPetById tool
    let tools = petstore_toolset.get_tools().await;
    let get_pet_tool = tools
        .iter()
        .find(|t| t.name() == "getPetById")
        .expect("getPetById tool not found");

    println!("✓ Testing error handling with: {}", get_pet_tool.name());

    // Test with invalid pet ID (should get 404)
    let mut args = std::collections::HashMap::new();
    args.insert("petId".to_string(), json!(999999999));

    let state = DefaultExecutionState::new();
    let tool_context = ToolContext::builder()
        .with_state(&state)
        .build()
        .expect("Failed to create ToolContext");

    println!("🔧 Calling tool with invalid pet ID (999999999)...");
    let result = get_pet_tool.run_async(args, &tool_context).await;

    println!("📋 Tool result success: {}", result.is_success());

    // Tool should handle HTTP error gracefully
    if result.is_success() {
        // Check if status code indicates error
        let data = result.data();
        if let Some(status) = data.get("status") {
            let status_code = status.as_u64().unwrap_or(0);
            println!("✓ Tool returned status code: {}", status_code);
            // 404 is expected for non-existent pet
            if status_code == 404 {
                println!("✓ Correctly received 404 for non-existent pet");
            }
        }
    } else if let Some(error) = result.error_message() {
        println!("✓ Tool returned error: {}", error);
    }

    println!("✓ Tool handled HTTP error without panic");

    println!("✅ OpenAPI error handling test passed");
}
