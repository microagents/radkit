//! Integration tests for tool macro with LlmWorker.
//!
//! These tests recreate the worker_execution.rs tests but using the #[tool] macro
//! instead of manual FunctionTool construction, demonstrating the macro's value
//! in real-world usage.

use radkit::agent::LlmWorker;
use radkit::models::{Content, ContentPart, LlmResponse, Thread, TokenUsage};
use radkit::test_support::{structured_response, FakeLlm};
use radkit::tools::{BaseTool, ToolCall, ToolContext, ToolResult};
use radkit_macros::tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

// ============================================================================
// Test 1: Basic weather tool using macro
// ============================================================================

#[derive(Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
}

#[derive(Deserialize, JsonSchema)]
struct GetWeatherArgs {
    location: String,
}

#[tool(description = "Get current weather")]
async fn get_weather(args: GetWeatherArgs) -> ToolResult {
    ToolResult::success(json!({
        "temperature": 72.5,
        "condition": "sunny",
        "location": args.location
    }))
}

#[tokio::test]
async fn test_llm_worker_with_macro_tool() {
    // First response: LLM calls the weather tool
    let tool_call_response = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-1",
            "get_weather",
            json!({"location": "Seattle"}),
        ))]),
        TokenUsage::empty(),
    );

    // Second response: LLM returns structured output after seeing tool result
    let final_response = WeatherReport {
        location: "Seattle".to_string(),
        temperature: 72.5,
        condition: "sunny".to_string(),
    };

    // Create worker with macro-generated tool
    let worker_llm = FakeLlm::with_responses(
        "fake-llm",
        [
            Ok(tool_call_response),
            Ok(structured_response(&final_response)),
        ],
    );

    let worker = LlmWorker::<WeatherReport>::builder(worker_llm)
        .with_tool(get_weather)
        .build();

    let thread = Thread::from_user("What's the weather in Seattle?");
    let report = worker.run(thread).await.unwrap();

    assert_eq!(report.location, "Seattle");
    assert_eq!(report.temperature, 72.5);
    assert_eq!(report.condition, "sunny");
}

// ============================================================================
// Test 2: Multiple tools with macro
// ============================================================================

#[derive(Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
struct CalculationResult {
    result: f64,
    steps: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
struct MathArgs {
    a: f64,
    b: f64,
}

#[tool(description = "Add two numbers")]
async fn add(args: MathArgs) -> ToolResult {
    ToolResult::success(json!(args.a + args.b))
}

#[tool(description = "Multiply two numbers")]
async fn multiply(args: MathArgs) -> ToolResult {
    ToolResult::success(json!(args.a * args.b))
}

#[tokio::test]
async fn test_llm_worker_multiple_macro_tools() {
    // Simulate: (2 + 3) * 4 = 20
    // First call: add(2, 3) = 5
    let response1 = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-1",
            "add",
            json!({"a": 2.0, "b": 3.0}),
        ))]),
        TokenUsage::empty(),
    );

    // Second call: multiply(5, 4) = 20
    let response2 = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-2",
            "multiply",
            json!({"a": 5.0, "b": 4.0}),
        ))]),
        TokenUsage::empty(),
    );

    // Final response: structured output
    let final_result = CalculationResult {
        result: 20.0,
        steps: vec![
            "add(2, 3) = 5".to_string(),
            "multiply(5, 4) = 20".to_string(),
        ],
    };

    let llm = FakeLlm::with_responses(
        "fake-llm",
        [
            Ok(response1),
            Ok(response2),
            Ok(structured_response(&final_result)),
        ],
    );

    let worker = LlmWorker::<CalculationResult>::builder(llm)
        .with_tool(add)
        .with_tool(multiply)
        .build();

    let thread = Thread::from_user("Calculate: (2 + 3) * 4");
    let result = worker.run(thread).await.unwrap();

    assert_eq!(result.result, 20.0);
    assert_eq!(result.steps.len(), 2);
}

