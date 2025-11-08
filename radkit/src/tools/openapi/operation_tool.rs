//! OpenAPI Operation Tool - Individual tool for each API operation
//!
//! This module implements BaseTool for individual OpenAPI operations.

use crate::tools::openapi::{AuthConfig, HeaderOrQuery, OpenApiSpec};
use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult};
use openapiv3::{Operation, Parameter, ParameterSchemaOrContent, ReferenceOr, SchemaKind, Type};
use std::collections::HashMap;
use std::sync::Arc;

/// Individual tool for a single OpenAPI operation
///
/// Each operation in the OpenAPI spec gets its own tool with typed parameters.
pub struct OpenApiOperationTool {
    /// Operation ID (tool name)
    operation_id: String,
    /// Operation description
    description: String,
    /// HTTP method for this operation
    method: String,
    /// Path for this operation
    path: String,
    /// Reference to the OpenAPI spec
    spec: Arc<OpenApiSpec>,
    /// Shared HTTP client
    http_client: Arc<reqwest::Client>,
    /// Authentication configuration
    auth: Option<AuthConfig>,
}

/// Helper function to convert JSON value to string
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        _ => value.to_string(),
    }
}

/// Percent-encode a string for use in URL path parameters
///
/// This encodes all characters except unreserved characters (A-Z, a-z, 0-9, -, _, ., ~)
/// according to RFC 3986. This prevents special URL characters like /, ?, #, % from
/// breaking the URL structure or causing security issues.
fn percent_encode_path_param(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            // Unreserved characters per RFC 3986 - do not encode
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            // Encode everything else
            _ => {
                // Convert char to UTF-8 bytes and percent-encode each byte
                c.to_string()
                    .bytes()
                    .map(|b| format!("%{:02X}", b))
                    .collect()
            }
        })
        .collect()
}

/// Convert OpenAPI Schema to JSON Schema
fn convert_schema_to_json_schema(schema: &openapiv3::Schema) -> serde_json::Value {
    let mut json_schema = serde_json::json!({});

    match &schema.schema_kind {
        SchemaKind::Type(Type::String(string_type)) => {
            json_schema["type"] = serde_json::json!("string");
            if !string_type.enumeration.is_empty() {
                let enum_values: Vec<&str> = string_type
                    .enumeration
                    .iter()
                    .filter_map(|v| v.as_deref())
                    .collect();
                if !enum_values.is_empty() {
                    json_schema["enum"] = serde_json::json!(enum_values);
                }
            }
        }
        SchemaKind::Type(Type::Number(_)) => {
            json_schema["type"] = serde_json::json!("number");
        }
        SchemaKind::Type(Type::Integer(_)) => {
            json_schema["type"] = serde_json::json!("integer");
        }
        SchemaKind::Type(Type::Boolean(_)) => {
            json_schema["type"] = serde_json::json!("boolean");
        }
        SchemaKind::Type(Type::Array(array_type)) => {
            json_schema["type"] = serde_json::json!("array");
            if let Some(items) = &array_type.items {
                match items {
                    ReferenceOr::Item(schema) => {
                        json_schema["items"] = convert_schema_to_json_schema(schema);
                    }
                    ReferenceOr::Reference { reference } => {
                        // Keep the $ref as is for now
                        json_schema["items"] = serde_json::json!({"$ref": reference});
                    }
                }
            }
        }
        SchemaKind::Type(Type::Object(_)) => {
            json_schema["type"] = serde_json::json!("object");
        }
        _ => {
            // For complex types or references, use a generic object
            json_schema["type"] = serde_json::json!("object");
        }
    }

    // Add description if available
    if let Some(description) = &schema.schema_data.description {
        json_schema["description"] = serde_json::json!(description);
    }

    json_schema
}

impl OpenApiOperationTool {
    /// Create a new operation tool
    pub fn new(
        operation_id: String,
        description: String,
        method: String,
        path: String,
        spec: Arc<OpenApiSpec>,
        http_client: Arc<reqwest::Client>,
        auth: Option<AuthConfig>,
    ) -> Self {
        Self {
            operation_id,
            description,
            method,
            path,
            spec,
            http_client,
            auth,
        }
    }

