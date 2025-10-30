//! Multi-turn conversation example using LlmFunction.
//!
//! This example demonstrates how to maintain conversation context
//! across multiple exchanges with the LLM.
//!
//! Run with: cargo run --example multi_turn_conversation

use radkit::agent::LlmFunction;
use radkit::models::{providers::AnthropicLlm, Event};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Response {
    answer: String,
    confidence: f32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create LLM client
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // Create conversational function
    let qa_function = LlmFunction::<Response>::new_with_system_instructions(
        llm,
        "You are a knowledgeable programming tutor. Answer questions clearly and concisely.",
    );

    println!("🤖 Multi-Turn Conversation Example\n");
    println!("{}\n", "=".repeat(60));

    // First question
    let question1 = "What is Rust?";
    println!("👤 User: {}", question1);

    let (answer1, thread) = qa_function.run_and_continue(question1).await?;

    println!("🤖 Assistant: {}", answer1.answer);
    println!("   Confidence: {:.0}%\n", answer1.confidence * 100.0);

    // Follow-up question (continues conversation)
    let question2 = "What are its main benefits?";
    println!("👤 User: {}", question2);

    let (answer2, thread) = qa_function
        .run_and_continue(thread.add_event(Event::user(question2)))
        .await?;

    println!("🤖 Assistant: {}", answer2.answer);
    println!("   Confidence: {:.0}%\n", answer2.confidence * 100.0);

    // Another follow-up
    let question3 = "Can you give me a simple code example?";
    println!("👤 User: {}", question3);

    let (answer3, thread) = qa_function
        .run_and_continue(thread.add_event(Event::user(question3)))
        .await?;

    println!("🤖 Assistant: {}", answer3.answer);
    println!("   Confidence: {:.0}%\n", answer3.confidence * 100.0);

    // Show conversation history
    println!("\n{}", "=".repeat(60));
    println!("📜 Conversation Summary:\n");
    println!("Total exchanges: {}", thread.events().len() / 2);
    println!("System prompt: {}", thread.system().unwrap_or("None"));

    Ok(())
}
