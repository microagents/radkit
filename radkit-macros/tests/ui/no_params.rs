use radkit_macros::tool;
use radkit::tools::ToolResult;
use serde_json::json;

#[tool(description = "No parameters")]
async fn no_args() -> ToolResult {  // No parameters
    ToolResult::success(json!({"result": "ok"}))
}

fn main() {}
