//! OpenAI GPT Multi-turn Conversation Tests
//!
//! Tests multi-turn conversations with context persistence and tool use across turns.
//! These tests require OPENAI_API_KEY environment variable to be set.

use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult,
};
use radkit::agents::Agent;
use radkit::events::InternalEvent;
use radkit::models::OpenAILlm;
use radkit::sessions::InMemorySessionService;
use radkit::task::InMemoryTaskStore;
use radkit::tools::{FunctionTool, ToolResult};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

mod common;

/// Helper function to get OpenAI API key from environment
fn get_openai_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY").ok()
}

/// Create a stateful counter tool that maintains state across calls
fn create_counter_tool() -> (Arc<FunctionTool>, Arc<Mutex<i32>>) {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let tool = FunctionTool::new(
        "counter".to_string(),
        "Increment and return a counter value".to_string(),
        move |args: HashMap<String, Value>, _context| {
            let counter = counter_clone.clone();
            Box::pin(async move {
                let increment = args
                    .get("increment")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1) as i32;

                let mut count = counter.lock().unwrap();
                *count += increment;
                let current_value = *count;

                ToolResult {
                    success: true,
                    data: json!({
                        "previous_value": current_value - increment,
                        "increment": increment,
                        "current_value": current_value
                    }),
                    error_message: None,
                }
            })
        },
    )
    .with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "increment": {
                "type": "integer",
                "description": "Amount to increment the counter by (default: 1)"
            }
        }
    }));

    (Arc::new(tool), counter)
}

/// Create a memory tool that can store and retrieve values
fn create_memory_tool() -> Arc<FunctionTool> {
    let memory = Arc::new(Mutex::new(HashMap::<String, String>::new()));

    Arc::new(
        FunctionTool::new(
            "memory".to_string(),
            "Store or retrieve a value from memory".to_string(),
            move |args: HashMap<String, Value>, _context| {
                let memory = memory.clone();
                Box::pin(async move {
                    let action = args
                        .get("action")
                        .and_then(|v| v.as_str())
                        .unwrap_or("get");

                    let key = args
                        .get("key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default");

                    let mut mem = memory.lock().unwrap();

                    match action {
                        "set" => {
                            let value = args
                                .get("value")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            mem.insert(key.to_string(), value.to_string());

                            ToolResult {
                                success: true,
                                data: json!({
                                    "action": "set",
                                    "key": key,
                                    "value": value,
                                    "message": format!("Stored '{}' = '{}'", key, value)
                                }),
                                error_message: None,
                            }
                        }
                        "get" => {
                            let value = mem.get(key).cloned();

                            ToolResult {
                                success: true,
                                data: json!({
                                    "action": "get",
                                    "key": key,
                                    "value": value,
                                    "found": value.is_some()
                                }),
                                error_message: None,
                            }
                        }
                        "list" => {
                            let keys: Vec<String> = mem.keys().cloned().collect();

                            ToolResult {
                                success: true,
                                data: json!({
                                    "action": "list",
                                    "keys": keys,
                                    "count": keys.len()
                                }),
                                error_message: None,
                            }
                        }
                        _ => ToolResult {
                            success: false,
                            data: json!(null),
                            error_message: Some(format!("Unknown action: {}", action)),
                        },
                    }
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "set", "list"],
                    "description": "The action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The key to store/retrieve"
                },
                "value": {
                    "type": "string",
                    "description": "The value to store (only for 'set' action)"
                }
            },
            "required": ["action"]
        })),
    )
}

