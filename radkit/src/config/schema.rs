use crate::errors::LoaderError;
use jsonschema::{Resource, Validator};
use once_cell::sync::Lazy;
use serde_json::Value;

/// JSON Schema for agent configuration validation
static AGENT_SCHEMA: Lazy<Validator> = Lazy::new(|| {
    // Use a blocking context to initialize the validator
    std::thread::spawn(move || {
        let agent_schema: Value = serde_json::from_str(include_str!("schema/agent.json"))
            .expect("Failed to parse embedded agent schema");

        let resources = [
            (
                "urn:model-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/model-schema.json"))
                        .expect("Failed to parse embedded model schema"),
                )
                .expect("Failed to create model schema resource"),
            ),
            (
                "urn:mcp-tool-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/mcp-tool-schema.json"))
                        .expect("Failed to parse embedded mcp-tool schema"),
                )
                .expect("Failed to create mcp-tool schema resource"),
            ),
            (
                "urn:mcp-stdio-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/mcp-stdio-schema.json"))
                        .expect("Failed to parse embedded mcp-stdio schema"),
                )
                .expect("Failed to create mcp-stdio schema resource"),
            ),
            (
                "urn:function-tool-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/function-tool-schema.json"))
                        .expect("Failed to parse embedded function-tool schema"),
                )
                .expect("Failed to create function-tool schema resource"),
            ),
            (
                "urn:builtin-tool-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/builtin-tool-schema.json"))
                        .expect("Failed to parse embedded builtin-tool schema"),
                )
                .expect("Failed to create builtin-tool schema resource"),
            ),
            (
                "urn:instruction-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/instruction-schema.json"))
                        .expect("Failed to parse embedded instruction schema"),
                )
                .expect("Failed to create instruction schema resource"),
            ),
            (
                "urn:a2a-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/a2a.json"))
                        .expect("Failed to parse embedded a2a schema"),
                )
                .expect("Failed to create a2a schema resource"),
            ),
            (
                "urn:remote-agent-schema",
                Resource::from_contents(
                    serde_json::from_str(include_str!("schema/remote-agent-schema.json"))
                        .expect("Failed to parse embedded remote-agent schema"),
                )
                .expect("Failed to create remote-agent schema resource"),
            ),
        ];

        jsonschema::options()
            .with_resources(resources.into_iter())
            .build(&agent_schema)
            .expect("Failed to compile agent schema")
    })
    .join()
    .expect("Failed to initialize schema validator in blocking thread")
});

/// Validate configuration against JSON schema
pub fn validate_config(config: &Value) -> Result<(), LoaderError> {
    let result = AGENT_SCHEMA.validate(config);

    if let Err(error) = result {
        return Err(LoaderError::validation(format!(
            "Schema validation failed: {error}"
        )));
    }

    Ok(())
}

/// Get the JSON schema as a Value
pub fn get_schema() -> &'static Value {
    static SCHEMA_VALUE: Lazy<Value> = Lazy::new(|| {
        let schema_str = include_str!("schema/agent.json");
        serde_json::from_str(schema_str).expect("Failed to parse embedded agent schema")
    });

    &SCHEMA_VALUE
}

/// Get the JSON schema as a string
pub fn get_schema_string() -> &'static str {
    include_str!("schema/agent.json")
}
