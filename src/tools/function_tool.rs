use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use super::base_tool::{BaseTool, FunctionDeclaration, ToolResult};
use crate::events::ExecutionContext;

/// Type alias for async function that can be used as a tool
pub type AsyncToolFunction = Box<
    dyn for<'a> Fn(
            HashMap<String, Value>,
            &'a ExecutionContext,
        ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>
        + Send
        + Sync,
>;

/// A tool that wraps a simple function.
pub struct FunctionTool {
    name: String,
    description: String,
    function: AsyncToolFunction,
    parameters_schema: Option<Value>,
    is_long_running: bool,
}

impl FunctionTool {
    /// Create a new function tool with the given name, description, and function
    pub fn new<F>(name: String, description: String, function: F) -> Self
    where
        F: for<'a> Fn(
                HashMap<String, Value>,
                &'a ExecutionContext,
            ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>
            + Send
            + Sync
            + 'static,
    {
        Self {
            name,
            description,
            function: Box::new(function),
            parameters_schema: None,
            is_long_running: false,
        }
    }

    /// Set the JSON schema for the function parameters
    pub fn with_parameters_schema(mut self, schema: Value) -> Self {
        self.parameters_schema = Some(schema);
        self
    }

    /// Mark this tool as long-running
    pub fn with_long_running(mut self, is_long_running: bool) -> Self {
        self.is_long_running = is_long_running;
        self
    }
}

