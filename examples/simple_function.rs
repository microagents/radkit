//! Simple example demonstrating LlmFunction for structured outputs.
//!
//! This example shows how to use LlmFunction to get type-safe responses
//! from an LLM without tool execution.
//!
//! Run with: cargo run --example simple_function

use radkit::agent::LlmFunction;
use radkit::models::providers::AnthropicLlm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WeatherResponse {
    temperature: f64,
    condition: String,
    location: String,
    humidity: Option<u8>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create LLM client from environment variable (ANTHROPIC_API_KEY)
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // Create function that returns structured output
    let weather_fn = LlmFunction::<WeatherResponse>::new(llm);

    // Run with natural language input
    println!("🤖 Asking LLM about weather in Tokyo...\n");
    let response = weather_fn
        .run("What's the weather like in Tokyo right now?")
        .await?;

    // Access structured fields
    println!("📍 Location: {}", response.location);
    println!("🌡️  Temperature: {}°F", response.temperature);
    println!("☁️  Condition: {}", response.condition);
    if let Some(humidity) = response.humidity {
        println!("💧 Humidity: {}%", humidity);
    }

    Ok(())
}
