//! `OpenAPI` Operation Tool - Individual tool for each API operation
//!
//! This module implements `BaseTool` for individual `OpenAPI` operations.

use crate::tools::openapi::{AuthConfig, HeaderOrQuery, OpenApiSpec};
use crate::tools::{BaseTool, FunctionDeclaration, ToolContext, ToolResult};
use openapiv3::{Operation, Parameter, ParameterSchemaOrContent, ReferenceOr, SchemaKind, Type};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

type ExtractedParams = (
    Vec<(String, String)>,
    HashMap<String, String>,
    HashMap<String, String>,
);

/// Individual tool for a single `OpenAPI` operation
///
/// Each operation in the `OpenAPI` spec gets its own tool with typed parameters.
pub struct OpenApiOperationTool {
    /// Operation ID (tool name)
    operation_id: String,
    /// Operation description
    description: String,
    /// HTTP method for this operation
    method: String,
    /// Path for this operation
    path: String,
    /// Reference to the `OpenAPI` spec
    spec: Arc<OpenApiSpec>,
    /// Shared HTTP client
    http_client: Arc<reqwest::Client>,
    /// Authentication configuration
    auth: Option<AuthConfig>,
}

/// Helper function to convert JSON value to string (for simple scalar values)
fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        _ => value.to_string(),
    }
}

/// Encode a query parameter value according to `OpenAPI` style and explode settings
///
/// Handles arrays and objects according to the `OpenAPI` specification.
/// Returns a vector of (key, value) pairs to support exploded arrays.
fn encode_query_param(
    name: &str,
    value: &serde_json::Value,
    style: &openapiv3::QueryStyle,
    explode: bool,
) -> Vec<(String, String)> {
    use openapiv3::QueryStyle;

    match value {
        // Arrays are encoded based on style and explode
        serde_json::Value::Array(arr) => {
            let string_values: Vec<String> = arr.iter().map(value_to_string).collect();

            match (style, explode) {
                // Form style with explode=true (default): ?status=available&status=sold
                (QueryStyle::Form, true) => string_values
                    .into_iter()
                    .map(|v| (name.to_string(), v))
                    .collect(),

                // Form style with explode=false: ?status=available,sold
                (QueryStyle::Form, false) | (QueryStyle::DeepObject, _) => {
                    vec![(name.to_string(), string_values.join(","))]
                }

                // Space-delimited: ?status=available%20sold
                (QueryStyle::SpaceDelimited, _) => {
                    vec![(name.to_string(), string_values.join(" "))]
                }

                // Pipe-delimited: ?status=available|sold
                (QueryStyle::PipeDelimited, _) => {
                    vec![(name.to_string(), string_values.join("|"))]
                }
            }
        }

        // Objects with DeepObject style: ?obj[key1]=value1&obj[key2]=value2
        serde_json::Value::Object(obj) if matches!(style, QueryStyle::DeepObject) => obj
            .iter()
            .map(|(key, val)| (format!("{name}[{key}]"), value_to_string(val)))
            .collect(),

        // For other cases (scalars, objects with non-DeepObject style), use simple encoding
        _ => vec![(name.to_string(), value_to_string(value))],
    }
}

/// Encode a header parameter value (headers don't support explode in the same way)
fn encode_header_param(name: &str, value: &serde_json::Value) -> (String, String) {
    match value {
        // Arrays are comma-separated
        serde_json::Value::Array(arr) => {
            let string_values: Vec<String> = arr.iter().map(value_to_string).collect();
            (name.to_string(), string_values.join(","))
        }
        // Everything else converts to string
        _ => (name.to_string(), value_to_string(value)),
    }
}

/// Percent-encode a string for use in URL path parameters
///
/// This encodes all characters except unreserved characters (A-Z, a-z, 0-9, -, _, ., ~)
/// according to RFC 3986. This prevents special URL characters like /, ?, #, % from
/// breaking the URL structure or causing security issues.
fn percent_encode_path_param(s: &str) -> String {
    let mut encoded = String::new();
    for c in s.chars() {
        match c {
            // Unreserved characters per RFC 3986 - do not encode
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => encoded.push(c),
            // Encode everything else
            _ => {
                let mut buf = [0u8; 4];
                for byte in c.encode_utf8(&mut buf).as_bytes() {
                    let _ = write!(&mut encoded, "%{byte:02X}");
                }
            }
        }
    }
    encoded
}

