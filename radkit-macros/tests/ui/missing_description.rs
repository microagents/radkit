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

#[tool]  // Missing description attribute
async fn add(args: AddArgs) -> ToolResult {
    ToolResult::success(json!({"sum": args.a + args.b}))
}

fn main() {}
