//! Complete weather agent example with tool execution.
//!
//! This example demonstrates a realistic agent workflow where the LLM
//! calls a weather API tool and then returns structured information.
//!
//! Run with: cargo run --example weather_agent

use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
    humidity: u8,
    wind_speed: f64,
    forecast: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create weather API tool
    let weather_tool = Arc::new(
        FunctionTool::new(
            "get_weather",
            "Get current weather conditions for a specific location",
            |args, _ctx| {
                Box::pin(async move {
                    let location = args
                        .get("location")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");

                    // Simulate API call (in production, call a real weather API)
                    println!("🌐 Fetching weather for: {}", location);

                    // Mock weather data based on location
                    let (temp, condition, humidity, wind) = match location.to_lowercase().as_str() {
                        loc if loc.contains("tokyo") => (72.5, "Sunny", 65, 8.5),
                        loc if loc.contains("london") => (58.0, "Cloudy", 78, 12.3),
                        loc if loc.contains("new york") => (65.2, "Partly Cloudy", 70, 10.1),
                        loc if loc.contains("san francisco") => (68.8, "Foggy", 85, 6.7),
                        _ => (70.0, "Clear", 60, 5.0),
                    };

                    ToolResult::success(json!({
                        "location": location,
                        "temperature": temp,
                        "condition": condition,
                        "humidity": humidity,
                        "wind_speed": wind,
                        "timestamp": "2024-01-15T10:30:00Z"
                    }))
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City name or location (e.g., 'Tokyo', 'London', 'New York')"
                }
            },
            "required": ["location"]
        })),
    );

    // Create LLM client
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // Create weather agent with tool
    let weather_agent = LlmWorker::<WeatherReport>::builder(llm)
        .with_system_instructions(
            "You are a helpful weather assistant. Use the get_weather tool to fetch \
             current conditions and provide a brief forecast summary.",
        )
        .with_tool(weather_tool)
        .with_max_iterations(5)
        .build();

    // Test with different locations
    let locations = vec![
        "What's the weather in Tokyo?",
        "How's it looking in San Francisco?",
        "Tell me about the weather in London",
    ];

    for query in locations {
        println!("\n{}\n", "=".repeat(60));
        println!("❓ Query: {}", query);
        println!();

        match weather_agent.run(query).await {
            Ok(report) => {
                println!("📍 Location: {}", report.location);
                println!("🌡️  Temperature: {}°F", report.temperature);
                println!("☁️  Condition: {}", report.condition);
                println!("💧 Humidity: {}%", report.humidity);
                println!("💨 Wind Speed: {} mph", report.wind_speed);
                println!("📅 Forecast: {}", report.forecast);
            }
            Err(e) => {
                eprintln!("❌ Error: {}", e);
            }
        }
    }

    println!("\n{}\n", "=".repeat(60));

    Ok(())
}
