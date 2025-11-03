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
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

// 1. Define the final output structure
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WeatherReport {
    location: String,
    temperature: f64,
    condition: String,
    forecast: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 2. Define the tool
    let weather_tool = Arc::new(FunctionTool::new(
        "get_weather",
        "Get current weather for a location",
        |args, _ctx| {
            Box::pin(async move {
                let location = args
                    .get("location")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                // In a real app, you would call a weather API here
                let weather_data = json!({
                    "temperature": 72.5,
                    "condition": "Sunny",
                    "humidity": 65,
                });

                ToolResult::success(weather_data)
            })
        },
    ).with_parameters_schema(json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "description": "The city and state, e.g., San Francisco, CA"
            }
        },
        "required": ["location"]
    })));

    // 3. Create the worker with the tool
    let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;
    let worker = LlmWorker::<WeatherReport>::builder(llm)
        .with_system_instructions("You are a friendly weather assistant.")
        .with_tool(weather_tool)
        .build();

    // 4. Run the worker
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
use radkit::tools::{BaseTool, FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TravelPlan {
    destination: String,
    weather_summary: String,
    hotel_recommendation: String,
    estimated_cost: f64,
}

// Define the tools (simplified for brevity)
let weather_tool = Arc::new(FunctionTool::new(
    "get_weather", "Get weather forecast", |_, _| {
        Box::pin(async { ToolResult::success(json!({"forecast": "Sunny and 75°F"})) })
    },
)) as Arc<dyn BaseTool>;

let hotel_tool = Arc::new(FunctionTool::new(
    "search_hotels", "Search for hotels", |_, _| {
        Box::pin(async { ToolResult::success(json!({"hotel": "The Grand Hotel"})) })
    },
)) as Arc<dyn BaseTool>;

let cost_tool = Arc::new(FunctionTool::new(
    "calculate_cost", "Calculate trip cost", |_, _| {
        Box::pin(async { ToolResult::success(json!({"cost": 1250.0})) })
    },
)) as Arc<dyn BaseTool>;


let llm = AnthropicLlm::from_env("claude-3-sonnet-20240229")?;

// Build the worker with multiple tools
let worker = LlmWorker::<TravelPlan>::builder(llm)
    .with_system_instructions("You are a helpful travel planning assistant.")
    .with_tools(vec![weather_tool, hotel_tool, cost_tool])
    .build();

// Run the worker
let plan = worker.run("Plan a 3-day trip to Tokyo").await?;

println!("🗺️ {}", plan.destination);
println!("🌤️ {}", plan.weather_summary);
println!("🏨 {}", plan.hotel_recommendation);
println!("💰 Estimated cost: ${:.2}", plan.estimated_cost);
```

When you run this, the `LlmWorker` will orchestrate multiple calls to the LLM and your tools to gather all the necessary information before producing the final `TravelPlan`.

For tools that need to maintain state across multiple calls (like a shopping cart), see the **[Stateful Tools](./stateful-tools.md)** guide.