    /// Find this operation in the OpenAPI spec
    /// Uses the stored path and method for direct lookup
    fn find_operation(&self) -> Option<(String, String, &Operation)> {
        // Get the path item
        let path_item_ref = self.spec.spec().paths.paths.get(&self.path)?;
        let path_item = match path_item_ref {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => return None,
        };

        // Get the operation for this method
        let operation = match self.method.to_uppercase().as_str() {
            "GET" => path_item.get.as_ref()?,
            "POST" => path_item.post.as_ref()?,
            "PUT" => path_item.put.as_ref()?,
            "DELETE" => path_item.delete.as_ref()?,
            "PATCH" => path_item.patch.as_ref()?,
            "HEAD" => path_item.head.as_ref()?,
            "OPTIONS" => path_item.options.as_ref()?,
            "TRACE" => path_item.trace.as_ref()?,
            _ => return None,
        };

        Some((self.method.clone(), self.path.clone(), operation))
    }

    /// Build the full URL with path parameters
    fn build_url(&self, path: &str, args: &HashMap<String, serde_json::Value>) -> String {
        let mut url = format!("{}{}", self.spec.base_url(), path);

        // Replace path parameters like {petId} with actual values
        // Use percent-encoding to prevent special characters from breaking the URL structure
        for (key, value) in args {
            let placeholder = format!("{{{}}}", key);
            if url.contains(&placeholder) {
                let value_str = value_to_string(value);
                // Percent-encode the value to handle special URL characters (/, ?, #, %, etc.)
                let encoded_value = percent_encode_path_param(&value_str);
                url = url.replace(&placeholder, &encoded_value);
            }
        }

        url
    }

    /// Extract parameters from operation definition
    fn extract_parameters(
        &self,
        operation: &Operation,
        args: &HashMap<String, serde_json::Value>,
    ) -> (
        HashMap<String, String>,
        HashMap<String, String>,
        HashMap<String, String>,
    ) {
        let mut query_params = HashMap::new();
        let mut header_params = HashMap::new();
        let mut cookie_params = HashMap::new();

        for param_ref in &operation.parameters {
            let param = match param_ref {
                ReferenceOr::Item(p) => p,
                // TODO(Phase 2): Resolve $ref parameter references
                ReferenceOr::Reference { .. } => continue,
            };

            match param {
                Parameter::Query { parameter_data, .. } => {
                    if let Some(value) = args.get(&parameter_data.name) {
                        query_params.insert(parameter_data.name.clone(), value_to_string(value));
                    }
                }
                Parameter::Header { parameter_data, .. } => {
                    if let Some(value) = args.get(&parameter_data.name) {
                        header_params.insert(parameter_data.name.clone(), value_to_string(value));
                    }
                }
                Parameter::Path { .. } => {
                    // Path parameters are handled in build_url
                }
                Parameter::Cookie { parameter_data, .. } => {
                    if let Some(value) = args.get(&parameter_data.name) {
                        cookie_params.insert(parameter_data.name.clone(), value_to_string(value));
                    }
                }
            }
        }

        (query_params, header_params, cookie_params)
    }
}

