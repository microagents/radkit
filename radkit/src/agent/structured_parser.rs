//! Parser-based structured LLM outputs using tryparse.
//!
//! This module provides an alternative to tool-based structured outputs by instructing
//! the LLM to output JSON directly in its response text, then parsing that text using
//! tryparse to handle messy formatting, markdown wrappers, and type coercion.

use std::any::type_name;

use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::compat::{MaybeSend, MaybeSync};
use crate::errors::{AgentError, AgentResult};
use crate::models::Content;

/// Generates system instructions that include the JSON schema for type T.
///
/// This creates a prompt that tells the LLM to respond with JSON matching
/// the schema. The schema is included inline to guide the LLM's output.
pub fn build_structured_output_instructions<T>() -> AgentResult<String>
where
    T: JsonSchema + MaybeSend + MaybeSync + 'static,
{
    let schema = schema_for!(T);
    let schema_value = schema.to_value();

    let schema_str =
        serde_json::to_string_pretty(&schema_value).map_err(|err| AgentError::Serialization {
            format: "json_schema".to_string(),
            reason: err.to_string(),
        })?;

    let instructions = format!(
        r#"You must respond with a JSON object matching the following schema for type `{}`.

Schema:
```json
{}
```

Rules:
1. Your response MUST be valid JSON matching this exact schema
2. You may wrap the JSON in markdown code blocks (```json ... ```)
3. You may not include explanatory text before or after the JSON
4. All required fields must be present
5. Field types must match the schema

Example response format:
```json
{{
  "field1": "value1",
  "field2": 123
}}
```"#,
        type_name::<T>(),
        schema_str
    );

    Ok(instructions)
}

/// Extracts and deserializes structured output from LLM response content.
///
/// This function uses tryparse to handle messy LLM outputs including:
/// - Markdown code blocks
/// - Trailing commas
/// - Unquoted keys
/// - Type mismatches (e.g., string numbers)
/// - Mixed text and JSON
/// - Fuzzy field name matching (`user_name` matches userName, user-name, etc.)
/// - Fuzzy enum matching (`InProgress` matches "`in_progress`", "inprogress", etc.)
///
/// For types that derive `serde::Deserialize`, this provides basic parsing with
/// automatic type coercion (e.g., "42" -> 42). Tryparse will use its standard
/// deserialization strategy which handles most common cases.
///
/// # Arguments
///
/// * `content` - The content from the LLM response
///
/// # Returns
///
/// Returns the deserialized value of type `T` extracted from the content's text.
///
/// # Errors
///
/// Returns an error if:
/// - No text content is found
/// - Parsing fails even with tryparse's error correction
/// - Deserialization into type T fails
pub fn extract_structured_output<T>(content: Content) -> AgentResult<T>
where
    T: DeserializeOwned + JsonSchema + MaybeSend + MaybeSync + 'static,
{
    // Get all text from the content
    let text = content
        .joined_texts()
        .ok_or_else(|| AgentError::Validation {
            field: "structured_output".to_string(),
            reason: "LLM response did not include any text content".to_string(),
        })?;

    // Use tryparse with standard serde deserialization
    // This handles markdown, broken JSON, and basic type coercion
    tryparse::parse::<T>(&text).map_err(|err| AgentError::Serialization {
        format: "json".to_string(),
        reason: format!(
            "Failed to parse LLM output as {}: {}",
            type_name::<T>(),
            err
        ),
    })
}

/// Alternative version that returns both the parsed value and the raw text.
///
/// Useful for debugging or when you want to preserve the LLM's explanation
/// alongside the structured data.
pub fn extract_structured_output_with_text<T>(content: Content) -> AgentResult<(T, String)>
where
    T: DeserializeOwned + JsonSchema + MaybeSend + MaybeSync + 'static,
{
    let text = content
        .joined_texts()
        .ok_or_else(|| AgentError::Validation {
            field: "structured_output".to_string(),
            reason: "LLM response did not include any text content".to_string(),
        })?;

    let value = tryparse::parse::<T>(&text).map_err(|err| AgentError::Serialization {
        format: "json".to_string(),
        reason: format!(
            "Failed to parse LLM output as {}: {}",
            type_name::<T>(),
            err
        ),
    })?;

    Ok((value, text))
}

/// Helper to generate just the schema as a JSON value.
///
/// Useful if you want to build custom prompts or inspect the schema.
pub fn get_schema_for_type<T>() -> AgentResult<Value>
where
    T: JsonSchema + MaybeSend + MaybeSync + 'static,
{
    let schema = schema_for!(T);
    Ok(schema.to_value())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Content;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Deserialize, JsonSchema)]
    struct Sample {
        value: i32,
    }

    #[test]
    fn build_instructions_includes_schema() {
        let instructions =
            build_structured_output_instructions::<Sample>().expect("should generate instructions");

        assert!(instructions.contains("Sample"));
        assert!(instructions.contains("schema"));
        assert!(instructions.contains("JSON"));
        assert!(instructions.contains("value"));
    }

    #[test]
    fn extract_from_clean_json() {
        let content = Content::from_text(r#"{"value": 42}"#);
        let result = extract_structured_output::<Sample>(content).expect("should parse clean JSON");

        assert_eq!(result, Sample { value: 42 });
    }

    #[test]
    fn extract_from_markdown_wrapped_json() {
        let content = Content::from_text(
            r#"Here's your result:
```json
{
  "value": 123
}
```"#,
        );
        let result = extract_structured_output::<Sample>(content)
            .expect("should parse markdown-wrapped JSON");

        assert_eq!(result, Sample { value: 123 });
    }

    #[test]
    fn extract_from_messy_json() {
        // Trailing comma, unquoted key, string number
        let content = Content::from_text(
            r#"{
  value: "789",
}"#,
        );
        let result = extract_structured_output::<Sample>(content).expect("should parse messy JSON");

        assert_eq!(result, Sample { value: 789 });
    }

    #[test]
    fn extract_with_text_preserves_original() {
        let original = "The answer is:\n```json\n{\"value\": 42}\n```";
        let content = Content::from_text(original);

        let (result, text) = extract_structured_output_with_text::<Sample>(content)
            .expect("should parse and preserve text");

        assert_eq!(result, Sample { value: 42 });
        assert_eq!(text, original);
    }

    #[test]
    fn extract_without_text_fails() {
        let content = Content::from_parts(vec![]);
        let error =
            extract_structured_output::<Sample>(content).expect_err("should fail without text");

        match error {
            AgentError::Validation { field, .. } => assert_eq!(field, "structured_output"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn get_schema_returns_valid_schema() {
        let schema = get_schema_for_type::<Sample>().expect("should get schema");

        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        assert!(obj.contains_key("type"));
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }
}