// ============================================================================
// Test 3: Tool with ToolContext using macro
// ============================================================================

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct StateResult {
    saved: bool,
    value: String,
}

#[derive(Deserialize, JsonSchema)]
struct SaveStateArgs {
    key: String,
    value: String,
}

#[tool(description = "Save state")]
async fn save_state(args: SaveStateArgs, ctx: &ToolContext<'_>) -> ToolResult {
    ctx.state().set_state(&args.key, json!(args.value));
    ToolResult::success(json!({"saved": true, "key": args.key}))
}

#[tool(description = "Get state")]
async fn get_state(args: GetStateArgs, ctx: &ToolContext<'_>) -> ToolResult {
    let value = ctx.state().get_state(&args.key);
    match value {
        Some(val) => ToolResult::success(json!({"found": true, "value": val})),
        None => ToolResult::success(json!({"found": false})),
    }
}

#[derive(Deserialize, JsonSchema)]
struct GetStateArgs {
    key: String,
}

#[tokio::test]
async fn test_llm_worker_with_context_aware_tools() {
    // First call: save state
    let save_call = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-1",
            "save_state",
            json!({"key": "user_name", "value": "Alice"}),
        ))]),
        TokenUsage::empty(),
    );

    // Second call: get state
    let get_call = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-2",
            "get_state",
            json!({"key": "user_name"}),
        ))]),
        TokenUsage::empty(),
    );

    // Final response
    let final_response = StateResult {
        saved: true,
        value: "Alice".to_string(),
    };

    let llm = FakeLlm::with_responses(
        "fake-llm",
        [
            Ok(save_call),
            Ok(get_call),
            Ok(structured_response(&final_response)),
        ],
    );

    let worker = LlmWorker::<StateResult>::builder(llm)
        .with_tool(save_state)
        .with_tool(get_state)
        .build();

    let thread = Thread::from_user("Save 'Alice' as user_name and retrieve it");
    let result = worker.run(thread).await.unwrap();

    assert!(result.saved);
    assert_eq!(result.value, "Alice");
}

// ============================================================================
// Test 4: Function name becomes tool name
// ============================================================================

#[derive(Deserialize, JsonSchema)]
struct ApiArgs {
    endpoint: String,
}

#[tool(description = "Fetch data from API")]
async fn fetch_api(args: ApiArgs) -> ToolResult {
    ToolResult::success(json!({
        "endpoint": args.endpoint,
        "data": "mock data"
    }))
}

#[tokio::test]
async fn test_macro_function_name_as_tool_name() {
    let tool = &fetch_api as &dyn BaseTool;
    assert_eq!(tool.name(), "fetch_api");
    assert_eq!(tool.description(), "Fetch data from API");
}

// ============================================================================
// Test 5: Optional parameters with defaults
// ============================================================================

fn default_limit() -> usize {
    10
}

#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[tool(description = "Search with optional limit")]
async fn search(args: SearchArgs) -> ToolResult {
    ToolResult::success(json!({
        "query": args.query,
        "limit": args.limit,
        "results": vec!["result1", "result2"]
    }))
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct SearchResult {
    query: String,
    count: usize,
}

#[tokio::test]
async fn test_macro_tool_with_optional_params() {
    // Test with explicit limit
    let call1 = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-1",
            "search",
            json!({"query": "rust", "limit": 5}),
        ))]),
        TokenUsage::empty(),
    );

    // Test with default limit
    let call2 = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-2",
            "search",
            json!({"query": "async"}),
        ))]),
        TokenUsage::empty(),
    );

    let final_response = SearchResult {
        query: "async".to_string(),
        count: 2,
    };

    let llm = FakeLlm::with_responses(
        "fake-llm",
        [
            Ok(call1),
            Ok(call2),
            Ok(structured_response(&final_response)),
        ],
    );

    let worker = LlmWorker::<SearchResult>::builder(llm)
        .with_tool(search)
        .build();

    let thread = Thread::from_user("Search for rust then async");
    let result = worker.run(thread).await.unwrap();

    assert_eq!(result.query, "async");
    assert_eq!(result.count, 2);
}