#[cfg_attr(all(target_os = "wasi", target_env = "p1"), async_trait::async_trait(?Send))]
#[cfg_attr(
    not(all(target_os = "wasi", target_env = "p1")),
    async_trait::async_trait
)]
impl BaseTool for OpenApiOperationTool {
    fn name(&self) -> &str {
        &self.operation_id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn declaration(&self) -> FunctionDeclaration {
        // Find the operation to extract parameter schema
        let (_, _, operation) = match self.find_operation() {
            Some(result) => result,
            None => {
                // Fallback to empty schema if operation not found
                return FunctionDeclaration::new(
                    self.operation_id.clone(),
                    self.description.clone(),
                    serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    }),
                );
            }
        };

        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Extract parameter schemas
        for param_ref in &operation.parameters {
            let param_data = match param_ref {
                ReferenceOr::Item(p) => match p {
                    Parameter::Query { parameter_data, .. }
                    | Parameter::Header { parameter_data, .. }
                    | Parameter::Path { parameter_data, .. }
                    | Parameter::Cookie { parameter_data, .. } => parameter_data,
                },
                // For $ref parameters, we'll skip for now but could resolve later
                ReferenceOr::Reference { .. } => continue,
            };

            // Convert schema
            let param_schema = match &param_data.format {
                ParameterSchemaOrContent::Schema(schema_ref) => match schema_ref {
                    ReferenceOr::Item(schema) => convert_schema_to_json_schema(schema),
                    ReferenceOr::Reference { reference } => {
                        serde_json::json!({"$ref": reference})
                    }
                },
                ParameterSchemaOrContent::Content(_) => {
                    // Content is more complex, default to object for now
                    let mut default_schema = serde_json::json!({"type": "object"});
                    if let Some(desc) = &param_data.description {
                        default_schema["description"] = serde_json::json!(desc);
                    }
                    default_schema
                }
            };

            properties.insert(param_data.name.clone(), param_schema);

            // Mark as required
            if param_data.required {
                required.push(param_data.name.clone());
            }
        }

        // Add request body as "body" parameter for methods that support it
        if let Some(request_body) = &operation.request_body {
            let body_required = match request_body {
                ReferenceOr::Item(rb) => rb.required,
                ReferenceOr::Reference { .. } => false,
            };

            properties.insert(
                "body".to_string(),
                serde_json::json!({
                    "type": "object",
                    "description": "Request body"
                }),
            );

            if body_required {
                required.push("body".to_string());
            }
        }

