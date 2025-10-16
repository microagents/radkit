use futures::StreamExt;
use radkit::models::content::Content;
use radkit::models::llm_request::{GenerateContentConfig, LlmRequest};
use radkit::models::{BaseLlm, OpenAILlm};
use std::sync::Arc;

mod common;
use common::{get_openai_key, init_test_env};

/// Test basic streaming functionality
///
/// Run with: `cargo test test_openai_streaming_basic --ignored` (requires OPENAI_API_KEY)
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_streaming_basic() {
    init_test_env();

    let api_key = match get_openai_key() {
        Some(key) => key,
        None => {
            println!("Skipping OpenAI streaming test - no API key");
            return;
        }
    };

    let llm = OpenAILlm::new("gpt-4o-mini".to_string(), api_key);

    // Create a simple request
    let mut content = Content::new(
        "test_task".to_string(),
        "test_context".to_string(),
        uuid::Uuid::new_v4().to_string(),
        radkit::a2a::MessageRole::User,
    );
    content.add_text(
        "Write a short story about a robot learning to paint.".to_string(),
        None,
    );

    let request = LlmRequest {
        messages: vec![content],
        current_task_id: "test_task".to_string(),
        context_id: "test_context".to_string(),
        system_instruction: Some("You are a creative writer.".to_string()),
        config: GenerateContentConfig::default(),
        toolset: None,
        metadata: std::collections::HashMap::new(),
    };

    let stream = llm.generate_content_stream(request, None).await.unwrap();
    let mut stream = Box::pin(stream);

    let mut response_parts = Vec::new();
    let mut final_received = false;

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                println!(
                    "Streaming response: partial={}, turn_complete={}",
                    response.streaming_info.partial, response.streaming_info.turn_complete
                );

                // Collect text parts
                for part in &response.message.parts {
                    if let radkit::models::content::ContentPart::Text { text, .. } = part {
                        if !text.is_empty() {
                            response_parts.push(text.clone());
                            print!("{}", text);
                        }
                    }
                }

                // Check if this is the final response
                if response.streaming_info.turn_complete {
                    final_received = true;
                    break;
                }
            }
            Err(e) => {
                panic!("Streaming error: {}", e);
            }
        }
    }

    println!();
    assert!(final_received, "Should receive final response");
    assert!(
        !response_parts.is_empty(),
        "Should receive some response text"
    );

    let full_response = response_parts.join("");
    assert!(full_response.len() > 50, "Response should be substantial");
    println!("Full response length: {} characters", full_response.len());
}

/// Test streaming with function calls
///
/// Run with: `cargo test test_openai_streaming_with_tools --ignored` (requires OPENAI_API_KEY)
#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_streaming_with_tools() {
    init_test_env();

    let api_key = match get_openai_key() {
        Some(key) => key,
        None => {
            println!("Skipping OpenAI streaming tools test - no API key");
            return;
        }
    };

    let llm = OpenAILlm::new("gpt-4o".to_string(), api_key);

    // Create a toolset with a simple function
    use radkit::tools::{FunctionTool, ToolResult};
    use serde_json::{Value, json};
    use std::collections::HashMap;

    let calculator_tool = FunctionTool::new(
        "calculate".to_string(),
        "Perform mathematical calculations".to_string(),
        |args: HashMap<String, Value>, _context| {
            Box::pin(async move {
                let expression = args
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let result = match expression {
                    "2+2" => 4,
                    "10*5" => 50,
                    "25/5" => 5,
                    _ => 42, // Default result
                };

                ToolResult {
                    success: true,
                    data: json!({ "result": result }),
                    error_message: None,
                }
            })
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "expression": {
                "type": "string",
                "description": "Mathematical expression to evaluate"
            }
        },
        "required": ["expression"],
        "additionalProperties": false
    }));

    let toolset = radkit::tools::SimpleToolset::new(vec![Arc::new(calculator_tool)]);

    // Create a request that should trigger tool usage
    let mut content = Content::new(
        "test_task".to_string(),
        "test_context".to_string(),
        uuid::Uuid::new_v4().to_string(),
        radkit::a2a::MessageRole::User,
    );
    content.add_text(
        "Please use the calculate tool to compute 2+2.".to_string(),
        None,
    );

    let request = LlmRequest {
        messages: vec![content],
        current_task_id: "test_task".to_string(),
        context_id: "test_context".to_string(),
        system_instruction: Some(
            "You are a helpful assistant. Use the available tools when asked.".to_string(),
        ),
        config: GenerateContentConfig::default(),
        toolset: Some(Arc::new(toolset)),
        metadata: std::collections::HashMap::new(),
    };

    // Test streaming with tools
    let stream = llm.generate_content_stream(request, None).await.unwrap();
    let mut stream = Box::pin(stream);

    let mut has_function_call = false;
    let mut has_text = false;
    let mut final_received = false;

    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                println!(
                    "Streaming response: partial={}, turn_complete={}",
                    response.streaming_info.partial, response.streaming_info.turn_complete
                );

                // Check for function calls and text
                for part in &response.message.parts {
                    match part {
                        radkit::models::content::ContentPart::Text { text, .. } => {
                            if !text.is_empty() {
                                has_text = true;
                                print!("{}", text);
                            }
                        }
                        radkit::models::content::ContentPart::FunctionCall {
                            name,
                            arguments,
                            ..
                        } => {
                            has_function_call = true;
                            println!("Function call: {} with args: {:?}", name, arguments);
                        }
                        _ => {}
                    }
                }

                // Check if this is the final response
                if response.streaming_info.turn_complete {
                    final_received = true;
                    break;
                }
            }
            Err(e) => {
                panic!("Streaming error: {}", e);
            }
        }
    }

    println!();
    assert!(final_received, "Should receive final response");
    // Note: Function calls might appear in streaming, but it depends on how OpenAI structures the response
    println!(
        "Has function call: {}, Has text: {}",
        has_function_call, has_text
    );
}

