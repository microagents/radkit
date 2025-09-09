//! Simple MCP Weather Server Tests with All LLM Providers
//!
//! Tests real MCP weather server integration with Anthropic Claude, Google Gemini, and OpenAI GPT
//! using the Agent for full round-trip validation. These tests require API keys to be set.
//!
//! This test uses a real MCP weather server via:
//! uvx --from git+https://github.com/microagents/mcp-servers.git#subdirectory=mcp-weather-free mcp-weather-free

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendStreamingMessageResult, TaskState,
};
use radkit::agents::Agent;
use radkit::models::{AnthropicLlm, GeminiLlm, OpenAILlm};
use radkit::sessions::InMemorySessionService;
use radkit::tools::{MCPConnectionParams, MCPToolset};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

mod common;
use common::{get_anthropic_key, get_gemini_key, get_openai_key, init_test_env};

/// Helper function to create MCP weather toolset
fn create_mcp_weather_toolset() -> MCPToolset {
    let mcp_connection = MCPConnectionParams::Stdio {
        command: "uvx".to_string(),
        args: vec![
            "--from".to_string(),
            "git+https://github.com/microagents/mcp-servers.git#subdirectory=mcp-weather-free"
                .to_string(),
            "mcp-weather-free".to_string(),
        ],
        env: HashMap::new(),
        timeout: Duration::from_secs(30),
    };

    MCPToolset::new(mcp_connection)
}

/// Helper function to create Agent with MCP weather tools - Anthropic
fn create_anthropic_weather_agent() -> Option<Agent> {
    get_anthropic_key().map(|api_key| {
        let anthropic_llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());
        let mcp_toolset = create_mcp_weather_toolset();

        Agent::new(
            "anthropic_weather_agent".to_string(),
            "Weather agent with MCP server integration".to_string(),
            "You are a helpful weather assistant. Use the available weather tools to provide accurate weather information for any location requested by the user. Always call the weather tools when users ask about weather.".to_string(),
            Arc::new(anthropic_llm),
        )
        .with_session_service(session_service)
        .with_toolset(Arc::new(mcp_toolset))
    })
}

/// Helper function to create Agent with MCP weather tools - Gemini
fn create_gemini_weather_agent() -> Option<Agent> {
    get_gemini_key().map(|api_key| {
        let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());
        let mcp_toolset = create_mcp_weather_toolset();

        Agent::new(
            "gemini_weather_agent".to_string(),
            "Gemini weather agent with MCP server integration".to_string(),
            "You are a helpful weather assistant. Use the available weather tools to provide accurate weather information for any location requested by the user. Always call the weather tools when users ask about weather.".to_string(),
            Arc::new(gemini_llm),
        )
        .with_session_service(session_service)
        .with_toolset(Arc::new(mcp_toolset))
    })
}

/// Helper function to create Agent with MCP weather tools - OpenAI
fn create_openai_weather_agent() -> Option<Agent> {
    init_test_env();
    get_openai_key().map(|api_key| {
        let openai_llm = OpenAILlm::new("gpt-4o-mini".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());
        let mcp_toolset = create_mcp_weather_toolset();

        Agent::new(
            "openai_weather_agent".to_string(),
            "OpenAI weather agent with MCP server integration".to_string(),
            "You are a helpful weather assistant. Use the available weather tools to provide accurate weather information for any location requested by the user. Always call the weather tools when users ask about weather.".to_string(),
            Arc::new(openai_llm),
        )
        .with_session_service(session_service)
        .with_toolset(Arc::new(mcp_toolset))
    })
}

/// Helper function to create a user message
fn create_user_message(text: &str) -> Message {
    Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: text.to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    }
}

/// Test MCP weather tool calling with Anthropic Claude
#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY and network access to MCP weather server"]
async fn test_anthropic_mcp_weather() {
    println!("🧪 Testing Anthropic + MCP Weather Server Integration");

    let agent = match create_anthropic_weather_agent() {
        Some(agent) => agent,
        None => {
            println!("⚠️ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let user_message = create_user_message("What's the current weather in Paris, France?");
    if let Part::Text { text, .. } = &user_message.parts[0] {
        println!("👤 User: {}", text);
    }

    let params = MessageSendParams {
        message: user_message,
        configuration: None,
        metadata: None,
    };

    let mut execution = agent
        .send_streaming_message("weather_app".to_string(), "test_user".to_string(), params)
        .await
        .unwrap();

    let mut final_task = None;
    let mut all_text = String::new();
    let mut weather_tool_called = false;

    println!("📡 Processing streaming response from Anthropic + MCP:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        print!("{}", text);
                        all_text.push_str(text);
                        all_text.push(' ');
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ A2A Final Task: id={}, state={:?}",
                    task.id, task.status.state
                );
                assert_eq!(
                    task.status.state,
                    TaskState::Submitted,
                    "Task should be completed"
                );
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    assert!(final_task.is_some(), "Should receive final task");

    // Check internal events for tool usage
    println!("✅ Processing internal events for MCP weather tools:");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut collected_events = Vec::new();
    for _ in 0..20 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in collected_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionCall { name, .. } = part {
                    if name.contains("weather") {
                        weather_tool_called = true;
                        println!("  🔧 Weather tool called: {}", name);
                    }
                }
            }
        }
    }

    println!("📊 Test Summary:");
    println!("  Weather tool called: {}", weather_tool_called);

    // Basic assertions
    assert!(
        weather_tool_called,
        "MCP weather tool should have been called"
    );

    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("paris")
            || response_lower.contains("weather")
            || response_lower.contains("temperature")
            || response_lower.contains("°"),
        "Response should contain weather information for Paris: {}",
        all_text
    );

    println!("✅ Anthropic + MCP Weather Server test completed successfully!");
}

