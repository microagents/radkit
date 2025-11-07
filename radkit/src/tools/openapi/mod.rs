//! OpenAPI Tool Integration for Radkit
//!
//! This module provides OpenAPI 3.x specification support for Radkit agents,
//! enabling them to interact with REST APIs by dynamically generating tools
//! from OpenAPI specifications.
//!
//! # Example
//! ```no_run
//! use radkit::tools::openapi::{OpenApiToolSet, AuthConfig, HeaderOrQuery};
//!
//! # tokio_test::block_on(async {
//! // Configure authentication
//! let auth = AuthConfig::ApiKey {
//!     location: HeaderOrQuery::Header,
//!     name: "X-API-Key".to_string(),
//!     value: "my-api-key".to_string(),
//! };
//!
//! // Load OpenAPI spec and generate tools
//! let toolset = OpenApiToolSet::from_file(
//!     "petstore_api".to_string(),
//!     "specs/petstore.yaml",
//!     Some(auth)
//! ).await.unwrap();
//!
//! // Tools are now available for use with agents
//! let tools = toolset.get_tools().await;
//! # });
//! ```

pub mod operation_tool;
pub mod spec;
pub mod toolset;

pub use operation_tool::OpenApiOperationTool;
pub use spec::OpenApiSpec;
pub use toolset::{AuthConfig, HeaderOrQuery, OpenApiToolSet};