#[async_trait]
impl BaseTool for FunctionTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn is_long_running(&self) -> bool {
        self.is_long_running
    }

    fn get_declaration(&self) -> Option<FunctionDeclaration> {
        Some(FunctionDeclaration {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self
                .parameters_schema
                .clone()
                .unwrap_or(Value::Object(serde_json::Map::new())),
        })
    }

    async fn run_async(
        &self,
        args: HashMap<String, Value>,
        context: &ExecutionContext,
    ) -> ToolResult {
        (self.function)(args, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::{Message, MessageRole, MessageSendParams, Part};
    use crate::events::{EventProcessor, InMemoryEventBus};
    use crate::sessions::InMemorySessionService;
    use serde_json::json;
    use std::sync::Arc;

    // Helper function to create a test ExecutionContext
    async fn create_test_context() -> ExecutionContext {
        let session_service = Arc::new(InMemorySessionService::new());
        let event_bus = Arc::new(InMemoryEventBus::new());
        let event_processor = Arc::new(EventProcessor::new(session_service, event_bus));

        let params = MessageSendParams {
            message: Message {
                kind: "message".to_string(),
                message_id: "test_msg".to_string(),
                role: MessageRole::User,
                parts: vec![Part::Text {
                    text: "test".to_string(),
                    metadata: None,
                }],
                context_id: Some("test_ctx".to_string()),
                task_id: Some("test_task".to_string()),
                reference_task_ids: vec![],
                extensions: vec![],
                metadata: None,
            },
            configuration: None,
            metadata: None,
        };

        ExecutionContext::new(
            "test_ctx".to_string(),
            "test_task".to_string(),
            "test_app".to_string(),
            "test_user".to_string(),
            params,
            event_processor,
        )
    }

    #[tokio::test]
    async fn test_function_tool_creation() {
        let tool = FunctionTool::new(
            "test_tool".to_string(),
            "A test tool".to_string(),
            |args, _context| {
                Box::pin(async move {
                    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
                    ToolResult::success(json!({ "greeting": format!("Hello, {}!", name) }))
                })
            },
        );

        assert_eq!(tool.name(), "test_tool");
        assert_eq!(tool.description(), "A test tool");
        assert!(!tool.is_long_running());
    }

    #[tokio::test]
    async fn test_function_tool_execution() {
        let tool = FunctionTool::new(
            "greet".to_string(),
            "Greets a person".to_string(),
            |args, _context| {
                Box::pin(async move {
                    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("World");
                    ToolResult::success(json!({ "greeting": format!("Hello, {}!", name) }))
                })
            },
        );

        let mut args = HashMap::new();
        args.insert("name".to_string(), json!("Alice"));

        let context = create_test_context().await;
        let result = tool.run_async(args, &context).await;

        assert!(result.success);
        assert_eq!(result.data["greeting"], "Hello, Alice!");
    }

    #[tokio::test]
    async fn test_function_tool_with_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name to greet"
                }
            },
            "required": ["name"]
        });

        let tool = FunctionTool::new(
            "greet".to_string(),
            "Greets a person".to_string(),
            |_args, _context| {
                Box::pin(async move { ToolResult::success(json!({ "greeting": "Hello!" })) })
            },
        )
        .with_parameters_schema(schema.clone());

        let declaration = tool.get_declaration().unwrap();
        assert_eq!(declaration.name, "greet");
        assert_eq!(declaration.parameters, schema);
    }

    #[tokio::test]
    async fn test_function_tool_with_long_running() {
        let long_tool = FunctionTool::new(
            "long_running_tool".to_string(),
            "A tool that takes time".to_string(),
            |_args, _ctx| {
                Box::pin(async {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    ToolResult::success(json!({"completed": "after_delay"}))
                })
            },
        )
        .with_long_running(true);

        assert!(long_tool.is_long_running());

        let context = create_test_context().await;
        let start = std::time::Instant::now();
        let result = long_tool.run_async(HashMap::new(), &context).await;
        let duration = start.elapsed();

        assert!(result.success);
        assert!(duration.as_millis() >= 50);
        assert_eq!(result.data, json!({"completed": "after_delay"}));
    }

    #[tokio::test]
    async fn test_function_tool_with_complex_parameters() {
        let complex_tool = FunctionTool::new(
            "complex_tool".to_string(),
            "A tool with complex parameters".to_string(),
            |args, _ctx| {
                Box::pin(async move {
                    let name = args
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let age = args.get("age").and_then(|v| v.as_u64()).unwrap_or(0);
                    let skills = args
                        .get("skills")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();

                    ToolResult::success(json!({
                        "greeting": format!("Hello {}, age {}", name, age),
                        "skill_count": skills.len()
                    }))
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Person's name"
                },
                "age": {
                    "type": "integer",
                    "description": "Person's age"
                },
                "skills": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "List of skills"
                }
            },
            "required": ["name"]
        }));

        let declaration = complex_tool.get_declaration().unwrap();
        assert_eq!(declaration.name, "complex_tool");
        assert!(declaration.parameters.get("properties").is_some());
        assert!(declaration.parameters.get("required").is_some());

        let mut args = HashMap::new();
        args.insert("name".to_string(), json!("Alice"));
        args.insert("age".to_string(), json!(30));
        args.insert(
            "skills".to_string(),
            json!(["rust", "python", "javascript"]),
        );

        let context = create_test_context().await;
        let result = complex_tool.run_async(args, &context).await;
        assert!(result.success);

        let data = result.data.as_object().unwrap();
        assert_eq!(
            data.get("greeting").unwrap().as_str().unwrap(),
            "Hello Alice, age 30"
        );
        assert_eq!(data.get("skill_count").unwrap().as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_function_tool_error_handling() {
        let error_tool = FunctionTool::new(
            "error_tool".to_string(),
            "A tool that can fail".to_string(),
            |args, _ctx| {
                Box::pin(async move {
                    let should_fail = args.get("fail").and_then(|v| v.as_bool()).unwrap_or(false);

                    if should_fail {
                        ToolResult::error("Intentional failure for testing".to_string())
                    } else {
                        ToolResult::success(json!({"status": "success"}))
                    }
                })
            },
        );

        let context = create_test_context().await;
        // Test success case
        let mut args = HashMap::new();
        args.insert("fail".to_string(), json!(false));
        let result = error_tool.run_async(args, &context).await;
        assert!(result.success);
        assert_eq!(result.data, json!({"status": "success"}));

        // Test error case
        let mut args = HashMap::new();
        args.insert("fail".to_string(), json!(true));
        let result = error_tool.run_async(args, &context).await;
        assert!(!result.success);
        assert_eq!(
            result.error_message,
            Some("Intentional failure for testing".to_string())
        );
    }

    #[tokio::test]
    async fn test_function_tool_with_context_usage() {
        let context_tool = FunctionTool::new(
            "context_aware_tool".to_string(),
            "A tool that uses context information".to_string(),
            |_args, ctx| {
                Box::pin(async move {
                    ToolResult::success(json!({
                        "context_id": ctx.context_id,
                        "task_id": ctx.task_id,
                        "app_name": ctx.app_name,
                        "user_id": ctx.user_id,
                        "has_metadata": ctx.current_params.metadata.is_some()
                    }))
                })
            },
        );

        let context = create_test_context().await;

        let result = context_tool.run_async(HashMap::new(), &context).await;
        assert!(result.success);

        let data = result.data.as_object().unwrap();
        assert_eq!(
            data.get("context_id").unwrap().as_str().unwrap(),
            "test_ctx"
        );
        assert_eq!(data.get("task_id").unwrap().as_str().unwrap(), "test_task");
        assert_eq!(data.get("app_name").unwrap().as_str().unwrap(), "test_app");
        assert_eq!(data.get("user_id").unwrap().as_str().unwrap(), "test_user");
        assert_eq!(data.get("has_metadata").unwrap().as_bool().unwrap(), false); // Our test context doesn't have metadata
    }

    #[tokio::test]
    async fn test_function_tool_concurrent_execution() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use tokio::task::JoinSet;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let concurrent_tool = Arc::new(FunctionTool::new(
            "concurrent_tool".to_string(),
            "A tool for concurrent testing".to_string(),
            move |args, _ctx| {
                let counter = counter_clone.clone();
                Box::pin(async move {
                    let delay = args.get("delay_ms").and_then(|v| v.as_u64()).unwrap_or(10);

                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                    let count = counter.fetch_add(1, Ordering::SeqCst);

                    ToolResult::success(json!({"execution_order": count}))
                })
            },
        ));

        let mut join_set = JoinSet::new();

        // Spawn 10 concurrent executions
        for i in 0..10 {
            let tool = concurrent_tool.clone();
            join_set.spawn(async move {
                let context = create_test_context().await;
                let mut args = HashMap::new();
                args.insert("delay_ms".to_string(), json!(5 + i)); // Stagger delays

                tool.run_async(args, &context).await
            });
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let tool_result = result.unwrap();
            assert!(tool_result.success);
            results.push(tool_result);
        }

        assert_eq!(results.len(), 10);
        assert_eq!(counter.load(Ordering::SeqCst), 10);

        // Verify all executions completed
        for result in results {
            let execution_order = result
                .data
                .get("execution_order")
                .unwrap()
                .as_u64()
                .unwrap();
            assert!(execution_order < 10);
        }
    }

    #[tokio::test]
    async fn test_function_tool_builder_pattern() {
        let tool = FunctionTool::new(
            "builder_test".to_string(),
            "Testing builder pattern".to_string(),
            |_args, _ctx| Box::pin(async { ToolResult::success(json!({"built": true})) }),
        )
        .with_parameters_schema(json!({"type": "object"}))
        .with_long_running(true);

        // Verify all builder methods were applied
        assert_eq!(tool.name(), "builder_test");
        assert_eq!(tool.description(), "Testing builder pattern");
        assert!(tool.is_long_running());

        let declaration = tool.get_declaration().unwrap();
        assert_eq!(declaration.name, "builder_test");
        assert_eq!(declaration.parameters, json!({"type": "object"}));
    }

    #[tokio::test]
    async fn test_function_tool_with_empty_parameters_schema() {
        let tool = FunctionTool::new(
            "no_params".to_string(),
            "Tool with no parameters".to_string(),
            |_args, _ctx| Box::pin(async { ToolResult::success(json!({"no_args": true})) }),
        );

        let declaration = tool.get_declaration().unwrap();
        assert_eq!(declaration.parameters, json!({})); // Should default to empty object

        let context = create_test_context().await;
        let result = tool.run_async(HashMap::new(), &context).await;
        assert!(result.success);
        assert_eq!(result.data, json!({"no_args": true}));
    }
}
