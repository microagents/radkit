//! Helpers for structured LLM outputs via the radkit tool protocol.

use std::any::type_name;
use std::collections::HashMap;
use std::sync::Arc;

use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::compat::{MaybeSend, MaybeSync};
use crate::errors::{AgentError, AgentResult};
use crate::models::{Content, ContentPart};
use crate::tools::{
    BaseTool, BaseToolset, FunctionDeclaration, SimpleToolset, ToolContext, ToolResult,
};

pub const STRUCTURED_OUTPUT_TOOL_NAME: &str = "radkit_structured_output";

pub fn build_structured_output_toolset<T>() -> AgentResult<Arc<dyn BaseToolset>>
where
    T: JsonSchema + MaybeSend + MaybeSync + 'static,
{
    let tool: Arc<dyn BaseTool> = Arc::new(StructuredOutputTool::<T>::new()?);
    Ok(Arc::new(SimpleToolset::new(vec![tool])))
}

pub fn extract_structured_output<T>(content: Content) -> AgentResult<(T, Option<Content>)>
where
    T: DeserializeOwned + JsonSchema + MaybeSend + MaybeSync + 'static,
{
    let mut structured_payload: Option<Value> = None;
    let mut filtered_parts = Vec::new();

    for part in content.into_parts() {
        match part {
            ContentPart::ToolCall(call) if call.name() == STRUCTURED_OUTPUT_TOOL_NAME => {
                if structured_payload.is_some() {
                    return Err(AgentError::Validation {
                        field: "structured_output".to_string(),
                        reason: "LLM returned multiple structured output tool calls".to_string(),
                    });
                }
                let (_, _, arguments) = call.into_parts();
                structured_payload = Some(arguments);
            }
            other => filtered_parts.push(other),
        }
    }

    let payload = structured_payload.ok_or_else(|| AgentError::Validation {
        field: "structured_output".to_string(),
        reason: "LLM response did not include the structured output tool call".to_string(),
    })?;

    let value: T = serde_json::from_value(payload).map_err(|err| AgentError::Serialization {
        format: "json".to_string(),
        reason: err.to_string(),
    })?;

    let assistant_content = if filtered_parts.is_empty() {
        None
    } else {
        Some(Content::from_parts(filtered_parts))
    };

    Ok((value, assistant_content))
}

struct StructuredOutputTool<T> {
    name: String,
    description: String,
    parameters: Value,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> StructuredOutputTool<T>
where
    T: JsonSchema + MaybeSend + MaybeSync + 'static,
{
    fn new() -> AgentResult<Self> {
        let root = schema_for!(T);
        let mut schema_value =
            serde_json::to_value(&root.schema).map_err(|err| AgentError::Serialization {
                format: "json_schema".to_string(),
                reason: err.to_string(),
            })?;

        if !root.definitions.is_empty() {
            if let Value::Object(ref mut map) = schema_value {
                map.insert(
                    "definitions".to_string(),
                    serde_json::to_value(&root.definitions).map_err(|err| {
                        AgentError::Serialization {
                            format: "json_schema".to_string(),
                            reason: err.to_string(),
                        }
                    })?,
                );
            }
        }

        let description = format!(
            "Return the final structured result for `{}` by calling this tool exactly once with arguments that match the JSON schema.",
            type_name::<T>()
        );

        Ok(Self {
            name: STRUCTURED_OUTPUT_TOOL_NAME.to_string(),
            description,
            parameters: schema_value,
            _phantom: std::marker::PhantomData,
        })
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl<T> BaseTool for StructuredOutputTool<T>
where
    T: JsonSchema + MaybeSend + MaybeSync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> FunctionDeclaration {
        FunctionDeclaration::new(
            self.name.clone(),
            self.description.clone(),
            self.parameters.clone(),
        )
    }

    async fn run_async(
        &self,
        _args: HashMap<String, Value>,
        _context: &ToolContext<'_>,
    ) -> ToolResult {
        ToolResult::error("structured output tool should not be executed directly")
    }
}