// ============================================================================
// Test 6: Complex nested structures
// ============================================================================

#[derive(Deserialize, JsonSchema)]
struct Address {
    #[allow(dead_code)] // Required for JSON deserialization but not accessed in test assertions
    street: String,
    city: String,
    country: String,
}

#[derive(Deserialize, JsonSchema)]
struct CreateUserArgs {
    name: String,
    age: u32,
    address: Address,
}

#[tool(description = "Create user with nested address")]
async fn create_user(args: CreateUserArgs) -> ToolResult {
    ToolResult::success(json!({
        "user_id": "u123",
        "name": args.name,
        "age": args.age,
        "city": args.address.city,
        "country": args.address.country
    }))
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct UserCreated {
    user_id: String,
    success: bool,
}

#[tokio::test]
async fn test_macro_tool_with_nested_structures() {
    let tool_call = LlmResponse::new(
        Content::from_parts(vec![ContentPart::ToolCall(ToolCall::new(
            "call-1",
            "create_user",
            json!({
                "name": "Alice",
                "age": 30,
                "address": {
                    "street": "123 Main St",
                    "city": "Seattle",
                    "country": "USA"
                }
            }),
        ))]),
        TokenUsage::empty(),
    );

    let final_response = UserCreated {
        user_id: "u123".to_string(),
        success: true,
    };

    let llm = FakeLlm::with_responses(
        "fake-llm",
        [Ok(tool_call), Ok(structured_response(&final_response))],
    );

    let worker = LlmWorker::<UserCreated>::builder(llm)
        .with_tool(create_user)
        .build();

    let thread = Thread::from_user("Create user Alice in Seattle");
    let result = worker.run(thread).await.unwrap();

    assert_eq!(result.user_id, "u123");
    assert!(result.success);
}

// ============================================================================
// Test 7: Verify tool declarations are correct
// ============================================================================

#[tokio::test]
async fn test_macro_tool_declarations() {
    let add_tool = &add as &dyn BaseTool;
    let declaration = add_tool.declaration();

    // Verify name and description
    assert_eq!(declaration.name(), "add");
    assert_eq!(declaration.description(), "Add two numbers");

    // Verify schema structure
    let schema = declaration.parameters();
    assert!(schema.is_object());

    let properties = schema.get("properties").expect("Should have properties");
    assert!(properties.is_object());

    // Verify parameter 'a' exists
    let props = properties.as_object().unwrap();
    assert!(props.contains_key("a"), "Should have parameter 'a'");
    assert!(props.contains_key("b"), "Should have parameter 'b'");

    // Verify required fields
    let required = schema.get("required").expect("Should have required");
    assert!(required.is_array());
    let req_arr = required.as_array().unwrap();
    assert!(
        req_arr.iter().any(|v| v.as_str() == Some("a")),
        "Should require 'a'"
    );
    assert!(
        req_arr.iter().any(|v| v.as_str() == Some("b")),
        "Should require 'b'"
    );
}

// ============================================================================
// Test 8: Error handling with invalid arguments
// ============================================================================

#[tokio::test]
async fn test_macro_tool_invalid_arguments() {
    use radkit::tools::DefaultExecutionState;

    let tool = &add as &dyn BaseTool;
    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder()
        .with_state(&state)
        .build()
        .expect("context");

    // Missing required parameter 'b'
    let result = tool
        .run_async(
            std::collections::HashMap::from([("a".to_string(), json!(5.0))]),
            &ctx,
        )
        .await;

    assert!(!result.is_success());
    assert!(result
        .error_message()
        .unwrap()
        .contains("missing field `b`"));
}
