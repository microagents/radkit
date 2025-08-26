//! Radkit Test Suite Entrypoint
//!
//! This is the main entry point for all real LLM API tests in the Radkit project.
//! It provides a quick overview and runs basic validation tests using the Agent.
//!
//! ## Test Structure:
//!
//! ### Anthropic Claude Tests:
//! - `anthropic_simple.rs`: Basic conversation tests
//! - `anthropic_tool_use.rs`: Function calling tests with full round-trip validation
//! - `anthropic_multi_turn.rs`: Conversation history and context retention tests
//!
//! ### Google Gemini Tests:
//! - `gemini_simple.rs`: Basic conversation tests
//! - `gemini_tool_use.rs`: Function calling tests with full round-trip validation
//! - `gemini_multi_turn_tool_use.rs`: Multi-turn conversations with function calling
//!
//! ## Usage:
//!
//! **Run the entrypoint overview:**
//! ```bash
//! cargo test --test entrypoint --ignored
//! ```
//!
//! **Run all tests:**
//! ```bash
//! cargo test --ignored
//! ```
//!
//! **Run specific provider tests:**
//! ```bash
//! # Anthropic tests
//! cargo test --test anthropic_simple --ignored
//! cargo test --test anthropic_tool_use --ignored
//! cargo test --test anthropic_multi_turn --ignored
//!
//! # Gemini tests  
//! cargo test --test gemini_simple --ignored
//! cargo test --test gemini_tool_use --ignored
//! cargo test --test gemini_multi_turn_tool_use --ignored
//! ```
//!
//! ## Requirements:
//! - `ANTHROPIC_API_KEY` environment variable for Claude tests
//! - `GEMINI_API_KEY` environment variable for Gemini tests

use futures::StreamExt;
use radkit::a2a::{
    Message, MessageRole, MessageSendParams, Part, SendMessageResult, SendStreamingMessageResult,
};
use radkit::agents::Agent;
use radkit::models::{AnthropicLlm, GeminiLlm};
use std::sync::Arc;

mod common;
use common::{get_anthropic_key, get_gemini_key, has_anthropic_key, has_gemini_key, init_test_env};

/// Helper function to create a simple user message
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

/// Helper function to create Agent with Anthropic if API key is available
fn create_anthropic_agent() -> Option<Agent> {
    init_test_env();
    get_anthropic_key().map(|api_key| {
        let anthropic_llm = AnthropicLlm::new("claude-3-5-sonnet-20241022".to_string(), api_key);

        Agent::new(
            "anthropic_test_agent".to_string(),
            "Anthropic test agent".to_string(),
            "You are Claude, a helpful AI assistant. Respond briefly and clearly.".to_string(),
            Arc::new(anthropic_llm),
        )
    })
}