/// Helper function to create Agent with tools
fn create_test_agent_with_tools(tools: Vec<Arc<FunctionTool>>) -> Option<Agent> {
    get_openai_key().map(|api_key| {
        let openai_llm = OpenAILlm::new("gpt-4o-mini".to_string(), api_key);
        let session_service = Arc::new(InMemorySessionService::new());
        let task_store = Arc::new(InMemoryTaskStore::new());

        let base_tools: Vec<Arc<dyn radkit::tools::BaseTool>> = tools
            .into_iter()
            .map(|tool| -> Arc<dyn radkit::tools::BaseTool> { tool })
            .collect();

        Agent::new(
            "test_agent".to_string(),
            "Test agent for multi-turn conversations".to_string(),
            "You are a helpful assistant with access to tools. Use them when requested. Remember information from previous turns in the conversation."
                .to_string(),
            Arc::new(openai_llm),
        )
        .with_session_service(session_service)
        .with_task_store(task_store)
        .with_tools(base_tools)
    })
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_multi_turn_with_stateful_tools() {
    let (counter_tool, counter_state) = create_counter_tool();
    let memory_tool = create_memory_tool();
    
    let Some(agent) = create_test_agent_with_tools(vec![counter_tool, memory_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI multi-turn conversation with stateful tools...");

    // ✅ Turn 1: Store a value in memory
    let message1 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please store the value 'OpenAI Test' with the key 'test_key' in memory.".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params1 = MessageSendParams {
        message: message1,
        configuration: None,
        metadata: None,
    };

    let result1 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params1)
        .await;
    assert!(result1.is_ok(), "First message should succeed");

    let task1 = match result1.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    let context_id = task1.context_id.clone();

    // ✅ Turn 2: Increment counter
    let message2 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Please increment the counter by 5.".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()),
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params2 = MessageSendParams {
        message: message2,
        configuration: None,
        metadata: None,
    };

    let result2 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params2)
        .await;
    assert!(result2.is_ok(), "Second message should succeed");

    let task2 = match result2.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // Verify counter was incremented
    assert_eq!(*counter_state.lock().unwrap(), 5, "Counter should be 5");
    println!("  ✅ Turn 2: Counter incremented to 5");

    // ✅ Turn 3: Retrieve memory and increment counter again
    let message3 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "What value did we store with key 'test_key'? Also increment the counter by 3.".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()),
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params3 = MessageSendParams {
        message: message3,
        configuration: None,
        metadata: None,
    };

    let result3 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params3)
        .await;
    assert!(result3.is_ok(), "Third message should succeed");

    let task3 = match result3.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // Verify final counter state
    assert_eq!(*counter_state.lock().unwrap(), 8, "Counter should be 8");

    // ✅ Validate tool execution via session events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut memory_set_calls = 0;
    let mut memory_get_calls = 0;
    let mut counter_calls = 0;
    let mut found_openai_test_value = false;

    println!("✅ Validating tool execution across all turns:");
    for event in &session.events {
        if let InternalEvent::MessageReceived { content, .. } = event {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, arguments, .. } => {
                        match name.as_str() {
                            "memory" => {
                                let action = arguments.get("action").and_then(|v| v.as_str());
                                match action {
                                    Some("set") => {
                                        memory_set_calls += 1;
                                        println!("  🔧 Memory set call");
                                    }
                                    Some("get") => {
                                        memory_get_calls += 1;
                                        println!("  🔧 Memory get call");
                                    }
                                    _ => {}
                                }
                            }
                            "counter" => {
                                counter_calls += 1;
                                println!("  🔧 Counter call");
                            }
                            _ => {}
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        if name.as_str() == "memory" && *success {
                            if let Some(value) = result.get("value").and_then(|v| v.as_str()) {
                                if value == "OpenAI Test" {
                                    found_openai_test_value = true;
                                    println!("  ✅ Retrieved stored value: {}", value);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ✅ Validate tool execution
    assert!(memory_set_calls >= 1, "Should have made memory set calls");
    assert!(memory_get_calls >= 1, "Should have made memory get calls");  
    assert!(counter_calls >= 2, "Should have made at least 2 counter calls");
    assert!(found_openai_test_value, "Should have retrieved the stored OpenAI Test value");

    // ✅ Validate conversation context
    let mut mentioned_memory = false;
    let mut mentioned_counter = false;

    for message in &task3.history {
        if message.role == MessageRole::Agent {
            for part in &message.parts {
                if let Part::Text { text, .. } = part {
                    let text_lower = text.to_lowercase();
                    if text_lower.contains("openai test") {
                        mentioned_memory = true;
                        println!("  ✅ Model mentioned stored value in response");
                    }
                    if text_lower.contains("8") || text_lower.contains("counter") {
                        mentioned_counter = true;
                        println!("  ✅ Model mentioned counter value in response");
                    }
                }
            }
        }
    }

    // ✅ Validate all tasks are in the same context
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should be able to list tasks");

    let context_tasks: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.context_id == context_id)
        .collect();

    assert_eq!(
        context_tasks.len(),
        3,
        "Should have exactly 3 tasks in the same context"
    );

    println!("✅ Multi-turn conversation with stateful tools completed successfully");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API key is available
async fn test_openai_cross_task_tool_persistence() {
    let memory_tool = create_memory_tool();
    
    let Some(agent) = create_test_agent_with_tools(vec![memory_tool]) else {
        println!("⚠️  Skipping test: OPENAI_API_KEY not found");
        return;
    };

    println!("🧪 Testing OpenAI cross-task tool state persistence...");

    // ✅ Task 1: Store multiple values
    let message1 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "Store 'name' = 'GPT-4', 'version' = 'OpenAI', and 'type' = 'LLM' in memory. Then list all keys.".to_string(),
            metadata: None,
        }],
        context_id: None,
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params1 = MessageSendParams {
        message: message1,
        configuration: None,
        metadata: None,
    };

    let result1 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params1)
        .await;
    assert!(result1.is_ok(), "First task should succeed");

    let task1 = match result1.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    let context_id = task1.context_id.clone();

    // ✅ Task 2: Retrieve specific values in new task
    let message2 = Message {
        kind: "message".to_string(),
        message_id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        parts: vec![Part::Text {
            text: "What are the values for 'name' and 'version' in memory?".to_string(),
            metadata: None,
        }],
        context_id: Some(context_id.clone()),
        task_id: None,
        reference_task_ids: Vec::new(),
        extensions: Vec::new(),
        metadata: None,
    };

    let params2 = MessageSendParams {
        message: message2,
        configuration: None,
        metadata: None,
    };

    let result2 = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params2)
        .await;
    assert!(result2.is_ok(), "Second task should succeed");

    let task2 = match result2.unwrap().result {
        SendMessageResult::Task(task) => task,
        _ => panic!("Expected Task result"),
    };

    // ✅ Validate tool persistence via session events
    let session_service = agent.session_service();
    let session = session_service
        .get_session("test_app", "test_user", &context_id)
        .await
        .expect("Should be able to get session")
        .expect("Session should exist");

    let mut set_calls = 0;
    let mut get_calls = 0;
    let mut list_calls = 0;
    let mut found_gpt4 = false;
    let mut found_openai = false;

    println!("✅ Validating cross-task tool persistence:");
    for event in &session.events {
        if let InternalEvent::MessageReceived { content, .. } = event {
            for part in &content.parts {
                match part {
                    radkit::models::content::ContentPart::FunctionCall { name, arguments, .. } => {
                        if name.as_str() == "memory" {
                            let action = arguments.get("action").and_then(|v| v.as_str());
                            match action {
                                Some("set") => set_calls += 1,
                                Some("get") => get_calls += 1,
                                Some("list") => list_calls += 1,
                                _ => {}
                            }
                        }
                    }
                    radkit::models::content::ContentPart::FunctionResponse {
                        name,
                        success,
                        result,
                        ..
                    } => {
                        if name.as_str() == "memory" && *success {
                            if let Some(value) = result.get("value").and_then(|v| v.as_str()) {
                                if value == "GPT-4" {
                                    found_gpt4 = true;
                                }
                                if value == "OpenAI" {
                                    found_openai = true;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    assert!(set_calls >= 3, "Should have made at least 3 set calls");
    assert!(get_calls >= 2, "Should have made at least 2 get calls");
    assert!(found_gpt4 && found_openai, "Should have retrieved both stored values across tasks");

    println!("  ✅ Tool calls: {} sets, {} gets, {} lists", set_calls, get_calls, list_calls);
    println!("  ✅ Retrieved values: GPT-4 = {}, OpenAI = {}", found_gpt4, found_openai);

    // ✅ Validate task management
    let all_tasks = agent
        .list_tasks("test_app", "test_user")
        .await
        .expect("Should be able to list tasks");

    let context_tasks: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.context_id == context_id)
        .collect();

    assert_eq!(
        context_tasks.len(),
        2,
        "Should have exactly 2 tasks in the same context"
    );

    println!("✅ Cross-task tool state persistence test completed successfully");
}