//! OpenAPI Specification Parser
//!
//! This module provides functionality to load, parse, and validate OpenAPI 3.x specifications.
//! It supports loading from files (JSON/YAML) and URLs with strict format detection.

use openapiv3::OpenAPI;
use std::collections::HashSet;
use std::path::Path;

/// Parsed OpenAPI specification with extracted metadata
#[derive(Debug)]
pub struct OpenApiSpec {
    /// The parsed OpenAPI specification
    spec: OpenAPI,
    /// Base URL extracted from servers section
    base_url: String,
}

impl OpenApiSpec {
    /// Load OpenAPI spec from file with STRICT format detection
    ///
    /// File extension MUST match content:
    /// - `.json` → JSON content (fails if YAML)
    /// - `.yaml` or `.yml` → YAML content (fails if JSON)
    /// - No extension or unknown → Error
    ///
    /// # Arguments
    /// * `path` - Path to the OpenAPI spec file
    ///
    /// # Example
    /// ```no_run
    /// use radkit::tools::openapi::OpenApiSpec;
    ///
    /// let spec = OpenApiSpec::from_file("specs/petstore.yaml").unwrap();
    /// ```
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read spec file '{}': {}", path.display(), e))?;

        // STRICT: Use extension to determine format
        let spec = match path.extension().and_then(|e| e.to_str()) {
            Some("json") => serde_json::from_str::<OpenAPI>(&content).map_err(|e| {
                format!(
                    "Failed to parse '{}' as JSON (extension is .json): {}\n\
                        Hint: If the file contains YAML, rename it to .yaml or .yml",
                    path.display(),
                    e
                )
            })?,
            Some("yaml") | Some("yml") => {
                serde_yaml::from_str::<OpenAPI>(&content).map_err(|e| {
                    format!(
                        "Failed to parse '{}' as YAML (extension is .yaml/.yml): {}\n\
                        Hint: If the file contains JSON, rename it to .json",
                        path.display(),
                        e
                    )
                })?
            }
            Some(ext) => {
                return Err(format!(
                    "Unsupported file extension '.{}' for '{}'.\n\
                    OpenAPI specs must have .json, .yaml, or .yml extension",
                    ext,
                    path.display()
                ));
            }
            None => {
                return Err(format!(
                    "No file extension for '{}'.\n\
                    OpenAPI specs must have .json, .yaml, or .yml extension",
                    path.display()
                ));
            }
        };

        // Extract base URL from servers
        let base_url = spec
            .servers
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_else(|| "http://localhost".to_string());

        // Validate spec
        Self::validate_spec(&spec)?;