/// Helper function to create Agent with Gemini if API key is available
fn create_gemini_agent() -> Option<Agent> {
    init_test_env();
    get_gemini_key().map(|api_key| {
        let gemini_llm = GeminiLlm::new("gemini-2.0-flash-exp".to_string(), api_key);

        Agent::new(
            "gemini_test_agent".to_string(),
            "Gemini test agent".to_string(),
            "You are Gemini, a helpful AI assistant. Respond briefly and clearly.".to_string(),
            Arc::new(gemini_llm),
        )
    })
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API keys are available
async fn test_real_llm_overview() {
    println!("🧪 Real LLM Integration Test Overview");
    println!("=====================================");

    init_test_env();
    let has_anthropic = has_anthropic_key();
    let has_gemini = has_gemini_key();

    println!("📋 Available Providers:");
    println!(
        "  - Anthropic Claude: {}",
        if has_anthropic {
            "✅ Available"
        } else {
            "❌ Missing API key"
        }
    );
    println!(
        "  - Google Gemini: {}",
        if has_gemini {
            "✅ Available"
        } else {
            "❌ Missing API key"
        }
    );

    if !has_anthropic && !has_gemini {
        println!(
            "⚠️  No API keys found. Set ANTHROPIC_API_KEY and/or GEMINI_API_KEY to run tests."
        );
        return;
    }

    println!("\n🚀 Running quick validation tests...");

    // Test Anthropic if available
    if has_anthropic {
        if let Some(agent) = create_anthropic_agent() {
            println!("\n🔹 Testing Anthropic Claude...");

            let message = create_user_message("Say 'Hello from Claude!' in exactly those words.");
            let params = MessageSendParams {
                message,
                configuration: None,
                metadata: None,
            };

            let result = agent
                .send_message("test_app".to_string(), "test_user".to_string(), params)
                .await;

            if result.is_ok() {
                let send_result = result.unwrap();
                let mut got_response = false;

                match send_result.result {
                    SendMessageResult::Task(task) => {
                        for message in &task.history {
                            if message.role == MessageRole::Agent {
                                for part in &message.parts {
                                    if let Part::Text { text, .. } = part {
                                        if text.to_lowercase().contains("hello") {
                                            got_response = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                println!(
                    "  ✅ Anthropic Claude: {}",
                    if got_response {
                        "Working"
                    } else {
                        "Response received but unexpected format"
                    }
                );
            } else {
                println!("  ❌ Anthropic Claude: Failed to get response");
            }
        }
    }

    // Test Gemini if available
    if has_gemini {
        if let Some(agent) = create_gemini_agent() {
            println!("\n🔹 Testing Google Gemini...");

            let message = create_user_message("Say 'Hello from Gemini!' in exactly those words.");
            let params = MessageSendParams {
                message,
                configuration: None,
                metadata: None,
            };

            let result = agent
                .send_message("test_app".to_string(), "test_user".to_string(), params)
                .await;

            if result.is_ok() {
                let send_result = result.unwrap();
                let mut got_response = false;

                match send_result.result {
                    SendMessageResult::Task(task) => {
                        for message in &task.history {
                            if message.role == MessageRole::Agent {
                                for part in &message.parts {
                                    if let Part::Text { text, .. } = part {
                                        if text.to_lowercase().contains("hello") {
                                            got_response = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                println!(
                    "  ✅ Google Gemini: {}",
                    if got_response {
                        "Working"
                    } else {
                        "Response received but unexpected format"
                    }
                );
            } else {
                println!("  ❌ Google Gemini: Failed to get response");
            }
        }
    }

    println!("\n🎯 For detailed testing, run specific test files:");
    println!("   # Anthropic Claude tests:");
    println!("   cargo test --test anthropic_simple --ignored");
    println!("   cargo test --test anthropic_tool_use --ignored");
    println!("   cargo test --test anthropic_multi_turn --ignored");
    println!("   ");
    println!("   # Google Gemini tests:");
    println!("   cargo test --test gemini_simple --ignored");
    println!("   cargo test --test gemini_tool_use --ignored");
    println!("   cargo test --test gemini_multi_turn_tool_use --ignored");

    println!("\n✅ Overview test completed!");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API keys are available
async fn test_anthropic_basic_sanity() {
    let Some(agent) = create_anthropic_agent() else {
        println!("⚠️  Skipping test: ANTHROPIC_API_KEY not found");
        return;
    };

    println!("🧪 Testing Anthropic basic sanity check (streaming)...");

    let message = create_user_message("What is 1+1? Answer with just the number.");
    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_streaming_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let mut execution = result.unwrap();
    let mut all_text = String::new();

    while let Some(result) = execution.stream.next().await {
        match result {
            SendStreamingMessageResult::Message(message) => {
                if message.role == MessageRole::Agent {
                    for part in &message.parts {
                        if let Part::Text { text, .. } = part {
                            all_text.push_str(text);
                        }
                    }
                }
            }
            SendStreamingMessageResult::Task(task) => {
                for message in &task.history {
                    if message.role == MessageRole::Agent {
                        for part in &message.parts {
                            if let Part::Text { text, .. } = part {
                                all_text.push_str(text);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    assert!(
        all_text.contains("2"),
        "Response should contain '2': {}",
        all_text
    );
    println!("✅ Anthropic streaming sanity check passed!");
}

#[tokio::test]
#[ignore] // Only run with --ignored flag when API keys are available
async fn test_gemini_basic_sanity() {
    let Some(agent) = create_gemini_agent() else {
        println!("⚠️  Skipping test: GEMINI_API_KEY not found");
        return;
    };

    println!("🧪 Testing Gemini basic sanity check...");

    let message = create_user_message("What is 1+1? Answer with just the number.");
    let params = MessageSendParams {
        message,
        configuration: None,
        metadata: None,
    };

    let result = agent
        .send_message("test_app".to_string(), "test_user".to_string(), params)
        .await;
    assert!(result.is_ok(), "Agent execution should succeed");

    let send_result = result.unwrap();
    let mut all_text = String::new();

    match send_result.result {
        SendMessageResult::Task(task) => {
            for message in &task.history {
                if message.role == MessageRole::Agent {
                    for part in &message.parts {
                        if let Part::Text { text, .. } = part {
                            all_text.push_str(text);
                        }
                    }
                }
            }
        }
        _ => panic!("Expected Task result"),
    }

    assert!(
        all_text.contains("2"),
        "Response should contain '2': {}",
        all_text
    );
    println!("✅ Gemini sanity check passed!");
}