/// Convert `OpenAPI` Schema to JSON Schema
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
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Owned Strings/Arcs require heap allocation, so this cannot be const.
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

    /// Find this operation in the `OpenAPI` spec
    /// Uses the stored path and method for direct lookup
    /// Returns (method, path, `path_item`, operation)
    fn find_operation(&self) -> Option<(String, String, &openapiv3::PathItem, &Operation)> {
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

        Some((self.method.clone(), self.path.clone(), path_item, operation))
    }

    /// Merge path-level and operation-level parameters
    ///
    /// Path-level parameters apply to all operations under that path.
    /// Operation-level parameters can override path-level ones.
    /// A parameter is uniquely identified by (name, location) pair.
    fn merge_parameters<'a>(
        path_params: &'a [ReferenceOr<Parameter>],
        operation_params: &'a [ReferenceOr<Parameter>],
    ) -> Vec<&'a ReferenceOr<Parameter>> {
        let mut merged = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Add operation-level parameters first (they take precedence)
        for param_ref in operation_params {
            if let ReferenceOr::Item(param) = param_ref {
                let key = Self::get_parameter_key(param);
                seen.insert(key);
            }
            merged.push(param_ref);
        }

        // Add path-level parameters that aren't overridden
        for param_ref in path_params {
            if let ReferenceOr::Item(param) = param_ref {
                let key = Self::get_parameter_key(param);
                if !seen.contains(&key) {
                    merged.push(param_ref);
                }
            } else {
                // For references, we can't check uniqueness without resolving,
                // so we include them (may result in duplicates if spec is invalid)
                merged.push(param_ref);
            }
        }

        merged
    }

    /// Get a unique key for a parameter (name + location)
    fn get_parameter_key(param: &Parameter) -> (String, String) {
        let (name, location) = match param {
            Parameter::Query { parameter_data, .. } => {
                (parameter_data.name.clone(), "query".to_string())
            }
            Parameter::Header { parameter_data, .. } => {
                (parameter_data.name.clone(), "header".to_string())
            }
            Parameter::Path { parameter_data, .. } => {
                (parameter_data.name.clone(), "path".to_string())
            }
            Parameter::Cookie { parameter_data, .. } => {
                (parameter_data.name.clone(), "cookie".to_string())
            }
        };
        (name, location)
    }

    /// Build the full URL with path parameters
    fn build_url(&self, path: &str, args: &HashMap<String, serde_json::Value>) -> String {
        let mut url = format!("{}{}", self.spec.base_url(), path);

        // Replace path parameters like {petId} with actual values
        // Use percent-encoding to prevent special characters from breaking the URL structure
        for (key, value) in args {
            let placeholder = format!("{{{key}}}");
            if url.contains(&placeholder) {
                let value_str = value_to_string(value);
                // Percent-encode the value to handle special URL characters (/, ?, #, %, etc.)
                let encoded_value = percent_encode_path_param(&value_str);
                url = url.replace(&placeholder, &encoded_value);
            }
        }

        url
    }

    /// Extract parameters from path and operation definitions
    ///
    /// Merges path-level and operation-level parameters before extraction.
    /// Returns (`query_params`, `header_params`, `cookie_params`).
    /// Query params use Vec to support exploded arrays with duplicate keys.
    fn extract_parameters(
        path_item: &openapiv3::PathItem,
        operation: &Operation,
        args: &HashMap<String, serde_json::Value>,
    ) -> ExtractedParams {
        let mut query_params = Vec::new();
        let mut header_params = HashMap::new();
        let mut cookie_params = HashMap::new();

        // Merge path-level and operation-level parameters
        let all_params = Self::merge_parameters(&path_item.parameters, &operation.parameters);

        for param_ref in all_params {
            let param = match param_ref {
                ReferenceOr::Item(p) => p,
                // TODO(Phase 2): Resolve $ref parameter references
                ReferenceOr::Reference { .. } => continue,
            };

            match param {
                Parameter::Query {
                    parameter_data,
                    style,
                    ..
                } => {
                    if let Some(value) = args.get(&parameter_data.name) {
                        // Determine explode setting (default is true for query params)
                        let explode = parameter_data.explode.unwrap_or(true);

                        // Encode according to OpenAPI style and explode settings
                        let encoded =
                            encode_query_param(&parameter_data.name, value, style, explode);
                        query_params.extend(encoded);
                    }
                }
                Parameter::Header { parameter_data, .. } => {
                    if let Some(value) = args.get(&parameter_data.name) {
                        let (name, encoded_value) =
                            encode_header_param(&parameter_data.name, value);
                        header_params.insert(name, encoded_value);
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
        let Some((_, _, path_item, operation)) = self.find_operation() else {
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
        };

        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        // Merge path-level and operation-level parameters
        let all_params = Self::merge_parameters(&path_item.parameters, &operation.parameters);

        // Extract parameter schemas from merged parameters
        for param_ref in all_params {
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
        let Some((method, path, path_item, operation)) = self.find_operation() else {
            return ToolResult::error(format!(
                "Operation '{}' not found in OpenAPI spec",
                self.operation_id
            ));
        };

        // Extract parameters (merges path-level and operation-level parameters)
        let (query_params, header_params, cookie_params) =
            Self::extract_parameters(path_item, operation, &args);

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
                return ToolResult::error(format!("Unsupported HTTP method: {method}"));
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
                .map(|(k, v)| format!("{k}={v}"))
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
                return ToolResult::error(format!("HTTP request failed: {e}"));
            }
        };

        let status = response.status();
        let status_code = status.as_u16();

        // Read response body
        let body_text = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                return ToolResult::error(format!("Failed to read response body: {e}"));
            }
        };

        // Try to parse as JSON, fallback to plain text
        let result_value = serde_json::from_str::<serde_json::Value>(&body_text).map_or_else(
            |_| {
                serde_json::json!({
                    "status": status_code,
                    "body": body_text.clone(),
                })
            },
            |json| {
                serde_json::json!({
                    "status": status_code,
                    "body": json,
                })
            },
        );

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

    #[test]
    fn test_path_level_parameters() {
        // Test that path-level parameters are included in the tool declaration
        let spec_json = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/pets/{petId}": {
                    "parameters": [
                        {
                            "name": "petId",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"}
                        },
                        {
                            "name": "version",
                            "in": "query",
                            "required": false,
                            "schema": {"type": "string"}
                        }
                    ],
                    "get": {
                        "operationId": "getPet",
                        "parameters": [
                            {
                                "name": "includeDetails",
                                "in": "query",
                                "required": false,
                                "schema": {"type": "boolean"}
                            }
                        ],
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
            "getPet".to_string(),
            "Get a pet".to_string(),
            "GET".to_string(),
            "/pets/{petId}".to_string(),
            openapi_spec,
            http_client,
            None,
        );

        // Get the declaration and check it includes both path and operation parameters
        let declaration = tool.declaration();
        let schema = declaration.parameters();

        // Should have petId (path-level), version (path-level), and includeDetails (operation-level)
        let properties = schema.get("properties").unwrap().as_object().unwrap();
        assert!(
            properties.contains_key("petId"),
            "Missing path parameter petId"
        );
        assert!(
            properties.contains_key("version"),
            "Missing path-level query parameter version"
        );
        assert!(
            properties.contains_key("includeDetails"),
            "Missing operation-level query parameter includeDetails"
        );

        // Check required parameters
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(
            required.contains(&serde_json::json!("petId")),
            "petId should be required"
        );
        assert!(
            !required.contains(&serde_json::json!("version")),
            "version should not be required"
        );
        assert!(
            !required.contains(&serde_json::json!("includeDetails")),
            "includeDetails should not be required"
        );
    }

    #[test]
    fn test_operation_parameters_override_path_parameters() {
        // Test that operation-level parameters override path-level ones
        let spec_json = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/items/{itemId}": {
                    "parameters": [
                        {
                            "name": "itemId",
                            "in": "path",
                            "required": true,
                            "schema": {"type": "string"},
                            "description": "Path-level description"
                        },
                        {
                            "name": "format",
                            "in": "query",
                            "required": false,
                            "schema": {"type": "string", "enum": ["json", "xml"]}
                        }
                    ],
                    "get": {
                        "operationId": "getItem",
                        "parameters": [
                            {
                                "name": "format",
                                "in": "query",
                                "required": true,
                                "schema": {"type": "string", "enum": ["json", "xml", "yaml"]},
                                "description": "Operation-level override"
                            }
                        ],
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
            "getItem".to_string(),
            "Get an item".to_string(),
            "GET".to_string(),
            "/items/{itemId}".to_string(),
            openapi_spec,
            http_client,
            None,
        );

        let declaration = tool.declaration();
        let schema = declaration.parameters();
        let properties = schema.get("properties").unwrap().as_object().unwrap();

        // Should have both itemId and format
        assert_eq!(properties.len(), 2, "Should have exactly 2 parameters");

        // format should be required (from operation-level override)
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(
            required.contains(&serde_json::json!("format")),
            "format should be required (operation-level override)"
        );

        // The enum should have yaml (from operation-level, not path-level)
        let format_schema = properties.get("format").unwrap();
        let format_enum = format_schema.get("enum").unwrap().as_array().unwrap();
        assert!(
            format_enum.contains(&serde_json::json!("yaml")),
            "format enum should include 'yaml' from operation-level parameter"
        );
    }

    #[test]
    fn test_encode_query_param_array_form_explode() {
        use openapiv3::QueryStyle;

        // Array with form style and explode=true (default)
        let value = serde_json::json!(["available", "sold"]);
        let result = encode_query_param("status", &value, &QueryStyle::Form, true);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("status".to_string(), "available".to_string()));
        assert_eq!(result[1], ("status".to_string(), "sold".to_string()));
    }

    #[test]
    fn test_encode_query_param_array_form_no_explode() {
        use openapiv3::QueryStyle;

        // Array with form style and explode=false
        let value = serde_json::json!(["available", "sold"]);
        let result = encode_query_param("status", &value, &QueryStyle::Form, false);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ("status".to_string(), "available,sold".to_string())
        );
    }

    #[test]
    fn test_encode_query_param_array_pipe_delimited() {
        use openapiv3::QueryStyle;

        // Array with pipe-delimited style
        let value = serde_json::json!(["red", "green", "blue"]);
        let result = encode_query_param("colors", &value, &QueryStyle::PipeDelimited, false);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ("colors".to_string(), "red|green|blue".to_string())
        );
    }

    #[test]
    fn test_encode_query_param_array_space_delimited() {
        use openapiv3::QueryStyle;

        // Array with space-delimited style
        let value = serde_json::json!(["red", "green", "blue"]);
        let result = encode_query_param("colors", &value, &QueryStyle::SpaceDelimited, false);

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ("colors".to_string(), "red green blue".to_string())
        );
    }

    #[test]
    fn test_encode_query_param_deep_object() {
        use openapiv3::QueryStyle;

        // Object with DeepObject style
        let value = serde_json::json!({"name": "John", "age": "30"});
        let result = encode_query_param("user", &value, &QueryStyle::DeepObject, true);

        assert_eq!(result.len(), 2);
        // Order may vary, so check both possibilities
        assert!(
            result.contains(&("user[name]".to_string(), "John".to_string()))
                || result.contains(&("user[age]".to_string(), "30".to_string()))
        );
    }

    #[test]
    fn test_encode_query_param_scalar() {
        use openapiv3::QueryStyle;

        // Scalar values are encoded simply
        let value = serde_json::json!("available");
        let result = encode_query_param("status", &value, &QueryStyle::Form, true);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], ("status".to_string(), "available".to_string()));
    }

    #[test]
    fn test_encode_header_param_array() {
        // Arrays in headers are comma-separated
        let value = serde_json::json!(["gzip", "deflate"]);
        let (name, encoded) = encode_header_param("Accept-Encoding", &value);

        assert_eq!(name, "Accept-Encoding");
        assert_eq!(encoded, "gzip,deflate");
    }

    #[test]
    fn test_encode_header_param_scalar() {
        let value = serde_json::json!("application/json");
        let (name, encoded) = encode_header_param("Content-Type", &value);

        assert_eq!(name, "Content-Type");
        assert_eq!(encoded, "application/json");
    }
}
