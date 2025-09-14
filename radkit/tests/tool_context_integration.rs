//! Integration tests for ToolContext with real LLM API calls
//!
//! These tests validate that the ToolContext system works correctly with actual
//! LLM providers and that state changes are properly reflected in events.

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult, TaskState,
};
use radkit::agents::{Agent, AgentConfig};
use radkit::models::{AnthropicLlm, GeminiLlm};
use radkit::sessions::{SessionEventType, StateScope};
use radkit::tools::{
    FunctionTool, ToolResult, ToolStateAccess, ToolTaskAccess, ToolUserInteraction,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

mod common;
use common::{get_anthropic_key, get_gemini_key};

/// Create a state management tool that uses the new ToolContext API
fn create_state_management_tool() -> Arc<dyn radkit::tools::BaseTool> {
    Arc::new(FunctionTool::new(
        "manage_preferences".to_string(),
        "Get or set user preferences and session data using the secure ToolContext API".to_string(),
        |args, context| Box::pin(async move {
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("get");
            match action {
                "set_user_pref" => {
                    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("default");
                    let value = args.get("value").cloned().unwrap_or(json!("default"));
                    match context.set_user_state(key.to_string(), value.clone()).await {
                        Ok(()) => ToolResult::success(json!({
                            "action": "set_user_pref",
                            "key": key,
                            "value": value,
                            "message": "User preference set successfully"
                        })),
                        Err(e) => ToolResult::error(format!("Failed to set user preference: {}", e))
                    }
                }
                "set_session_data" => {
                    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("default");
                    let value = args.get("value").cloned().unwrap_or(json!("default"));
                    match context.set_session_state(key.to_string(), value.clone()).await {
                        Ok(()) => ToolResult::success(json!({
                            "action": "set_session_data",
                            "key": key,
                            "value": value,
                            "message": "Session data set successfully"
                        })),
                        Err(e) => ToolResult::error(format!("Failed to set session data: {}", e))
                    }
                }
                "get_all_state" => {
                    let app_data = context.get_app_state("config").await.ok().flatten();
                    let user_theme = context.get_user_state("theme").await.ok().flatten();
                    let user_language = context.get_user_state("language").await.ok().flatten();
                    let session_progress = context.get_session_state("progress").await.ok().flatten();
                    let session_temp = context.get_session_state("temp_data").await.ok().flatten();
                    ToolResult::success(json!({
                        "action": "get_all_state",
                        "app_state": {
                            "config": app_data
                        },
                        "user_state": {
                            "theme": user_theme,
                            "language": user_language
                        },
                        "session_state": {
                            "progress": session_progress,
                            "temp_data": session_temp
                        }
                    }))
                }
                "update_task_progress" => {
                    let progress = args.get("progress").and_then(|v| v.as_u64()).unwrap_or(50);
                    let message = format!("Task is {}% complete", progress);
                    let task_state = if progress >= 100 {
                        TaskState::Completed
                    } else {
                        TaskState::Working
                    };
                    match context.update_task_status(task_state.clone(), Some(message.clone())).await {
                        Ok(()) => {
                            // Also save progress in session state
                            let _ = context.set_session_state("progress".to_string(), json!(progress)).await;
                            ToolResult::success(json!({
                                "action": "update_task_progress",
                                "progress": progress,
                                "status_message": message,
                                "task_status": format!("{:?}", task_state).to_lowercase()
                            }))
                        }
                        Err(e) => ToolResult::error(format!("Failed to update task progress: {}", e))
                    }
                }
                "complete_task" => {
                    let final_message = args.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Task completed successfully");
                    match context.update_task_status(TaskState::Completed, Some(final_message.to_string())).await {
                        Ok(()) => {
                            ToolResult::success(json!({
                                "action": "complete_task",
                                "message": final_message,
                                "task_status": "completed"
                            }))
                        }
                        Err(e) => ToolResult::error(format!("Failed to complete task: {}", e))
                    }
                }
                _ => {
                    ToolResult::error(format!("Unknown action: {}", action))
                }
            }
        }),
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["set_user_pref", "set_session_data", "get_all_state", "update_task_progress", "complete_task"],
                "description": "Action to perform with state management"
            },
            "key": {
                "type": "string",
                "description": "State key (for set actions)"
            },
            "value": {
                "description": "State value (for set actions, can be any JSON type)"
            },
            "progress": {
                "type": "integer",
                "minimum": 0,
                "maximum": 100,
                "description": "Progress percentage (for update_task_progress)"
            },
            "message": {
                "type": "string",
                "description": "Completion message (for complete_task)"
            }
        },
        "required": ["action"]
    })))
}

