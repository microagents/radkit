use radkit_macros::tool;
use radkit::tools::{ToolResult, ToolContext};
use serde::Deserialize;
use schemars::JsonSchema;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
struct AddArgs {
    a: i64,
    b: i64,
}

#[tool(description = "Add two numbers")]
async fn add(
    args: AddArgs,
    ctx: &ToolContext<'_>,
    extra: String,  // Too many parameters
) -> ToolResult {
    ToolResult::success(json!({"sum": args.a + args.b}))
}

fn main() {}
