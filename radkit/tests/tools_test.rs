//! Comprehensive unit tests for radkit tools module.

mod common;

use radkit::tools::{
    BaseTool, BaseToolset, CombinedToolset, DefaultExecutionState, ExecutionState,
    FunctionDeclaration, FunctionTool, SimpleToolset, ToolCall, ToolContext, ToolResponse,
    ToolResult,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// ToolResult Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_result_success() {
    let result = ToolResult::success(json!({"status": "ok"}));

    assert!(result.is_success());
    assert!(!result.is_error());
    assert_eq!(result.data(), &json!({"status": "ok"}));
    assert!(result.error_message().is_none());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_result_error() {
    let result = ToolResult::error("Something went wrong");

    assert!(!result.is_success());
    assert!(result.is_error());
    assert_eq!(result.error_message(), Some("Something went wrong"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_result_error_with_string() {
    let error = String::from("Error occurred");
    let result = ToolResult::error(error);

    assert!(result.is_error());
    assert_eq!(result.error_message(), Some("Error occurred"));
}

// ============================================================================
// ToolCall Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_call_new() {
    let call = ToolCall::new("call_123", "my_tool", json!({"arg": "value"}));

    assert_eq!(call.id(), "call_123");
    assert_eq!(call.name(), "my_tool");
    assert_eq!(call.arguments(), &json!({"arg": "value"}));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_call_with_empty_args() {
    let call = ToolCall::new("id", "tool", json!({}));

    assert_eq!(call.arguments(), &json!({}));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_call_with_null_args() {
    let call = ToolCall::new("id", "tool", Value::Null);

    assert_eq!(call.arguments(), &Value::Null);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_call_clone() {
    let call1 = ToolCall::new("id", "tool", json!({"x": 1}));
    let call2 = call1.clone();

    assert_eq!(call1.id(), call2.id());
    assert_eq!(call1.name(), call2.name());
}

// ============================================================================
// ToolResponse Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_response_new() {
    let result = ToolResult::success(json!({"data": "result"}));
    let response = ToolResponse::new("call_id".to_string(), result);

    assert_eq!(response.tool_call_id(), "call_id");
    assert!(response.result().is_success());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_response_with_error_result() {
    let result = ToolResult::error("Failed");
    let response = ToolResponse::new("call_id".to_string(), result);

    assert!(response.result().is_error());
}

// ============================================================================
// FunctionDeclaration Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_declaration_new() {
    let schema = json!({
        "type": "object",
        "properties": {
            "input": {"type": "string"}
        }
    });

    let decl = FunctionDeclaration::new("my_func", "Does something", schema.clone());

    assert_eq!(decl.name(), "my_func");
    assert_eq!(decl.description(), "Does something");
    assert_eq!(decl.parameters(), &schema);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_declaration_empty_schema() {
    let decl = FunctionDeclaration::new("func", "desc", json!({}));

    assert_eq!(decl.parameters(), &json!({}));
}

// ============================================================================
// FunctionTool Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_new() {
    let tool = FunctionTool::new("test_tool", "A test tool", |_args, _ctx| {
        Box::pin(async { ToolResult::success(json!({"result": "ok"})) })
    });

    assert_eq!(tool.name(), "test_tool");
    assert_eq!(tool.description(), "A test tool");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_with_parameters_schema() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}
        }
    });

    let tool = FunctionTool::new("tool", "desc", |_args, _ctx| {
        Box::pin(async { ToolResult::success(json!({})) })
    })
    .with_parameters_schema(schema.clone());

    assert_eq!(tool.parameters_schema(), &schema);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_declaration() {
    let schema = json!({"type": "object"});
    let tool = FunctionTool::new("my_tool", "My description", |_args, _ctx| {
        Box::pin(async { ToolResult::success(json!({})) })
    })
    .with_parameters_schema(schema.clone());

    let decl = tool.declaration();
    assert_eq!(decl.name(), "my_tool");
    assert_eq!(decl.description(), "My description");
    assert_eq!(decl.parameters(), &schema);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_run_async() {
    let tool = FunctionTool::new("calculator", "Adds two numbers", |args, _ctx| {
        Box::pin(async move {
            let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
            let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
            ToolResult::success(json!({"sum": a + b}))
        })
    });

    let mut args = HashMap::new();
    args.insert("a".to_string(), json!(5));
    args.insert("b".to_string(), json!(3));

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let result = tool.run_async(args, &ctx).await;

    assert!(result.is_success());
    assert_eq!(result.data(), &json!({"sum": 8}));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_error_handling() {
    let tool = FunctionTool::new("faulty_tool", "Always fails", |_args, _ctx| {
        Box::pin(async { ToolResult::error("Tool failed") })
    });

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let result = tool.run_async(HashMap::new(), &ctx).await;

    assert!(result.is_error());
    assert_eq!(result.error_message(), Some("Tool failed"));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_function_tool_access_context() {
    let tool = FunctionTool::new("context_tool", "Uses context", |_args, ctx| {
        Box::pin(async move {
            // Store something in context
            ctx.state().set_state("key", json!("value"));

            // Retrieve it
            let value = ctx.state().get_state("key");
            ToolResult::success(json!({"found": value.is_some()}))
        })
    });

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let result = tool.run_async(HashMap::new(), &ctx).await;

    assert!(result.is_success());
    assert_eq!(result.data()["found"], json!(true));
}

// ============================================================================
// SimpleToolset Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_new() {
    let tool1 = Arc::new(common::MockTool::new("tool1", "First tool")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "Second tool")) as Arc<dyn BaseTool>;

    let toolset = SimpleToolset::new(vec![tool1, tool2]);
    let tools = toolset.get_tools().await;

    assert_eq!(tools.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_empty() {
    let toolset = SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new());
    let tools = toolset.get_tools().await;

    assert_eq!(tools.len(), 0);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_add_tool() {
    let mut toolset = SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new());

    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;
    toolset.add_tool(tool);

    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_add_tools() {
    let mut toolset = SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new());

    let tool1 = Arc::new(common::MockTool::new("tool1", "desc")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "desc")) as Arc<dyn BaseTool>;

    toolset.add_tools(vec![tool1, tool2]);

    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_with_tool() {
    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;

    let toolset = SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new()).with_tool(tool);

    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_with_tools() {
    let tool1 = Arc::new(common::MockTool::new("tool1", "desc")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "desc")) as Arc<dyn BaseTool>;

    let toolset =
        SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new()).with_tools(vec![tool1, tool2]);

    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_default() {
    let toolset = SimpleToolset::default();
    let tools = toolset.get_tools().await;

    assert_eq!(tools.len(), 0);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_simple_toolset_close() {
    let toolset = SimpleToolset::new(Vec::<Arc<dyn BaseTool>>::new());
    toolset.close().await; // Should not panic
}

// ============================================================================
// CombinedToolset Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_combined_toolset_new() {
    let tool1 = Arc::new(common::MockTool::new("tool1", "desc")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "desc")) as Arc<dyn BaseTool>;

    let toolset1 = Arc::new(SimpleToolset::new(vec![tool1])) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::new(vec![tool2])) as Arc<dyn BaseToolset>;

    let combined = CombinedToolset::new(toolset1, toolset2);
    let tools = combined.get_tools().await;

    assert_eq!(tools.len(), 2);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_combined_toolset_empty() {
    let toolset1 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;

    let combined = CombinedToolset::new(toolset1, toolset2);
    let tools = combined.get_tools().await;

    assert_eq!(tools.len(), 0);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_combined_toolset_one_empty() {
    let tool = Arc::new(common::MockTool::new("tool", "desc")) as Arc<dyn BaseTool>;

    let toolset1 = Arc::new(SimpleToolset::new(vec![tool])) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;

    let combined = CombinedToolset::new(toolset1, toolset2);
    let tools = combined.get_tools().await;

    assert_eq!(tools.len(), 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_combined_toolset_close() {
    let toolset1 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::default()) as Arc<dyn BaseToolset>;

    let combined = CombinedToolset::new(toolset1, toolset2);
    combined.close().await; // Should not panic
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_combined_toolset_multiple_layers() {
    let tool1 = Arc::new(common::MockTool::new("tool1", "desc")) as Arc<dyn BaseTool>;
    let tool2 = Arc::new(common::MockTool::new("tool2", "desc")) as Arc<dyn BaseTool>;
    let tool3 = Arc::new(common::MockTool::new("tool3", "desc")) as Arc<dyn BaseTool>;

    let toolset1 = Arc::new(SimpleToolset::new(vec![tool1])) as Arc<dyn BaseToolset>;
    let toolset2 = Arc::new(SimpleToolset::new(vec![tool2])) as Arc<dyn BaseToolset>;
    let toolset3 = Arc::new(SimpleToolset::new(vec![tool3])) as Arc<dyn BaseToolset>;

    let combined1 = Arc::new(CombinedToolset::new(toolset1, toolset2)) as Arc<dyn BaseToolset>;
    let combined2 = CombinedToolset::new(combined1, toolset3);

    let tools = combined2.get_tools().await;
    assert_eq!(tools.len(), 3);
}

// ============================================================================
// ExecutionState Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_execution_state_new() {
    let state = DefaultExecutionState::new();
    assert!(state.get_state("nonexistent").is_none());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_execution_state_set_and_get() {
    let state = DefaultExecutionState::new();

    state.set_state("key1", json!("value1"));
    state.set_state("key2", json!(42));

    assert_eq!(state.get_state("key1"), Some(json!("value1")));
    assert_eq!(state.get_state("key2"), Some(json!(42)));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_execution_state_overwrite() {
    let state = DefaultExecutionState::new();

    state.set_state("key", json!("first"));
    state.set_state("key", json!("second"));

    assert_eq!(state.get_state("key"), Some(json!("second")));
}

async fn test_execution_state_complex_values() {
    let state = DefaultExecutionState::new();

    let complex_value = json!({
        "nested": {
            "array": [1, 2, 3],
            "object": {"key": "value"}
        }
    });

    state.set_state("complex", complex_value.clone());

    assert_eq!(state.get_state("complex"), Some(complex_value));
}

// ============================================================================
// ToolContext Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_context_builder() {
    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build();

    assert!(ctx.is_ok());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_context_builder_without_state() {
    let result = ToolContext::builder().build();
    assert!(result.is_err());
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_context_state_access() {
    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    ctx.state().set_state("test_key", json!("test_value"));

    assert_eq!(ctx.state().get_state("test_key"), Some(json!("test_value")));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_context_state_isolation() {
    let state1 = DefaultExecutionState::new();
    let state2 = DefaultExecutionState::new();

    let ctx1 = ToolContext::builder().with_state(&state1).build().unwrap();
    let ctx2 = ToolContext::builder().with_state(&state2).build().unwrap();

    ctx1.state().set_state("key", json!("value1"));
    ctx2.state().set_state("key", json!("value2"));

    assert_eq!(ctx1.state().get_state("key"), Some(json!("value1")));
    assert_eq!(ctx2.state().get_state("key"), Some(json!("value2")));
}

// ============================================================================
// BaseTool Tests (using mock)
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_name() {
    let tool = common::MockTool::new("my_tool", "description");
    assert_eq!(tool.name(), "my_tool");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_description() {
    let tool = common::MockTool::new("tool", "A useful tool");
    assert_eq!(tool.description(), "A useful tool");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_declaration() {
    let tool = common::MockTool::new("test_tool", "Test description");
    let decl = tool.declaration();

    assert_eq!(decl.name(), "test_tool");
    assert_eq!(decl.description(), "Test description");
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_run_async() {
    let tool = common::MockTool::new("tool", "desc");

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let result = tool.run_async(HashMap::new(), &ctx).await;

    assert!(result.is_success());
    assert_eq!(tool.call_count().await, 1);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_run_async_multiple_times() {
    let tool = common::MockTool::new("tool", "desc");

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    tool.run_async(HashMap::new(), &ctx).await;
    tool.run_async(HashMap::new(), &ctx).await;
    tool.run_async(HashMap::new(), &ctx).await;

    assert_eq!(tool.call_count().await, 3);
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_base_tool_custom_response() {
    let custom_response = json!({"custom": "data", "count": 42});
    let tool = common::MockTool::new("tool", "desc").with_response(custom_response.clone());

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let result = tool.run_async(HashMap::new(), &ctx).await;

    assert_eq!(result.data(), &custom_response);
}

// ============================================================================
// Integration Tests
// ============================================================================

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_tool_execution_with_state() {
    let tool = FunctionTool::new("counter", "Increments a counter", |_args, ctx| {
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
    });

    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    // Call multiple times
    let result1 = tool.run_async(HashMap::new(), &ctx).await;
    assert_eq!(result1.data()["count"], json!(1));

    let result2 = tool.run_async(HashMap::new(), &ctx).await;
    assert_eq!(result2.data()["count"], json!(2));

    let result3 = tool.run_async(HashMap::new(), &ctx).await;
    assert_eq!(result3.data()["count"], json!(3));
}

#[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), tokio::test)]
#[cfg_attr(
    all(target_os = "wasi", target_env = "p1"),
    wasm_bindgen_test::wasm_bindgen_test
)]
async fn test_toolset_with_multiple_tools() {
    let tool1 = Arc::new(FunctionTool::new("add", "Adds numbers", |args, _ctx| {
        Box::pin(async move {
            let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
            let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
            ToolResult::success(json!({"result": a + b}))
        })
    })) as Arc<dyn BaseTool>;

    let tool2 = Arc::new(FunctionTool::new(
        "multiply",
        "Multiplies numbers",
        |args, _ctx| {
            Box::pin(async move {
                let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(1);
                let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(1);
                ToolResult::success(json!({"result": a * b}))
            })
        },
    )) as Arc<dyn BaseTool>;

    let toolset = SimpleToolset::new(vec![tool1, tool2]);
    let tools = toolset.get_tools().await;

    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name(), "add");
    assert_eq!(tools[1].name(), "multiply");

    // Test execution
    let state = DefaultExecutionState::new();
    let ctx = ToolContext::builder().with_state(&state).build().unwrap();

    let mut args = HashMap::new();
    args.insert("a".to_string(), json!(5));
    args.insert("b".to_string(), json!(3));

    let result = tools[0].run_async(args.clone(), &ctx).await;
    assert_eq!(result.data()["result"], json!(8));

    let result = tools[1].run_async(args, &ctx).await;
    assert_eq!(result.data()["result"], json!(15));
}