        Ok(Self { spec, base_url })
    }

    /// Load OpenAPI spec from URL with Content-Type detection
    ///
    /// # Arguments
    /// * `url` - URL to fetch the OpenAPI spec from
    ///
    /// # Example
    /// ```no_run
    /// # tokio_test::block_on(async {
    /// use radkit::tools::openapi::OpenApiSpec;
    ///
    /// let spec = OpenApiSpec::from_url("https://petstore3.swagger.io/api/v3/openapi.json")
    ///     .await
    ///     .unwrap();
    /// # });
    /// ```
    pub async fn from_url(url: &str) -> Result<Self, String> {
        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to fetch spec from '{}': {}", url, e))?;

        // Check Content-Type header (clone to avoid borrow issue)
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let content = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response from '{}': {}", url, e))?;

        // Use Content-Type to determine format
        let spec = if content_type.contains("json") || url.ends_with(".json") {
            serde_json::from_str::<OpenAPI>(&content)
                .map_err(|e| format!("Failed to parse JSON from '{}': {}", url, e))?
        } else if content_type.contains("yaml") || url.ends_with(".yaml") || url.ends_with(".yml") {
            serde_yaml::from_str::<OpenAPI>(&content)
                .map_err(|e| format!("Failed to parse YAML from '{}': {}", url, e))?
        } else {
            // Try JSON first, then YAML
            serde_json::from_str::<OpenAPI>(&content)
                .or_else(|_| serde_yaml::from_str::<OpenAPI>(&content))
                .map_err(|e| {
                    format!(
                        "Failed to parse spec from '{}' (tried both JSON and YAML): {}",
                        url, e
                    )
                })?
        };

        // Extract base URL from servers (same as from_file)
        let base_url = spec
            .servers
            .first()
            .map(|s| s.url.clone())
            .unwrap_or_else(|| url.to_string());

        // Validate spec
        Self::validate_spec(&spec)?;

        Ok(Self { spec, base_url })
    }

    /// Load OpenAPI spec from string (auto-detect format)
    ///
    /// # Arguments
    /// * `content` - OpenAPI spec content as string
    /// * `base_url` - Base URL to use for API calls
    pub fn from_str(content: &str, base_url: String) -> Result<Self, String> {
        // Try JSON first (more common), then YAML
        let spec = serde_json::from_str::<OpenAPI>(content)
            .or_else(|_| serde_yaml::from_str::<OpenAPI>(content))
            .map_err(|e| format!("Failed to parse OpenAPI spec string: {}", e))?;

        // Validate spec
        Self::validate_spec(&spec)?;

        Ok(Self { spec, base_url })
    }

    /// PRAGMATIC VALIDATION: Fail only on critical issues
    fn validate_spec(spec: &OpenAPI) -> Result<(), String> {
        // 1. OpenAPI version (critical)
        if !spec.openapi.starts_with("3.") {
            return Err(format!(
                "Unsupported OpenAPI version '{}'. Only 3.x is supported.\n\
                Hint: OpenAPI 2.0 (Swagger) specs must be converted to 3.x first",
                spec.openapi
            ));
        }

        // 2. Has at least one path (critical)
        if spec.paths.paths.is_empty() {
            return Err(
                "OpenAPI spec has no paths defined. Cannot generate tools without API operations."
                    .to_string(),
            );
        }

        // 3. Check for duplicate operation IDs (critical)
        let mut operation_ids = HashSet::new();
        let mut duplicates = Vec::new();

        for path_item in spec.paths.paths.values() {
            for operation in Self::get_operations_from_path_item(path_item) {
                if let Some(op_id) = &operation.operation_id {
                    if !operation_ids.insert(op_id.clone()) {
                        duplicates.push(op_id.clone());
                    }
                }
            }
        }

        if !duplicates.is_empty() {
            return Err(format!(
                "Duplicate operation IDs found: [{}]\n\
                Each operation must have a unique operationId for tool generation.\n\
                Hint: Add unique operationId to each operation in your spec",
                duplicates.join(", ")
            ));
        }

        Ok(())
    }

    /// Helper to extract operations from a path item reference
    fn get_operations_from_path_item(
        path_item_ref: &openapiv3::ReferenceOr<openapiv3::PathItem>,
    ) -> Vec<&openapiv3::Operation> {
        let mut ops = Vec::new();

        // TODO(Phase 2): Resolve $ref path items instead of skipping
        let path_item = match path_item_ref {
            openapiv3::ReferenceOr::Item(item) => item,
            openapiv3::ReferenceOr::Reference { .. } => return ops,
        };

        if let Some(op) = &path_item.get {
            ops.push(op);
        }
        if let Some(op) = &path_item.post {
            ops.push(op);
        }
        if let Some(op) = &path_item.put {
            ops.push(op);
        }
        if let Some(op) = &path_item.delete {
            ops.push(op);
        }
        if let Some(op) = &path_item.patch {
            ops.push(op);
        }
        if let Some(op) = &path_item.head {
            ops.push(op);
        }
        if let Some(op) = &path_item.options {
            ops.push(op);
        }
        if let Some(op) = &path_item.trace {
            ops.push(op);
        }
        ops
    }

    /// Get the base URL for API calls
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get access to the underlying OpenAPI spec
    pub fn spec(&self) -> &OpenAPI {
        &self.spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str_json() {
        let json_spec = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {
                "/test": {
                    "get": {
                        "operationId": "getTest",
                        "responses": {"200": {"description": "OK"}}
                    }
                }
            }
        }"#;

        let spec = OpenApiSpec::from_str(json_spec, "http://localhost".to_string());
        assert!(spec.is_ok());
    }

    #[test]
    fn test_validation_requires_version_3() {
        let json_spec = r#"{
            "openapi": "2.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {}
        }"#;

        let spec = OpenApiSpec::from_str(json_spec, "http://localhost".to_string());
        assert!(spec.is_err());
        assert!(spec.unwrap_err().contains("Unsupported OpenAPI version"));
    }

    #[test]
    fn test_validation_requires_paths() {
        let json_spec = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Test API", "version": "1.0.0"},
            "paths": {}
        }"#;

        let spec = OpenApiSpec::from_str(json_spec, "http://localhost".to_string());
        assert!(spec.is_err());
        assert!(spec.unwrap_err().contains("no paths defined"));
    }
}
