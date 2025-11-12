//! Test OpenAPI Operations Without Explicit operationId
//!
//! Verifies that operations without operationId (using synthetic IDs)
//! can be successfully executed.

#![cfg(feature = "openapi")]

use radkit::tools::{BaseToolset, DefaultExecutionState, OpenApiToolSet, ToolContext};
use std::collections::HashMap;
use std::io::Write;

#[tokio::test]
async fn test_operation_without_operation_id() {
    // Create a simple OpenAPI spec without operationId
    let spec_json = r#"
{
    "openapi": "3.0.0",
    "info": {
        "title": "Test API",
        "version": "1.0.0"
    },
    "servers": [
        {
            "url": "https://jsonplaceholder.typicode.com"
        }
    ],
    "paths": {
        "/posts/1": {
            "get": {
                "summary": "Get a post",
                "description": "Fetch post by ID",
                "responses": {
                    "200": {
                        "description": "Success"
                    }
                }
            }
        }
    }
}
    "#;

    // Write to temporary file
    let temp_file = std::env::temp_dir().join("test_no_op_id.json");
    let mut file = std::fs::File::create(&temp_file).unwrap();
    file.write_all(spec_json.as_bytes()).unwrap();

    // Parse and create toolset
    let toolset = OpenApiToolSet::from_file("test_api".to_string(), &temp_file, None)
        .await
        .expect("Failed to create toolset");

    // Cleanup
    let _ = std::fs::remove_file(&temp_file);

    // Get tools - should have one tool with synthetic ID
    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 1, "Should have exactly one tool");

    let tool = &tools[0];

    // Synthetic ID should be "get_posts_1" (method_path with normalization)
    assert_eq!(
        tool.name(),
        "get_posts_1",
        "Synthetic ID should match pattern"
    );

    // Verify declaration works
    let declaration = tool.declaration();
    assert_eq!(declaration.name(), "get_posts_1");
    assert_eq!(declaration.description(), "Get a post");

    println!("✓ Tool created with synthetic ID: {}", tool.name());
    println!("✓ Declaration: {}", declaration.description());
}

#[tokio::test]
#[ignore = "requires network access to JSONPlaceholder API"]
async fn test_execute_operation_without_operation_id() {
    // Create a simple OpenAPI spec without operationId
    let spec_json = r#"
{
    "openapi": "3.0.0",
    "info": {
        "title": "Test API",
        "version": "1.0.0"
    },
    "servers": [
        {
            "url": "https://jsonplaceholder.typicode.com"
        }
    ],
    "paths": {
        "/posts/1": {
            "get": {
                "summary": "Get a post",
                "description": "Fetch post by ID",
                "responses": {
                    "200": {
                        "description": "Success"
                    }
                }
            }
        }
    }
}
    "#;

    // Write to temporary file
    let temp_file = std::env::temp_dir().join("test_exec_no_op_id.json");
    let mut file = std::fs::File::create(&temp_file).unwrap();
    file.write_all(spec_json.as_bytes()).unwrap();

    // Parse and create toolset
    let toolset = OpenApiToolSet::from_file("test_api".to_string(), &temp_file, None)
        .await
        .expect("Failed to create toolset");

    // Cleanup
    let _ = std::fs::remove_file(&temp_file);

    let tools = toolset.get_tools().await;
    let tool = &tools[0];

    // Execute the tool - this would previously fail with "Operation not found"
    let args = HashMap::new();
    let state = DefaultExecutionState::new();
    let context = ToolContext::builder()
        .with_state(&state)
        .build()
        .expect("Failed to create context");

    println!("🔧 Executing tool without explicit operationId...");
    let result = tool.run_async(args, &context).await;

    // Should succeed (or return HTTP error, but not "Operation not found")
    assert!(
        result.is_success(),
        "Tool execution should succeed, got error: {:?}",
        result.error_message()
    );

    println!("✅ Tool execution successful!");
    println!("Result: {:?}", result.data());
}

#[tokio::test]
async fn test_multiple_operations_without_operation_id() {
    // Create spec with multiple operations, none having operationId
    let spec_json = r#"
{
    "openapi": "3.0.0",
    "info": {
        "title": "Test API",
        "version": "1.0.0"
    },
    "servers": [
        {
            "url": "https://api.example.com"
        }
    ],
    "paths": {
        "/users": {
            "get": {
                "summary": "List users",
                "responses": {
                    "200": {"description": "Success"}
                }
            },
            "post": {
                "summary": "Create user",
                "responses": {
                    "201": {"description": "Created"}
                }
            }
        },
        "/users/{id}": {
            "get": {
                "summary": "Get user",
                "parameters": [
                    {
                        "name": "id",
                        "in": "path",
                        "required": true,
                        "schema": {"type": "string"}
                    }
                ],
                "responses": {
                    "200": {"description": "Success"}
                }
            }
        }
    }
}
    "#;

    // Write to temporary file
    let temp_file = std::env::temp_dir().join("test_multi_no_op_id.json");
    let mut file = std::fs::File::create(&temp_file).unwrap();
    file.write_all(spec_json.as_bytes()).unwrap();

    let toolset = OpenApiToolSet::from_file("test_api".to_string(), &temp_file, None)
        .await
        .expect("Failed to create toolset");

    // Cleanup
    let _ = std::fs::remove_file(&temp_file);

    let tools = toolset.get_tools().await;
    assert_eq!(tools.len(), 3, "Should have 3 tools");

    // Verify synthetic IDs
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

    assert!(
        tool_names.contains(&"get_users"),
        "Should have 'get_users' tool"
    );
    assert!(
        tool_names.contains(&"post_users"),
        "Should have 'post_users' tool"
    );
    assert!(
        tool_names.contains(&"get_users_by_id"),
        "Should have 'get_users_by_id' tool"
    );

    println!("✓ All tools created with synthetic IDs:");
    for name in &tool_names {
        println!("  - {}", name);
    }

    // Verify declarations work for all tools
    for tool in &tools {
        let declaration = tool.declaration();
        assert!(!declaration.name().is_empty());
        assert!(!declaration.description().is_empty());
        println!("✓ Declaration OK for: {}", tool.name());
    }
}
