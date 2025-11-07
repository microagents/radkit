//! OpenAPI Toolset - Dynamic tool generation from OpenAPI specifications
//!
//! This module implements the BaseToolset trait for OpenAPI specifications,
//! generating one tool per API operation.

use crate::tools::openapi::{OpenApiOperationTool, OpenApiSpec};
use crate::tools::{BaseTool, BaseToolset};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

/// OpenAPI Toolset that generates tools from OpenAPI specifications
///
/// Dynamically generates one BaseTool per API operation.
pub struct OpenApiToolSet {
    /// Toolset name (for identification)
    name: String,
    /// Path or URL to the spec file
    spec_path: String,
    /// Generated tools (one per operation)
    /// Each tool has its own Arc references to spec, http_client, and auth
    tools: Vec<Arc<OpenApiOperationTool>>,
}

impl OpenApiToolSet {
    /// Create OpenAPI toolset from file
    ///
    /// # Arguments
    /// * `name` - Name for this toolset (e.g., "petstore_api") - **MUST be first parameter** (matches MCPToolset::new pattern)
    /// * `path` - Path to OpenAPI spec file (.json, .yaml, or .yml)
    /// * `auth` - Optional authentication configuration
    ///
    /// # Pattern Note
    /// The `name` parameter is first to match the MCP pattern:
    /// - `MCPToolset::new(name, connection_params)`
    /// - `OpenApiToolSet::from_file(name, path, auth)`
    ///
    /// # Example
    /// ```no_run
    /// use radkit::tools::openapi::{OpenApiToolSet, AuthConfig, HeaderOrQuery};
    ///
    /// # tokio_test::block_on(async {
    /// let auth = AuthConfig::ApiKey {
    ///     location: HeaderOrQuery::Header,
    ///     name: "X-API-Key".to_string(),
    ///     value: "my-key".to_string(),
    /// };
    ///
    /// let toolset = OpenApiToolSet::from_file(
    ///     "petstore_api".to_string(),
    ///     "specs/petstore.yaml",
    ///     Some(auth)
    /// ).await.unwrap();
    /// # });
    /// ```
    pub async fn from_file(
        name: String,
        path: impl AsRef<Path>,
        auth: Option<AuthConfig>,
    ) -> Result<Self, String> {
        let spec_path = path.as_ref().to_string_lossy().to_string();
        let spec = OpenApiSpec::from_file(path)?;
        Self::from_spec(name, spec_path, spec, auth)
    }

    /// Create OpenAPI toolset from URL
    ///
    /// # Arguments
    /// * `name` - Name for this toolset
    /// * `url` - URL to fetch the OpenAPI spec from
    /// * `auth` - Optional authentication configuration
    pub async fn from_url(
        name: String,
        url: &str,
        auth: Option<AuthConfig>,
    ) -> Result<Self, String> {
        let spec = OpenApiSpec::from_url(url).await?;
        Self::from_spec(name, url.to_string(), spec, auth)
    }

    /// Internal: Create toolset from parsed spec
    fn from_spec(
        name: String,
        spec_path: String,
        spec: OpenApiSpec,
        auth: Option<AuthConfig>,
    ) -> Result<Self, String> {
        let spec = Arc::new(spec);
        let http_client = Arc::new(Self::create_http_client(&auth)?);

        // Generate tools for each operation
        let mut tools = Vec::new();

        for (path, path_item_ref) in &spec.spec().paths.paths {
            let path_item = match path_item_ref {
                openapiv3::ReferenceOr::Item(item) => item,
                // TODO(Phase 2): Resolve $ref path items
                openapiv3::ReferenceOr::Reference { .. } => continue,
            };

            // Helper closure to add operation tool
            let mut add_operation = |method: &str, operation: &openapiv3::Operation| {
                let operation_id = operation.operation_id.clone().unwrap_or_else(|| {
                    // Generate operation ID: method_path (e.g., "get_pets", "post_pets_by_id")
                    let path_normalized = path
                        .trim_start_matches('/')
                        .replace('/', "_")
                        .replace('{', "by_")
                        .replace('}', "");
                    format!("{}_{}", method.to_lowercase(), path_normalized)
                });

                let description = operation
                    .summary
                    .clone()
                    .or_else(|| operation.description.clone())
                    .unwrap_or_else(|| format!("{} {}", method, path));

                let tool = Arc::new(OpenApiOperationTool::new(
                    operation_id,
                    description,
                    method.to_string(),
                    path.clone(),
                    spec.clone(),
                    http_client.clone(),
                    auth.clone(),
                ));

                tools.push(tool);
            };

            // Extract operations for each HTTP method
            if let Some(op) = &path_item.get {
                add_operation("GET", op);
            }
            if let Some(op) = &path_item.post {
                add_operation("POST", op);
            }
            if let Some(op) = &path_item.put {
                add_operation("PUT", op);
            }
            if let Some(op) = &path_item.delete {
                add_operation("DELETE", op);
            }
            if let Some(op) = &path_item.patch {
                add_operation("PATCH", op);
            }
            if let Some(op) = &path_item.head {
                add_operation("HEAD", op);
            }
            if let Some(op) = &path_item.options {
                add_operation("OPTIONS", op);
            }
            if let Some(op) = &path_item.trace {
                add_operation("TRACE", op);
            }
        }

        Ok(Self {
            name,
            spec_path,
            tools,
        })
    }

    /// Create HTTP client with optional authentication
    fn create_http_client(auth: &Option<AuthConfig>) -> Result<reqwest::Client, String> {
        let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));

        // Configure default headers based on auth
        if let Some(auth) = auth {
            if let AuthConfig::ApiKey {
                location: HeaderOrQuery::Header,
                name,
                value,
            } = auth
            {
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::HeaderName::from_bytes(name.as_bytes())
                        .map_err(|e| format!("Invalid header name: {}", e))?,
                    reqwest::header::HeaderValue::from_str(value)
                        .map_err(|e| format!("Invalid header value: {}", e))?,
                );
                builder = builder.default_headers(headers);
            }
        }

        builder.build().map_err(|e| e.to_string())
    }

    /// Get the toolset name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the spec path or URL
    pub fn spec_path(&self) -> &str {
        &self.spec_path
    }
}

/// Runtime authentication configuration (non-serializable, contains secrets)
///
/// Use `None` instead of an enum variant to represent no authentication.
#[derive(Debug, Clone)]
pub enum AuthConfig {
    Basic {
        username: String,
        password: String,
    },
    ApiKey {
        location: HeaderOrQuery,
        name: String,
        value: String,
    },
}

/// Location for API key authentication
#[derive(Debug, Clone, Copy)]
pub enum HeaderOrQuery {
    Header,
    Query,
}

// Implement BaseToolset
#[async_trait]
impl BaseToolset for OpenApiToolSet {
    async fn get_tools(&self) -> Vec<Arc<dyn BaseTool>> {
        self.tools
            .iter()
            .map(|t| t.clone() as Arc<dyn BaseTool>)
            .collect()
    }

    async fn close(&self) {
        // OpenAPI toolset doesn't need special cleanup
        // HTTP client will be dropped automatically
    }
}

impl std::fmt::Debug for OpenApiToolSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenApiToolSet")
            .field("name", &self.name)
            .field("spec_path", &self.spec_path)
            .field("tools_count", &self.tools.len())
            .finish()
    }
}