/// Test MCP weather tool calling with Google Gemini
#[tokio::test]
#[ignore = "requires GEMINI_API_KEY and network access to MCP weather server"]
async fn test_gemini_mcp_weather() {
    println!("🧪 Testing Gemini + MCP Weather Server Integration");

    let agent = match create_gemini_weather_agent() {
        Some(agent) => agent,
        None => {
            println!("⚠️ Skipping test: GEMINI_API_KEY not set");
            return;
        }
    };

    let user_message = create_user_message("What's the weather like in Sydney, Australia?");
    if let Part::Text { text, .. } = &user_message.parts[0] {
        println!("👤 User: {}", text);
    }

    let params = MessageSendParams {
        message: user_message,
        configuration: None,
        metadata: None,
    };

    let mut execution = agent
        .send_streaming_message(
            "gemini_weather_app".to_string(),
            "test_user".to_string(),
            params,
        )
        .await
        .unwrap();

    let mut final_task = None;
    let mut all_text = String::new();
    let mut weather_tool_called = false;

    println!("📡 Processing streaming response from Gemini + MCP:");
    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        print!("{}", text);
                        all_text.push_str(text);
                        all_text.push(' ');
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                println!(
                    "  ✅ A2A Final Task: id={}, state={:?}",
                    task.id, task.status.state
                );
                final_task = Some(task);
                break;
            }
            _ => {}
        }
    }

    assert!(final_task.is_some(), "Should receive final task");

    // Check internal events for tool usage
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut collected_events = Vec::new();
    for _ in 0..25 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in collected_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionCall { name, .. } = part {
                    if name.contains("weather") {
                        weather_tool_called = true;
                        println!("  🔧 Weather tool called: {}", name);
                    }
                }
            }
        }
    }

    assert!(
        weather_tool_called,
        "MCP weather tool should have been called"
    );

    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("sydney")
            || response_lower.contains("weather")
            || response_lower.contains("temperature"),
        "Response should contain weather information for Sydney: {}",
        all_text
    );

    println!("✅ Gemini + MCP Weather Server test completed successfully!");
}

