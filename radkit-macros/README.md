# radkit-macros

Procedural macros for the radkit agent framework.

## Overview

This crate provides the `#[skill]` attribute macro for defining A2A-compliant agent skills with automatic metadata generation.

## Usage

Add the macro to your skill struct and it will automatically generate the required `SkillMetadata` and implement the `RegisteredSkill` trait:

```rust
use radkit::prelude::*;
use radkit_macro::skill;

#[skill(
    id = "weather_checker",
    name = "Weather Checker",
    description = "Fetches weather information for any location",
    tags = ["weather", "api", "location"],
    examples = [
        "What's the weather in London?",
        "Check the forecast for Tokyo"
    ],
    input_modes = ["text/plain"],
    output_modes = ["application/json", "text/plain"]
)]
pub struct WeatherSkill;

#[async_trait]
impl SkillHandler for WeatherSkill {
    async fn on_request(
        &self,
        task_context: &mut TaskContext,
        context: &Context,
        runtime: &dyn Runtime,
        content: Content,
    ) -> Result<OnRequestResult, AgentError> {
        // Your skill implementation here
        Ok(OnRequestResult::Completed {
            message: Some(Content::from_text("Weather data retrieved")),
            artifacts: vec![],
        })
    }
}
```

## Required Parameters

- `id`: A unique identifier for the skill (String)
- `name`: A human-readable name for the skill (String)
- `description`: A detailed description of what the skill does (String)

## Optional Parameters

- `tags`: Array of keywords describing the skill's capabilities (default: [])
- `examples`: Array of example prompts or scenarios (default: [])
- `input_modes`: Array of supported input MIME types (default: [])
- `output_modes`: Array of supported output MIME types (default: [])

## MIME Type Validation

The macro validates `input_modes` and `output_modes` against a list of common MIME types:

- **Text types**: `text/plain`, `text/html`, `text/markdown`, `text/csv`, etc.
- **Application types**: `application/json`, `application/xml`, `application/yaml`, `application/pdf`, etc.
- **Image types**: `image/png`, `image/jpeg`, `image/svg+xml`, etc.
- **Wildcards**: `*/*`, `text/*`, `application/*`, `image/*`

If an invalid MIME type is provided, you'll get a compile error with suggestions for similar valid types.

## What Gets Generated

The macro generates:

1. A static `SkillMetadata` constant named `{STRUCT_NAME}_METADATA`
2. An implementation of the `RegisteredSkill` trait for your struct

This allows you to register the skill with an agent builder:

```rust
let agent = AgentBuilder::new()
    .with_skill(WeatherSkill)
    .build(runtime)?;
```

## Integration with A2A Protocol

The generated metadata is used to create the `AgentSkill` entries in your agent's AgentCard, making your skills discoverable and callable via the A2A protocol.

## License

MIT
