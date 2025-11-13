---
title: Tool Execution
description: Enable LLMs to call functions and interact with the outside world using LlmWorker.
---



`LlmWorker<T>` builds on `LlmFunction<T>` by adding a powerful capability: **tool execution**. It allows the LLM to call functions you provide to interact with the outside world, get information, or perform actions.

The `LlmWorker` manages a multi-turn loop:
1. It calls the LLM with the user's prompt and a list of available tools.
2. If the LLM decides to call a tool, Radkit executes your tool function.
3. The tool's output is sent back to the LLM.
4. The LLM uses the tool's output to formulate its final, structured response.

## Basic Tool Use

Let's build a simple weather agent that uses a `get_weather` tool.

```rust
use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{tool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

// 1. Define the final output structure
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
    forecast: String,
}

// 2. Define tool arguments
#[derive(Deserialize, JsonSchema)]
struct GetWeatherArgs {
    /// The city and state, e.g., San Francisco, CA
    location: String,
}

// 3. Define the tool using the #[tool] macro
#[tool(description = "Get current weather for a location")]
async fn get_weather(args: GetWeatherArgs) -> ToolResult {
    // In a real app, you would call a weather API here
    let weather_data = json!({
        "temperature": 72.5,
        "condition": "Sunny",
        "humidity": 65,
        "location": args.location,
    });

    ToolResult::success(weather_data)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 4. Create the worker with the tool
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;
    let worker = LlmWorker::<WeatherReport>::builder(llm)
        .with_system_instructions("You are a friendly weather assistant.")
        .with_tool(get_weather)  // Pass the tool directly
        .build();

    // 5. Run the worker
    let report = worker.run("What's the weather in San Francisco?").await?;

    println!("📍 Location: {}", report.location);
    println!("🌡️ Temperature: {}°F", report.temperature);
    println!("☀️ Condition: {}", report.condition);
    println!("📅 Forecast: {}", report.forecast);

    Ok(())
}
```

## Multiple Tools

An `LlmWorker` can be equipped with multiple tools. The LLM will automatically choose the right tool (or sequence of tools) to accomplish the user's goal.

Let's create a travel agent that can get weather, search for hotels, and calculate costs.

```rust
use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{tool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TravelPlan {
    destination: String,
    weather_summary: String,
    hotel_recommendation: String,
    estimated_cost: f64,
}

// Define tool arguments
#[derive(Deserialize, JsonSchema)]
struct WeatherArgs {
    location: String,
}

#[derive(Deserialize, JsonSchema)]
struct HotelArgs {
    location: String,
}

#[derive(Deserialize, JsonSchema)]
struct CostArgs {
    nights: i64,
}

// Define the tools using the #[tool] macro
#[tool(description = "Get weather forecast")]
async fn get_weather(args: WeatherArgs) -> ToolResult {
    ToolResult::success(json!({
        "forecast": format!("Sunny and 75°F in {}", args.location)
    }))
}

#[tool(description = "Search for hotels")]
async fn search_hotels(args: HotelArgs) -> ToolResult {
    ToolResult::success(json!({
        "hotel": "The Grand Hotel",
        "location": args.location
    }))
}

#[tool(description = "Calculate trip cost")]
async fn calculate_cost(args: CostArgs) -> ToolResult {
    let cost = args.nights as f64 * 150.0 + 500.0;  // Hotel + flight estimate
    ToolResult::success(json!({"cost": cost}))
}

let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

// Build the worker with multiple tools
let worker = LlmWorker::<TravelPlan>::builder(llm)
    .with_system_instructions("You are a helpful travel planning assistant.")
    .with_tools(vec![get_weather, search_hotels, calculate_cost])
    .build();

// Run the worker
let plan = worker.run("Plan a 3-day trip to Tokyo").await?;

println!("🗺️ {}", plan.destination);
println!("🌤️ {}", plan.weather_summary);
println!("🏨 {}", plan.hotel_recommendation);
println!("💰 Estimated cost: ${:.2}", plan.estimated_cost);
```

When you run this, the `LlmWorker` will orchestrate multiple calls to the LLM and your tools to gather all the necessary information before producing the final `TravelPlan`.