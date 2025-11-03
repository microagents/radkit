//! Stateful shopping cart agent example.
//!
//! This example demonstrates how tools can maintain state across
//! multiple calls using the ExecutionState.
//!
//! Run with: cargo run --example stateful_shopping_cart

use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{FunctionTool, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ShoppingCartSummary {
    items: Vec<String>,
    total_items: u32,
    total_price: f64,
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Add item to cart tool
    let add_item_tool = Arc::new(
        FunctionTool::new(
            "add_to_cart",
            "Add an item to the shopping cart",
            |args, ctx| {
                Box::pin(async move {
                    let item = args.get("item").and_then(|v| v.as_str()).unwrap_or("");
                    let price = args.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);

                    // Get current cart state
                    let mut items: Vec<String> = ctx
                        .state()
                        .get_state("items")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default();

                    let total_price: f64 = ctx
                        .state()
                        .get_state("total_price")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);

                    // Add item
                    items.push(item.to_string());
                    let new_total = total_price + price;

                    // Update state
                    ctx.state().set_state("items", json!(items));
                    ctx.state().set_state("total_price", json!(new_total));

                    ToolResult::success(json!({
                        "item_added": item,
                        "price": price,
                        "cart_size": items.len(),
                        "total": new_total
                    }))
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "item": {
                    "type": "string",
                    "description": "Name of the item to add"
                },
                "price": {
                    "type": "number",
                    "description": "Price of the item in USD"
                }
            },
            "required": ["item", "price"]
        })),
    );

    // Remove item from cart tool
    let remove_item_tool = Arc::new(
        FunctionTool::new(
            "remove_from_cart",
            "Remove an item from the shopping cart",
            |args, ctx| {
                Box::pin(async move {
                    let item = args.get("item").and_then(|v| v.as_str()).unwrap_or("");

                    // Get current cart state
                    let mut items: Vec<String> = ctx
                        .state()
                        .get_state("items")
                        .and_then(|v| serde_json::from_value(v).ok())
                        .unwrap_or_default();

                    // Remove first occurrence of item
                    if let Some(pos) = items.iter().position(|x| x == item) {
                        items.remove(pos);
                        ctx.state().set_state("items", json!(items));
                        ToolResult::success(json!({
                            "removed": item,
                            "cart_size": items.len()
                        }))
                    } else {
                        ToolResult::error(format!("Item '{}' not found in cart", item))
                    }
                })
            },
        )
        .with_parameters_schema(json!({
            "type": "object",
            "properties": {
                "item": {
                    "type": "string",
                    "description": "Name of the item to remove"
                }
            },
            "required": ["item"]
        })),
    );

    // Create LLM client
    let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

    // Create shopping assistant with stateful tools
    let shopping_assistant = LlmWorker::<ShoppingCartSummary>::builder(llm)
        .with_system_instructions(
            "You are a helpful shopping assistant. Help users manage their shopping cart. \
             Use add_to_cart and remove_from_cart tools to modify the cart.",
        )
        .with_tool(add_item_tool)
        .with_tool(remove_item_tool)
        .with_max_iterations(10)
        .build();

    // Simulate shopping session
    println!("🛒 Shopping Cart Assistant\n");
    println!("{}\n", "=".repeat(60));

    let request = "I want to buy a laptop for $999, a mouse for $25, and a keyboard for $75. \
                   Actually, remove the keyboard. What's in my cart now?";

    println!("👤 User: {}\n", request);

    match shopping_assistant.run(request).await {
        Ok(summary) => {
            println!("🤖 Assistant: {}\n", summary.message);
            println!("📦 Cart Contents:");
            for (i, item) in summary.items.iter().enumerate() {
                println!("   {}. {}", i + 1, item);
            }
            println!("\n📊 Total Items: {}", summary.total_items);
            println!("💰 Total Price: ${:.2}", summary.total_price);
        }
        Err(e) => {
            eprintln!("❌ Error: {}", e);
        }
    }

    println!("\n{}\n", "=".repeat(60));

    Ok(())
}
