---
title: Stateful Tools
description: Build tools that remember information across multiple calls within a conversation.
---



Sometimes, a tool needs to remember information across multiple calls within the same conversation. A classic example is a shopping cart where you add items one by one.

Radkit supports this by passing a `ToolContext` to every tool call. You can use this context to get and set state that persists for the duration of a single `LlmWorker` run.

## How it Works

The `ToolContext` provides access to an `ExecutionState` object, which is a simple key-value store.

-   `ctx.state().set_state("my_key", json!(value))`: Saves a `serde_json::Value` to the store.
-   `ctx.state().get_state("my_key")`: Retrieves a value from the store.

This state is **not** persisted across different `worker.run()` calls. It is temporary state for a single, multi-turn tool execution sequence.

## Example: A Shopping Cart

Let's build a shopping cart tool. The `add_to_cart` tool will be called multiple times by the LLM, and it will use the `ToolContext` to keep track of the items and the total price.

```rust
use radkit::agent::LlmWorker;
use radkit::models::providers::AnthropicLlm;
use radkit::tools::{tool, ToolContext, ToolResult};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

// The final output we want
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ShoppingCart {
    items: Vec<String>,
    total_items: u32,
    estimated_total: f64,
}

// Define tool arguments
#[derive(Deserialize, JsonSchema)]
struct AddToCartArgs {
    /// Item name to add
    item: String,
    /// Price of the item
    price: f64,
}

// The tool that adds items to the cart
#[tool(description = "Add an item to the shopping cart")]
async fn add_to_cart(args: AddToCartArgs, ctx: ToolContext) -> ToolResult {
    // 1. Get current state from the context
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

    // 2. Update the state
    items.push(args.item.clone());
    let new_total = total_price + args.price;

    // 3. Set the new state back into the context
    ctx.state().set_state("items", json!(items));
    ctx.state().set_state("total_price", json!(new_total));

    // 4. Return the result of this specific action
    ToolResult::success(json!({
        "item_added": args.item,
        "cart_size": items.len(),
        "current_total": new_total
    }))
}

let llm = AnthropicLlm::from_env("claude-sonnet-4-5-20250929")?;

let worker = LlmWorker::<ShoppingCart>::builder(llm)
    .with_system_instructions("You are a shopping assistant. First, add all items to the cart, then provide the final cart summary.")
    .with_tool(add_to_cart)
    .build();

// The worker will call the tool multiple times to fulfill the request
let cart = worker.run("Please add a laptop for $999 and a mouse for $25 to my cart.").await?;

println!("🛒 Final Cart Summary:");
for item in &cart.items {
    println!("  - {}", item);
}
println!("📦 Total items: {}", cart.total_items);
println!("💵 Estimated total: ${:.2}", cart.estimated_total);
```

### Execution Flow

1.  The user asks to add two items.
2.  The LLM sees the `add_to_cart` tool and calls it for the laptop.
3.  Your tool code executes, adds `"laptop"` to a new `items` vector, sets the `total_price` to `999.0`, and saves both to the `ToolContext` state. It returns a success message.
4.  The `LlmWorker` sends this result back to the LLM. The LLM sees that it still needs to add the mouse.
5.  The LLM calls `add_to_cart` again, this time for the mouse.
6.  Your tool code executes again. It retrieves the *existing* `items` and `total_price` from the context, adds the mouse, updates the totals, and saves the new state.
7.  The `LlmWorker` sends this new result back to the LLM. The LLM now sees that all tasks are complete.
8.  Finally, the LLM generates the final `ShoppingCart` JSON object, which the `LlmWorker` parses and returns to you.