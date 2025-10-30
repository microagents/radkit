//! Comprehensive unit tests for radkit agent module.

mod common;

use radkit::agent::{LlmFunction, LlmWorker};
use radkit::models::{Content, ContentPart, Event, Thread};
use radkit::tools::{
    BaseTool, BaseToolset, DefaultExecutionState, FunctionTool, SimpleToolset, ToolCall,
    ToolContext, ToolResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

// ============================================================================
// Test Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
struct SimpleResponse {
    message: String,
    count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
struct WeatherResponse {
    temperature: f64,
    condition: String,
    location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
struct ComplexResponse {
    title: String,
    items: Vec<String>,
    metadata: serde_json::Value,
}

// ============================================================================
// LlmFunction Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_new() {
    let llm = common::MockLlm::new("test-model");
    let _function = LlmFunction::<SimpleResponse>::new(llm);
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_with_system_instructions() {
    let llm = common::MockLlm::new("test-model");
    let _function = LlmFunction::<SimpleResponse>::new_with_system_instructions(
        llm,
        "You are a helpful assistant",
    );
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_run() {
    // Create mock LLM with structured output
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Hello",
            "count": 42
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let thread = Thread::from_user("Test input");
    let result = function.run(thread).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.message, "Hello");
    assert_eq!(response.count, 42);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_run_and_continue() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Response",
            "count": 1
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let thread = Thread::from_user("Input");
    let result = function.run_and_continue(thread).await;

    assert!(result.is_ok());
    let (response, continued_thread) = result.unwrap();
    assert_eq!(response.message, "Response");
    assert_eq!(response.count, 1);

    // Thread should have events
    assert!(!continued_thread.events().is_empty());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_with_str_input() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Hi",
            "count": 0
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let result = function.run("Test").await;
    assert!(result.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_with_string_input() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Hi",
            "count": 0
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let result = function.run(String::from("Test")).await;
    assert!(result.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_complex_type() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "title": "Test Title",
            "items": ["item1", "item2"],
            "metadata": {"key": "value"}
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<ComplexResponse>::new(llm);

    let result = function.run("Generate complex response").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.title, "Test Title");
    assert_eq!(response.items.len(), 2);
    assert_eq!(response.metadata["key"], "value");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_with_system_instructions_applied() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Response with system",
            "count": 100
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function =
        LlmFunction::<SimpleResponse>::new_with_system_instructions(llm, "Be concise and direct");

    let result = function.run("Test").await;
    assert!(result.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_error_no_structured_output() {
    // LLM returns content without structured output tool call
    let content = Content::from_text("Just text, no tool call");

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let result = function.run("Test").await;
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_function_error_invalid_schema() {
    // LLM returns structured output with wrong schema
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "wrong_field": "value"
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let function = LlmFunction::<SimpleResponse>::new(llm);

    let result = function.run("Test").await;
    assert!(result.is_err());
}

// ============================================================================
// LlmWorker Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_builder() {
    let llm = common::MockLlm::new("test-model");
    let _worker = LlmWorker::<SimpleResponse>::builder(llm).build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_system_instructions() {
    let llm = common::MockLlm::new("test-model");
    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_system_instructions("You are an expert")
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_tool() {
    let llm = common::MockLlm::new("test-model");
    let tool = Arc::new(common::MockTool::new("test_tool", "A test tool")) as Arc<dyn BaseTool>;

    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(tool)
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_tools() {
    let llm = common::MockLlm::new("test-model");
    let tool1 = Arc::new(common::MockTool::new("tool1", "desc")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "desc")) as Arc<dyn BaseTool>;

    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tools(vec![tool1, tool2])
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_toolset() {
    let llm = common::MockLlm::new("test-model");
    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;
    let toolset = Arc::new(SimpleToolset::new(vec![tool]));

    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_toolset(toolset)
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_toolsets() {
    let llm = common::MockLlm::new("test-model");
    let toolset1 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;

    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_toolsets(vec![toolset1, toolset2])
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_max_iterations() {
    let llm = common::MockLlm::new("test-model");
    let _worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_max_iterations(10)
        .build();
    // Should compile without errors
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_run_no_tools() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Worker response",
            "count": 5
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let worker = LlmWorker::<SimpleResponse>::builder(llm).build();

    let result = worker.run("Test input").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.message, "Worker response");
    assert_eq!(response.count, 5);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_run_and_continue() {
    let tool_call = ToolCall::new(
        "call_1",
        "radkit_structured_output",
        json!({
            "message": "Continue response",
            "count": 99
        }),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(tool_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);
    let worker = LlmWorker::<SimpleResponse>::builder(llm).build();

    let result = worker.run_and_continue("Input").await;

    assert!(result.is_ok());
    let (response, thread) = result.unwrap();
    assert_eq!(response.message, "Continue response");
    assert!(!thread.events().is_empty());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_has_tools() {
    let llm1 = common::MockLlm::new("test-model");
    let worker_no_tools = LlmWorker::<SimpleResponse>::builder(llm1).build();
    assert!(!worker_no_tools.has_tools());

    let llm2 = common::MockLlm::new("test-model");
    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;
    let worker_with_tools = LlmWorker::<SimpleResponse>::builder(llm2)
        .with_tool(tool)
        .build();
    assert!(worker_with_tools.has_tools());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_toolset() {
    let llm = common::MockLlm::new("test-model");
    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;
    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(tool)
        .build();

    assert!(worker.toolset().is_some());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_tool_execution() {
    // Create a tool that returns structured output on second call
    let calculator = Arc::new(FunctionTool::new(
        "calculate",
        "Performs calculation",
        |args, _ctx| {
            Box::pin(async move {
                let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
                let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
                ToolResult::success(json!({"result": a + b}))
            })
        },
    )) as Arc<dyn BaseTool>;

    // First response: LLM calls the tool
    let tool_call1 = ToolCall::new("call_1", "calculate", json!({"a": 5, "b": 3}));
    let first_content = Content::from_parts(vec![ContentPart::ToolCall(tool_call1)]);

    // Second response: LLM returns structured output
    let tool_call2 = ToolCall::new(
        "call_2",
        "radkit_structured_output",
        json!({
            "message": "Result calculated",
            "count": 8
        }),
    );
    let second_content = Content::from_parts(vec![ContentPart::ToolCall(tool_call2)]);

    let llm = common::MockLlm::new("test-model")
        .with_response(first_content)
        .with_response(second_content);

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(calculator)
        .build();

    let result = worker.run("Calculate 5 + 3").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.count, 8);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_multiple_tool_calls() {
    let tool1 = Arc::new(FunctionTool::new("tool1", "First tool", |_args, _ctx| {
        Box::pin(async { ToolResult::success(json!({"tool": 1})) })
    })) as Arc<dyn BaseTool>;

    let tool2 = Arc::new(FunctionTool::new("tool2", "Second tool", |_args, _ctx| {
        Box::pin(async { ToolResult::success(json!({"tool": 2})) })
    })) as Arc<dyn BaseTool>;

    // First response: Call multiple tools
    let call1 = ToolCall::new("call_1", "tool1", json!({}));
    let call2 = ToolCall::new("call_2", "tool2", json!({}));
    let first_content = Content::from_parts(vec![
        ContentPart::ToolCall(call1),
        ContentPart::ToolCall(call2),
    ]);

    // Second response: Return structured output
    let structured_call = ToolCall::new(
        "call_3",
        "radkit_structured_output",
        json!({
            "message": "Both tools executed",
            "count": 2
        }),
    );
    let second_content = Content::from_parts(vec![ContentPart::ToolCall(structured_call)]);

    let llm = common::MockLlm::new("test-model")
        .with_response(first_content)
        .with_response(second_content);

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tools(vec![tool1, tool2])
        .build();

    let result = worker.run("Use both tools").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.count, 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_max_iterations_exceeded() {
    // Create content that always calls tools (never returns structured output)
    let tool = Arc::new(FunctionTool::new(
        "infinite_tool",
        "Tool that keeps calling",
        |_args, _ctx| Box::pin(async { ToolResult::success(json!({})) }),
    )) as Arc<dyn BaseTool>;

    let tool_call = ToolCall::new("call", "infinite_tool", json!({}));
    let looping_content = Content::from_parts(vec![ContentPart::ToolCall(tool_call.clone())]);

    // Make the LLM return the same tool call repeatedly
    let mut llm = common::MockLlm::new("test-model");
    for _ in 0..25 {
        llm = llm.with_response(looping_content.clone());
    }

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(tool)
        .with_max_iterations(3)
        .build();

    let result = worker.run("This will loop").await;

    // Should error due to exceeding max iterations
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_with_mixed_tools_and_toolsets() {
    let tool = Arc::new(common::MockTool::new("individual_tool", "desc")) as Arc<dyn BaseTool>;
    let toolset_tool = Arc::new(common::MockTool::new("toolset_tool", "desc")) as Arc<dyn BaseTool>;
    let toolset = Arc::new(SimpleToolset::new(vec![toolset_tool]));

    let structured_call = ToolCall::new(
        "call",
        "radkit_structured_output",
        json!({"message": "Done", "count": 0}),
    );
    let content = Content::from_parts(vec![ContentPart::ToolCall(structured_call)]);

    let llm = common::MockLlm::new("test-model").with_response(content);

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(tool)
        .with_toolset(toolset)
        .build();

    let result = worker.run("Test").await;
    assert!(result.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_tool_error_handling() {
    let failing_tool = Arc::new(FunctionTool::new(
        "fail_tool",
        "Always fails",
        |_args, _ctx| Box::pin(async { ToolResult::error("Tool execution failed") }),
    )) as Arc<dyn BaseTool>;

    // First: LLM calls the failing tool
    let tool_call1 = ToolCall::new("call_1", "fail_tool", json!({}));
    let first_content = Content::from_parts(vec![ContentPart::ToolCall(tool_call1)]);

    // Second: LLM returns structured output after seeing the error
    let structured_call = ToolCall::new(
        "call_2",
        "radkit_structured_output",
        json!({
            "message": "Handled error",
            "count": 0
        }),
    );
    let second_content = Content::from_parts(vec![ContentPart::ToolCall(structured_call)]);

    let llm = common::MockLlm::new("test-model")
        .with_response(first_content)
        .with_response(second_content);

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(failing_tool)
        .build();

    let result = worker.run("Test error handling").await;

    // Worker should complete successfully despite tool error
    assert!(result.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_llm_worker_stateful_tool_execution() {
    let counter_tool = Arc::new(FunctionTool::new(
        "counter",
        "Increments counter",
        |_args, ctx| {
            Box::pin(async move {
                let current = ctx
                    .state()
                    .get_state("count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let new_count = current + 1;
                ctx.state().set_state("count", json!(new_count));
                ToolResult::success(json!({"count": new_count}))
            })
        },
    )) as Arc<dyn BaseTool>;

    // First: Call counter tool twice
    let call1 = ToolCall::new("call_1", "counter", json!({}));
    let call2 = ToolCall::new("call_2", "counter", json!({}));
    let first_content = Content::from_parts(vec![
        ContentPart::ToolCall(call1),
        ContentPart::ToolCall(call2),
    ]);

    // Second: Return structured output
    let structured = ToolCall::new(
        "call_3",
        "radkit_structured_output",
        json!({
            "message": "Counter called twice",
            "count": 2
        }),
    );
    let second_content = Content::from_parts(vec![ContentPart::ToolCall(structured)]);

    let llm = common::MockLlm::new("test-model")
        .with_response(first_content)
        .with_response(second_content);

    let worker = LlmWorker::<SimpleResponse>::builder(llm)
        .with_tool(counter_tool)
        .build();

    let result = worker.run("Test state").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.count, 2);
}

// ============================================================================
// Integration Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_full_agent_workflow() {
    // Simulates a realistic agent workflow with tools

    let weather_tool = Arc::new(FunctionTool::new(
        "get_weather",
        "Get weather for a location",
        |args, _ctx| {
            Box::pin(async move {
                let location = args
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                ToolResult::success(json!({
                    "temperature": 72.5,
                    "condition": "Sunny",
                    "location": location
                }))
            })
        },
    )) as Arc<dyn BaseTool>;

    // First: LLM calls weather tool
    let weather_call = ToolCall::new(
        "call_1",
        "get_weather",
        json!({"location": "San Francisco"}),
    );
    let first_response = Content::from_parts(vec![ContentPart::ToolCall(weather_call)]);

    // Second: LLM returns structured output with weather info
    let structured = ToolCall::new(
        "call_2",
        "radkit_structured_output",
        json!({
            "temperature": 72.5,
            "condition": "Sunny",
            "location": "San Francisco"
        }),
    );
    let second_response = Content::from_parts(vec![ContentPart::ToolCall(structured)]);

    let llm = common::MockLlm::new("test-model")
        .with_response(first_response)
        .with_response(second_response);

    let worker = LlmWorker::<WeatherResponse>::builder(llm)
        .with_system_instructions("You are a weather assistant")
        .with_tool(weather_tool)
        .build();

    let result = worker.run("What's the weather in San Francisco?").await;

    assert!(result.is_ok());
    let weather = result.unwrap();
    assert_eq!(weather.temperature, 72.5);
    assert_eq!(weather.condition, "Sunny");
    assert_eq!(weather.location, "San Francisco");
}