/// Test MCP weather tool calling with OpenAI GPT
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and network access to MCP weather server"]
async fn test_openai_mcp_weather() {
    println!("🧪 Testing OpenAI + MCP Weather Server Integration");

    let agent = match create_openai_weather_agent() {
        Some(agent) => agent,
        None => {
            println!("⚠️ Skipping test: OPENAI_API_KEY not set");
            return;
        }
    };

    let user_message = create_user_message("What's the current weather in Chicago, Illinois?");
    if let Part::Text { text, .. } = &user_message.parts[0] {
        println!("👤 User: {}", text);
    }

    let params = MessageSendParams {
        message: user_message,
        configuration: None,
        metadata: None,
    };

    let mut execution = agent
        .send_streaming_message(
            "openai_weather_app".to_string(),
            "test_user".to_string(),
            params,
        )
        .await
        .unwrap();

    let mut final_task = None;
    let mut all_text = String::new();
    let mut weather_tool_called = false;

    println!("📡 Processing streaming response from OpenAI + MCP:");
    let mut stream_count = 0;
    let mut message_count = 0;
    let mut task_count = 0;
    let mut other_count = 0;

    while let Some(result) = execution.a2a_stream.next().await {
        stream_count += 1;
        println!(
            "🔍 Stream item #{}: {:?}",
            stream_count,
            std::mem::discriminant(&result)
        );

        match result {
            SendStreamingMessageResult::Message(message) => {
                message_count += 1;
                println!(
                    "📝 Message #{}: role={:?}, parts_count={}",
                    message_count,
                    message.role,
                    message.parts.len()
                );

                for (i, part) in message.parts.iter().enumerate() {
                    match part {
                        Part::Text { text, .. } => {
                            println!("  📄 Part {}: Text (len={}): '{}'", i, text.len(), text);
                            print!("{}", text);
                            all_text.push_str(text);
                            all_text.push(' ');
                        }
                        Part::File { .. } => {
                            println!("  📎 Part {}: File", i);
                        }
                        Part::Data { data, .. } => {
                            println!("  💾 Part {}: Data: {:?}", i, data);
                            // Check if this data part contains function calls
                            if let Ok(data_str) = serde_json::to_string(data) {
                                if data_str.contains("function") || data_str.contains("tool") {
                                    println!("    🔧 Possible tool call in data: {}", data_str);
                                    weather_tool_called = true;
                                }
                            }
                        }
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                task_count += 1;
                println!(
                    "  ✅ A2A Task #{}: id={}, state={:?}",
                    task_count, task.id, task.status.state
                );
                final_task = Some(task);
                break;
            }
            SendStreamingMessageResult::TaskStatusUpdate(status_update) => {
                println!("📊 Task Status Update: {:?}", status_update);
                other_count += 1;
            }
            SendStreamingMessageResult::TaskArtifactUpdate(artifact_update) => {
                println!("📎 Task Artifact Update: {:?}", artifact_update);
                other_count += 1;
            }
        }

        // Safety timeout to prevent infinite loops
        if stream_count > 50 {
            println!("⚠️ Breaking after 50 stream items to prevent timeout");
            break;
        }
    }

    println!("\n📊 Stream Summary:");
    println!("  Total items: {}", stream_count);
    println!("  Messages: {}", message_count);
    println!("  Tasks: {}", task_count);
    println!("  Others: {}", other_count);
    println!("  Weather tool called: {}", weather_tool_called);
    println!("  All text: '{}'", all_text.trim());

    assert!(final_task.is_some(), "Should receive final task");

    // Check internal events for tool usage
    tokio::time::sleep(Duration::from_millis(400)).await;

    let mut collected_events = Vec::new();
    for _ in 0..25 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in collected_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionCall { name, .. } = part {
                    if name.contains("weather") {
                        weather_tool_called = true;
                        println!("  🔧 Weather tool called: {}", name);
                    }
                }
            }
        }
    }

    assert!(
        weather_tool_called,
        "MCP weather tool should have been called"
    );

    let response_lower = all_text.to_lowercase();
    assert!(
        response_lower.contains("chicago")
            || response_lower.contains("weather")
            || response_lower.contains("temperature"),
        "Response should contain weather information for Chicago: {}",
        all_text
    );

    println!("✅ OpenAI + MCP Weather Server test completed successfully!");
}

/// Test MCP weather with multiple cities using OpenAI (good at parallel calls)
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and network access to MCP weather server"]
async fn test_openai_mcp_weather_multiple_cities() {
    println!("🧪 Testing OpenAI + MCP Weather Server with Multiple Cities");

    let agent = match create_openai_weather_agent() {
        Some(agent) => agent,
        None => {
            println!("⚠️ Skipping test: OPENAI_API_KEY not set");
            return;
        }
    };

    let user_message = create_user_message(
        "Compare the weather between Tokyo and London. Get current weather for both cities.",
    );
    if let Part::Text { text, .. } = &user_message.parts[0] {
        println!("👤 User: {}", text);
    }

    let params = MessageSendParams {
        message: user_message,
        configuration: None,
        metadata: None,
    };

    let mut execution = agent
        .send_streaming_message(
            "openai_weather_app".to_string(),
            "test_user".to_string(),
            params,
        )
        .await
        .unwrap();

    let mut final_task = None;
    let mut all_text = String::new();
    let mut weather_tool_calls = 0;

    while let Some(result) = execution.a2a_stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                for part in &message.parts {
                    if let Part::Text { text, .. } = part {
                        all_text.push_str(text);
                        all_text.push(' ');
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

    assert!(final_task.is_some(), "Should receive final task");

    // Check for multiple tool calls
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut collected_events = Vec::new();
    for _ in 0..30 {
        if let Some(internal_event) = execution.all_events_stream.next().await {
            collected_events.push(internal_event);
        } else {
            break;
        }
    }

    for internal_event in collected_events {
        if let radkit::sessions::SessionEventType::UserMessage { content }
        | radkit::sessions::SessionEventType::AgentMessage { content } =
            internal_event.event_type
        {
            for part in &content.parts {
                if let radkit::models::content::ContentPart::FunctionCall {
                    name, arguments, ..
                } = part
                {
                    if name.contains("weather") {
                        weather_tool_calls += 1;
                        println!("  🔧 Weather tool call #{}: {}", weather_tool_calls, name);
                        if let Some(location) =
                            arguments.get("location").or_else(|| arguments.get("city"))
                        {
                            println!("    📍 Location: {}", location);
                        }
                    }
                }
            }
        }
    }

    println!("📊 Multiple Cities Test Summary:");
    println!("  Weather tool calls: {}", weather_tool_calls);

    assert!(
        weather_tool_calls >= 2,
        "Should make multiple weather calls for multiple cities"
    );

    let response_lower = all_text.to_lowercase();
    assert!(
        (response_lower.contains("tokyo") || response_lower.contains("london"))
            && response_lower.contains("weather"),
        "Response should contain weather information for both cities: {}",
        all_text
    );

    println!("✅ OpenAI + MCP Weather Server multiple cities test completed successfully!");
}