/// Test streaming error handling
#[tokio::test]
async fn test_openai_streaming_error_handling() {
    use radkit::config::EnvKey;

    // Set up a temporary invalid key for testing error handling
    unsafe {
        std::env::set_var("TEST_INVALID_OPENAI_KEY", "invalid_key");
    }
    let llm = OpenAILlm::new("gpt-4o".to_string(), EnvKey::new("TEST_INVALID_OPENAI_KEY"));

    // Create a simple request
    let mut content = Content::new(
        "test_task".to_string(),
        "test_context".to_string(),
        uuid::Uuid::new_v4().to_string(),
        radkit::a2a::MessageRole::User,
    );
    content.add_text("Hello".to_string(), None);

    let request = LlmRequest {
        messages: vec![content],
        current_task_id: "test_task".to_string(),
        context_id: "test_context".to_string(),
        system_instruction: None,
        config: GenerateContentConfig::default(),
        toolset: None,
        metadata: std::collections::HashMap::new(),
    };

    // Test that streaming returns an error for invalid API key
    let result = llm.generate_content_stream(request, None).await;
    assert!(result.is_err(), "Should return error for invalid API key");

    if let Err(error) = result {
        println!("Expected error: {}", error);
        assert!(
            error.to_string().contains("OpenAI"),
            "Error should mention OpenAI"
        );
    }
}

/// Test that supports_streaming returns true
#[test]
fn test_openai_supports_streaming() {
    use radkit::config::EnvKey;

    unsafe {
        std::env::set_var("TEST_OPENAI_KEY", "test_key");
    }
    let llm = OpenAILlm::new("gpt-4o".to_string(), EnvKey::new("TEST_OPENAI_KEY"));
    assert!(llm.supports_streaming(), "OpenAI should support streaming");
}

/// Test streaming capabilities
#[test]
fn test_openai_streaming_capabilities() {
    let llm = OpenAILlm::new("gpt-4o".to_string(), "test_key".into());

    let capabilities = llm.get_capabilities();
    assert!(
        capabilities.max_context_length.is_some(),
        "Should have context length limit"
    );
    assert!(
        capabilities.supports_json_schema,
        "Should support JSON schema"
    );
    assert!(
        capabilities.supports_system_instructions,
        "Should support system instructions"
    );

    // Verify different model capabilities
    let mini_llm = OpenAILlm::new("gpt-4o-mini".to_string(), "test_key".into());
    let mini_caps = mini_llm.get_capabilities();

    let turbo_llm = OpenAILlm::new("gpt-4-turbo".to_string(), "test_key".into());
    let turbo_caps = turbo_llm.get_capabilities();

    // Both should support streaming implicitly
    println!(
        "GPT-4o context length: {:?}",
        capabilities.max_context_length
    );
    println!(
        "GPT-4o-mini context length: {:?}",
        mini_caps.max_context_length
    );
    println!(
        "GPT-4-turbo context length: {:?}",
        turbo_caps.max_context_length
    );
}
