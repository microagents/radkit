//! Example demonstrating LlmWorker with tool execution.
//!
//! This example shows how to create an LLM agent that can automatically
//! call tools to accomplish tasks.
//!
//! Run with: cargo run --example worker_with_tools

use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CalculationResult {
    expression: String,
    result: f64,
    explanation: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create calculator tool
    let calculator_tool = Arc::new(
        FunctionTool::new(
            "calculate",
            "Performs mathematical calculations. Supports +, -, *, /",
            |args, _ctx| {
                Box::pin(async move {
                    let expression = args
                        .get("expression")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // Simple calculator logic (in production, use a proper parser)
                    let result = if expression.contains("+") {
                        let parts: Vec<&str> = expression.split('+').collect();
                        if parts.len() == 2 {
                            let a: f64 = parts[0].trim().parse().unwrap_or(0.0);
                            let b: f64 = parts[1].trim().parse().unwrap_or(0.0);
                            a + b
                        } else {
                            0.0
                        }
                    } else if expression.contains("*") {
                        let parts: Vec<&str> = expression.split('*').collect();
                        if parts.len() == 2 {
                            let a: f64 = parts[0].trim().parse().unwrap_or(0.0);
                            let b: f64 = parts[1].trim().parse().unwrap_or(0.0);
                            a * b
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    ToolResult::success(json!({
                        "result": result,
                        "expression": expression
                    }))
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "Mathematical expression to calculate (e.g., '5 + 3' or '10 * 2')"
                }
            },
            "required": ["expression"]
        })),
    );

    // Create LLM client
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // Create worker with tool
    let worker = LlmWorker::<CalculationResult>::builder(llm)
        .with_system_instructions(
            "You are a helpful math assistant. Use the calculator tool to perform calculations.",
        )
        .with_tool(calculator_tool)
        .build();

    // Run - LLM will automatically call the calculator tool
    println!("🤖 Asking worker to calculate...\n");
    let result = worker.run("What is 25 times 4? Show your work.").await?;

    println!("📊 Expression: {}", result.expression);
    println!("🔢 Result: {}", result.result);
    println!("💡 Explanation: {}", result.explanation);

    Ok(())
}