        FunctionDeclaration::new(
            self.operation_id.clone(),
            self.description.clone(),
            serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required
            }),
        )
    }

    async fn run_async(
        &self,
        args: HashMap<String, serde_json::Value>,
        _context: &ToolContext<'_>,
    ) -> ToolResult {
        // Find the operation in the spec
        let (method, path, operation) = match self.find_operation() {
            Some(result) => result,
            None => {
                return ToolResult::error(format!(
                    "Operation '{}' not found in OpenAPI spec",
                    self.operation_id
                ));
            }
        };

        // Extract parameters
        let (query_params, header_params, cookie_params) =
            self.extract_parameters(operation, &args);

        // Build URL with path parameters
        let url = self.build_url(&path, &args);

        // Build the HTTP request
        let mut request_builder = match method.as_str() {
            "GET" => self.http_client.get(&url),
            "POST" => self.http_client.post(&url),
            "PUT" => self.http_client.put(&url),
            "DELETE" => self.http_client.delete(&url),
            "PATCH" => self.http_client.patch(&url),
            "HEAD" => self.http_client.head(&url),
            _ => {
                return ToolResult::error(format!("Unsupported HTTP method: {}", method));
            }
        };

        // Add query parameters
        if !query_params.is_empty() {
            request_builder = request_builder.query(&query_params);
        }

        // Add header parameters
        for (name, value) in header_params {
            request_builder = request_builder.header(name, value);
        }

        // Add cookie parameters
        if !cookie_params.is_empty() {
            let cookie_header = cookie_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            request_builder = request_builder.header("Cookie", cookie_header);
        }

        // Add authentication
        if let Some(auth) = &self.auth {
            request_builder = match auth {
                AuthConfig::Basic { username, password } => {
                    request_builder.basic_auth(username, Some(password))
                }
                AuthConfig::ApiKey {
                    location: HeaderOrQuery::Header,
                    name,
                    value,
                } => request_builder.header(name, value),
                AuthConfig::ApiKey {
                    location: HeaderOrQuery::Query,
                    name,
                    value,
                } => request_builder.query(&[(name, value)]),
            };
        }

        // Add request body for POST/PUT/PATCH
        if matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
            if let Some(body) = args.get("body") {
                request_builder = request_builder
                    .header("Content-Type", "application/json")
                    .json(body);
            }
        }

        // Execute the request
        let response = match request_builder.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return ToolResult::error(format!("HTTP request failed: {}", e));
            }
        };

        let status = response.status();
        let status_code = status.as_u16();

        // Read response body
        let body_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                return ToolResult::error(format!("Failed to read response body: {}", e));
            }
        };

        // Try to parse as JSON, fallback to plain text
        let result_value = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
            serde_json::json!({
                "status": status_code,
                "body": json,
            })
        } else {
            serde_json::json!({
                "status": status_code,
                "body": body_text,
            })
        };

        // Check if the request was successful
        if status.is_success() {
            ToolResult::success(result_value)
        } else {
            ToolResult::error(format!(
                "HTTP {} {}: {}",
                status_code,
                status.canonical_reason().unwrap_or("Unknown"),
                body_text
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_build_url_with_special_characters() {
        // Create a minimal OpenApiSpec for testing
        let spec_json = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/pets/{petId}": {
                    "get": {
                        "operationId": "test_op",
                        "responses": {
                            "200": {"description": "Success"}
                        }
                    }
                }
            }
        }"#;
        let openapi_spec = Arc::new(
            OpenApiSpec::from_str(spec_json, "https://api.example.com".to_string()).unwrap(),
        );

        let http_client = Arc::new(reqwest::Client::new());

        let tool = OpenApiOperationTool::new(
            "test_op".to_string(),
            "Test operation".to_string(),
            "GET".to_string(),
            "/pets/{petId}".to_string(),
            openapi_spec,
            http_client,
            None,
        );

        // Test with normal value
        let mut args = HashMap::new();
        args.insert("petId".to_string(), serde_json::json!("123"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/123");

        // Test with value containing slash (should be encoded as %2F)
        args.insert("petId".to_string(), serde_json::json!("foo/bar"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%2Fbar");

        // Test with value containing question mark (should be encoded as %3F)
        args.insert("petId".to_string(), serde_json::json!("foo?bar"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%3Fbar");

        // Test with value containing hash (should be encoded as %23)
        args.insert("petId".to_string(), serde_json::json!("foo#bar"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%23bar");

        // Test with value containing percent (should be encoded as %25)
        args.insert("petId".to_string(), serde_json::json!("foo%bar"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%25bar");

        // Test with value containing spaces (should be encoded as %20)
        args.insert("petId".to_string(), serde_json::json!("foo bar"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%20bar");

        // Test with multiple special characters
        args.insert("petId".to_string(), serde_json::json!("foo/bar?baz#qux"));
        let url = tool.build_url("/pets/{petId}", &args);
        assert_eq!(url, "https://api.example.com/pets/foo%2Fbar%3Fbaz%23qux");
    }

    #[test]
    fn test_build_url_with_multiple_path_params() {
        // Create a minimal OpenApiSpec for testing
        let spec_json = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/users/{userId}/pets/{petId}": {
                    "get": {
                        "operationId": "test_op",
                        "responses": {
                            "200": {"description": "Success"}
                        }
                    }
                }
            }
        }"#;
        let openapi_spec = Arc::new(
            OpenApiSpec::from_str(spec_json, "https://api.example.com".to_string()).unwrap(),
        );

        let http_client = Arc::new(reqwest::Client::new());

        let tool = OpenApiOperationTool::new(
            "test_op".to_string(),
            "Test operation".to_string(),
            "GET".to_string(),
            "/users/{userId}/pets/{petId}".to_string(),
            openapi_spec,
            http_client,
            None,
        );

        let mut args = HashMap::new();
        args.insert("userId".to_string(), serde_json::json!("user/123"));
        args.insert("petId".to_string(), serde_json::json!("pet?456"));

        let url = tool.build_url("/users/{userId}/pets/{petId}", &args);
        assert_eq!(
            url,
            "https://api.example.com/users/user%2F123/pets/pet%3F456"
        );
    }
}
