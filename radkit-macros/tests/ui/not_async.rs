use radkit_macros::tool;
use radkit::tools::ToolResult;
use serde::Deserialize;
use schemars::JsonSchema;
use serde_json::json;

#[derive(Deserialize, JsonSchema)]
struct AddArgs {
    a: i64,
    b: i64,
}

#[tool(description = "Add two numbers")]
fn add(args: AddArgs) -> ToolResult {  // Not async
    ToolResult::success(json!({"sum": args.a + args.b}))
}

fn main() {}