/// Create an interactive tool that uses add_user_input
fn create_interactive_tool() -> Arc<dyn radkit::tools::BaseTool> {
    Arc::new(
        FunctionTool::new(
            "request_user_input".to_string(),
            "Request additional input from the user during task execution".to_string(),
            |args, context| {
                Box::pin(async move {
                    let prompt = args
                        .get("prompt")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Please provide additional information:");
                    let input_type = args
                        .get("input_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("text");

                    // Create user input content directly
                    let mut content = radkit::models::content::Content::new(
                        context.task_id().to_string(),
                        context.context_id().to_string(),
                        Uuid::new_v4().to_string(),
                        MessageRole::User,
                    );

                    content.add_text(format!("[TOOL GENERATED] {}", prompt), {
                        let mut map = HashMap::new();
                        map.insert("tool_generated".to_string(), json!(true));
                        map.insert("interaction_type".to_string(), json!(input_type));
                        map.insert("awaiting_response".to_string(), json!(true));
                        Some(map)
                    });

                    match context.add_user_input(content).await {
                        Ok(()) => ToolResult::success(json!({
                            "action": "request_user_input",
                            "prompt": prompt,
                            "input_type": input_type,
                            "message": "User input request sent successfully"
                        })),
                        Err(e) => ToolResult::error(format!("Failed to request user input: {}", e)),
                    }
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The prompt to show the user"
                },
                "input_type": {
                    "type": "string",
                    "enum": ["text", "confirmation", "choice"],
                    "description": "Type of input expected from user"
                }
            },
            "required": ["prompt"]
        })),
    )
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY environment variable"]
async fn test_anthropic_tool_context_state_management() {
    println!("🧪 Testing ToolContext state management with Anthropic API...");

    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⚠️ Skipping test: ANTHROPIC_API_KEY not found in .env file");
            return;
        }
    };

    // Create LLM and agent with state management tool
    let llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);

    let state_tool = create_state_management_tool();
    let interactive_tool = create_interactive_tool();

    let agent = Agent::builder(
        r#"You are testing the ToolContext system. Use the manage_preferences tool to:
        1. Set user preference theme=dark
        2. Set user preference language=english
        3. Set session data temp_data="test_session_data"
        4. Update task progress to 75%
        5. Get all state to verify everything was saved
        6. Complete the task using complete_task action

        Be systematic and report what you're doing at each step."#,
        llm,
    )
    .with_card(|c| {
        c.with_name("tool_context_tester")
            .with_description("ToolContext Integration Tester")
    })
    .with_config(AgentConfig::default().with_max_iterations(15))
    .with_tools(vec![state_tool, interactive_tool])
    .build();

    // Create message requesting state management operations
    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please test the ToolContext state management system. Set user preferences, session data, update task progress, then retrieve all state to verify everything works.".to_string(),
            metadata: None,
        }],
        context_id: None, // New session
        task_id: None,    // New task
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Execute with streaming to capture events
    let mut execution = agent
        .send_streaming_message(
            "tool_context_test".to_string(),
            "test_user".to_string(),
            params,
        )
        .await
        .expect("Agent execution should succeed");

    let mut final_task = None;
    let mut status_updates = 0;
    let mut messages_received = 0;
    let mut state_changes_detected = Vec::new();

    // Process A2A streaming events
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::Message(msg) => {
                messages_received += 1;
                println!(
                    "💬 Agent Message #{}: role={:?}",
                    messages_received, msg.role
                );
                if let Some(Part::Text { text, .. }) = msg.parts.first() {
                    let preview = if text.len() > 100 {
                        format!("{}...", &text[..100])
                    } else {
                        text.clone()
                    };
                    println!("   Content: {}", preview);
                }
            }
            SendStreamingMessageResult::TaskStatusUpdate(update) => {
                status_updates += 1;
                println!(
                    "📊 Status Update #{}: {:?}",
                    status_updates, update.status.state
                );
                if let Some(msg) = &update.status.message {
                    for part in &msg.parts {
                        if let Part::Text { text, .. } = part {
                            println!("   Message: {}", text);
                        }
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Task Completed: {}", task.id);
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    // Verify we got a final task
    let task = final_task.expect("Should receive final task");

    // Access session service to examine events and state
    let session_service = agent.session_service();
    let session = session_service
        .get_session("tool_context_test", "test_user", &task.context_id)
        .await
        .expect("Should get session")
        .expect("Session should exist");

    println!("🔍 Analyzing session events and state...");
    println!("  Total events: {}", session.events.len());

    // Count different types of events
    let mut tool_calls = 0;
    let mut tool_responses = 0;
    let mut state_changes = 0;
    let mut task_status_changes = 0;
    let mut user_messages = 0;
    let mut agent_messages = 0;

    // Track specific state changes
    let mut user_state_changes = Vec::new();
    let mut session_state_changes = Vec::new();

    for (i, event) in session.events.iter().enumerate() {
        match &event.event_type {
            SessionEventType::UserMessage { content } => {
                user_messages += 1;
                println!(
                    "  📨 Event #{}: UserMessage with {} parts",
                    i,
                    content.parts.len()
                );

                // Look for tool calls and responses
                for (j, part) in content.parts.iter().enumerate() {
                    println!("    Part #{}: {:?}", j, std::mem::discriminant(part));
                    match part {
                        radkit::models::content::ContentPart::FunctionCall {
                            name,
                            arguments,
                            ..
                        } => {
                            tool_calls += 1;
                            println!("  🔧 Tool Call: {} with args: {:?}", name, arguments);
                        }
                        radkit::models::content::ContentPart::FunctionResponse {
                            name,
                            success,
                            result,
                            error_message,
                            ..
                        } => {
                            tool_responses += 1;
                            println!("  ⚙️ Tool Response: {} (success: {})", name, success);
                            if *success {
                                if let Some(action) = result.get("action").and_then(|v| v.as_str())
                                {
                                    println!("     Action: {}", action);
                                }
                                if let Some(message) =
                                    result.get("message").and_then(|v| v.as_str())
                                {
                                    println!("     Message: {}", message);
                                }
                            } else if let Some(error) = error_message {
                                println!("     Error: {}", error);
                            }
                        }
                        radkit::models::content::ContentPart::Text { text, .. } => {
                            let preview = if text.len() > 100 {
                                format!("{}...", &text[..100])
                            } else {
                                text.clone()
                            };
                            println!("    Text part: {}", preview);
                        }
                        _ => {
                            println!("    Other part type");
                        }
                    }
                }
            }
            SessionEventType::AgentMessage { content } => {
                agent_messages += 1;

                // Look for function calls in AgentMessage events (LLM requesting tool use)
                for part in content.parts.iter() {
                    if let radkit::models::content::ContentPart::FunctionCall {
                        name,
                        arguments,
                        ..
                    } = part
                    {
                        tool_calls += 1;
                        println!("  🔧 Tool Call: {} with args: {:?}", name, arguments);
                    }
                }
            }
            SessionEventType::StateChanged {
                scope,
                key,
                old_value,
                new_value,
            } => {
                state_changes += 1;
                println!(
                    "  📝 State Change: {:?} {} = {} (was: {})",
                    scope,
                    key,
                    new_value,
                    old_value.as_ref().unwrap_or(&json!(null))
                );

                match scope {
                    StateScope::User => user_state_changes.push((key.clone(), new_value.clone())),
                    StateScope::Session => {
                        session_state_changes.push((key.clone(), new_value.clone()))
                    }
                    _ => {}
                }

                state_changes_detected.push(format!("{:?}:{}", scope, key));
            }
            SessionEventType::TaskStatusChanged {
                old_state,
                new_state,
                ..
            } => {
                task_status_changes += 1;
                println!("  📊 Task Status: {:?} → {:?}", old_state, new_state);
            }
            _ => {}
        }
    }

    println!("📈 Event Summary:");
    println!("  User messages: {}", user_messages);
    println!("  Agent messages: {}", agent_messages);
    println!("  Tool calls: {}", tool_calls);
    println!("  Tool responses: {}", tool_responses);
    println!("  State changes: {}", state_changes);
    println!("  Task status changes: {}", task_status_changes);

    // Verify specific expectations - focus on functionality, not tool call mechanism
    assert!(state_changes > 0, "Should have state changes");
    assert!(task_status_changes > 0, "Should have task status changes");

    // If tool calls were made, verify they match responses
    if tool_calls > 0 {
        assert!(tool_responses > 0, "Should have tool responses");
        assert_eq!(
            tool_calls, tool_responses,
            "Tool calls and responses should match"
        );
    }

    // Verify specific state changes occurred
    println!("🔍 Verifying expected state changes...");

    // Check user state changes
    let user_theme = user_state_changes.iter().find(|(k, _)| k == "theme");
    let user_language = user_state_changes.iter().find(|(k, _)| k == "language");

    if let Some((_, value)) = user_theme {
        println!("✅ User theme set to: {}", value);
        assert_eq!(value, &json!("dark"), "Theme should be set to dark");
    } else {
        println!("⚠️ User theme not found in state changes");
    }

    if let Some((_, value)) = user_language {
        println!("✅ User language set to: {}", value);
        assert_eq!(
            value,
            &json!("english"),
            "Language should be set to english"
        );
    } else {
        println!("⚠️ User language not found in state changes");
    }

    // Check session state changes
    let session_temp = session_state_changes.iter().find(|(k, _)| k == "temp_data");
    let session_progress = session_state_changes.iter().find(|(k, _)| k == "progress");

    if let Some((_, value)) = session_temp {
        println!("✅ Session temp_data set to: {}", value);
    } else {
        println!("⚠️ Session temp_data not found in state changes");
    }

    if let Some((_, value)) = session_progress {
        println!("✅ Session progress set to: {}", value);
    } else {
        println!("⚠️ Session progress not found in state changes");
    }

    // Verify final session state includes our changes
    println!("🔍 Verifying final session state...");
    let user_theme_final = session.get_state("user:theme");
    let user_language_final = session.get_state("user:language");
    let session_temp_final = session.get_state("temp_data");
    let session_progress_final = session.get_state("progress");

    if let Some(theme) = user_theme_final {
        println!("✅ Final user theme: {}", theme);
        assert_eq!(theme, &json!("dark"));
    }

    if let Some(language) = user_language_final {
        println!("✅ Final user language: {}", language);
        assert_eq!(language, &json!("english"));
    }

    if let Some(temp_data) = session_temp_final {
        println!("✅ Final session temp_data: {}", temp_data);
    }

    if let Some(progress) = session_progress_final {
        println!("✅ Final session progress: {}", progress);
    }

    println!("✅ ToolContext state management integration test completed successfully!");

    // Verify task completed successfully
    assert!(
        matches!(task.status.state, TaskState::Completed | TaskState::Working),
        "Task should be completed or still working"
    );
    assert!(
        !task.history.is_empty(),
        "Task should have conversation history"
    );
}

#[tokio::test]
#[ignore = "requires GEMINI_API_KEY environment variable"]
async fn test_gemini_tool_context_user_interaction() {
    println!("🧪 Testing ToolContext user interaction with Gemini API...");

    let api_key = match get_gemini_key() {
        Some(key) => key,
        None => {
            println!("⚠️ Skipping test: GEMINI_API_KEY not found in .env file");
            return;
        }
    };

    // Create LLM and agent with interactive tool
    let llm = GeminiLlm::new("gemini-1.5-flash".to_string(), api_key);

    let interactive_tool = create_interactive_tool();
    let state_tool = create_state_management_tool();

    let agent = Agent::builder(
        r#"You are testing the interactive capabilities of ToolContext.
        Use the request_user_input tool to ask the user for their name, then use
        manage_preferences to save it as a user preference. After that, use
        update_status to mark the task as completed."#,
        llm,
    )
    .with_card(|c| {
        c.with_name("interactive_tester")
            .with_description("Interactive Tool Tester")
    })
    .with_config(AgentConfig::default().with_max_iterations(10))
    .with_tools(vec![interactive_tool, state_tool])
    .with_builtin_task_tools()
    .build();

    // Create message requesting interactive workflow
    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please use the interactive tool to ask for my name, then save it as a user preference.".to_string(),
            metadata: None,
        }],
        context_id: None, // New session
        task_id: None,    // New task
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    // Execute with streaming
    let mut execution = agent
        .send_streaming_message(
            "interactive_test".to_string(),
            "test_user_interactive".to_string(),
            params,
        )
        .await
        .expect("Agent execution should succeed");

    let mut final_task = None;
    let mut user_input_requests = 0;
    let mut tool_generated_messages = 0;

    // Process A2A streaming events
    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::Message(msg) => {
                // Check if this is a tool-generated user message
                if msg.role == MessageRole::User {
                    for part in &msg.parts {
                        if let Part::Text { text, metadata } = part {
                            if let Some(meta) = metadata {
                                if meta
                                    .get("tool_generated")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false)
                                {
                                    tool_generated_messages += 1;
                                    user_input_requests += 1;
                                    println!("🔔 Tool-generated user input request: {}", text);

                                    if let Some(interaction_type) =
                                        meta.get("interaction_type").and_then(|v| v.as_str())
                                    {
                                        println!("   Interaction type: {}", interaction_type);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                println!("✅ Task Completed: {}", task.id);
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    // Verify we got a final task
    let task = final_task.expect("Should receive final task");

    // Access session to examine events
    let session_service = agent.session_service();
    let session = session_service
        .get_session(
            "interactive_test",
            "test_user_interactive",
            &task.context_id,
        )
        .await
        .expect("Should get session")
        .expect("Session should exist");

    println!("🔍 Analyzing interactive session events...");
    println!("  Total events: {}", session.events.len());

    // Look for user input injection events
    let mut injected_user_messages = 0;
    let mut interactive_tool_calls = 0;

    for event in &session.events {
        match &event.event_type {
            SessionEventType::UserMessage { content } => {
                // Check for tool-generated user messages
                for part in &content.parts {
                    if let radkit::models::content::ContentPart::Text { text, metadata } = part {
                        if let Some(meta) = metadata {
                            if meta
                                .get("tool_generated")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                            {
                                injected_user_messages += 1;
                                println!("  📨 Injected user message: {}", text);
                            }
                        }
                    } else if let radkit::models::content::ContentPart::FunctionCall {
                        name, ..
                    } = part
                    {
                        if name == "request_user_input" {
                            interactive_tool_calls += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    println!("📈 Interactive Event Summary:");
    println!("  User input requests detected: {}", user_input_requests);
    println!("  Tool-generated messages: {}", tool_generated_messages);
    println!("  Injected user messages: {}", injected_user_messages);
    println!("  Interactive tool calls: {}", interactive_tool_calls);

    // Verify expectations
    assert!(
        interactive_tool_calls > 0,
        "Should have called interactive tool"
    );
    assert!(
        injected_user_messages > 0,
        "Should have injected user messages"
    );

    println!("✅ ToolContext user interaction integration test completed successfully!");
}

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY environment variable"]
async fn test_tool_context_concurrent_operations() {
    println!("🧪 Testing ToolContext concurrent state operations with Anthropic API...");

    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⚠️ Skipping test: ANTHROPIC_API_KEY not found in .env file");
            return;
        }
    };

    let llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);

    let state_tool = create_state_management_tool();

    let agent = Agent::builder(
        r#"Test concurrent state operations. Use manage_preferences to:
        1. Set multiple user preferences (theme, language, timezone)
        2. Set multiple session data items (cache1, cache2, cache3)
        3. Update task progress multiple times (25%, 50%, 75%, 100%)
        4. Get all state to verify everything was saved correctly

        Work systematically and report your progress."#,
        llm,
    )
    .with_card(|c| {
        c.with_name("concurrent_tester")
            .with_description("Concurrent State Operations Tester")
    })
    .with_config(AgentConfig::default().with_max_iterations(20))
    .with_tools(vec![state_tool])
    .with_builtin_task_tools()
    .build();

    let message = Message {
        kind: "message".to_string(),
        message_id: Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please test concurrent state operations by setting multiple user preferences, session data, and updating progress multiple times.".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: vec![],
        extensions: vec![],
        metadata: None,
    };

    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let mut execution = agent
        .send_streaming_message(
            "concurrent_test".to_string(),
            "test_user_concurrent".to_string(),
            params,
        )
        .await
        .expect("Agent execution should succeed");

    let mut final_task = None;
    let mut _state_update_count = 0;

    while let Some(event) = execution.a2a_stream.next().await {
        match event {
            SendStreamingMessageResult::TaskStatusUpdate(update) => {
                if let Some(msg) = &update.status.message {
                    for part in &msg.parts {
                        if let Part::Text { text, .. } = part {
                            if text.contains("complete") || text.contains("progress") {
                                _state_update_count += 1;
                            }
                        }
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    let task = final_task.expect("Should receive final task");

    // Analyze the session for concurrent operations
    let session_service = agent.session_service();
    let session = session_service
        .get_session("concurrent_test", "test_user_concurrent", &task.context_id)
        .await
        .expect("Should get session")
        .expect("Session should exist");

    println!("🔍 Analyzing concurrent operations...");

    // Count state changes by type
    let mut user_state_operations = 0;
    let mut session_state_operations = 0;
    let mut task_progress_updates = 0;

    for event in &session.events {
        match &event.event_type {
            SessionEventType::StateChanged { scope, key, .. } => match scope {
                StateScope::User => {
                    user_state_operations += 1;
                    println!("  👤 User state: {}", key);
                }
                StateScope::Session => {
                    session_state_operations += 1;
                    println!("  💾 Session state: {}", key);
                    if key == "progress" {
                        task_progress_updates += 1;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    println!("📊 Concurrent Operations Summary:");
    println!("  User state operations: {}", user_state_operations);
    println!("  Session state operations: {}", session_state_operations);
    println!("  Task progress updates: {}", task_progress_updates);

    // Verify we had multiple concurrent operations
    assert!(
        user_state_operations >= 2,
        "Should have multiple user state operations"
    );
    assert!(
        session_state_operations >= 2,
        "Should have multiple session state operations"
    );
    assert!(
        task_progress_updates >= 2,
        "Should have multiple progress updates"
    );

    // Verify final state integrity
    let final_user_prefs = ["theme", "language", "timezone"];
    let final_session_data = ["cache1", "cache2", "cache3"];

    for pref in &final_user_prefs {
        let key = format!("user:{}", pref);
        if let Some(value) = session.get_state(&key) {
            println!("✅ Final user {}: {}", pref, value);
        }
    }

    for data in &final_session_data {
        if let Some(value) = session.get_state(data) {
            println!("✅ Final session {}: {}", data, value);
        }
    }

    println!("✅ ToolContext concurrent operations integration test completed successfully!");
}
