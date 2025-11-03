//! Simple OpenAPI Petstore Tests
//!
//! Tests OpenAPI toolset with the public Petstore API to verify:
//! - Spec loading and validation
//! - Tool generation from operations
//! - Tool declarations are generated correctly

use radkit::tools::{BaseToolset, OpenApiToolSet};

#[tokio::test]
async fn test_openapi_spec_from_url() {
    // Load OpenAPI spec from URL
    let result = OpenApiToolSet::from_url(
        "petstore".to_string(),
        "https://petstore3.swagger.io/api/v3/openapi.json",
        None, // No auth required for petstore
    )
    .await;

    assert!(
        result.is_ok(),
        "Failed to load OpenAPI spec: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_openapi_tool_generation() {
    // Create toolset
    let toolset = OpenApiToolSet::from_url(
        "petstore".to_string(),
        "https://petstore3.swagger.io/api/v3/openapi.json",
        None,
    )
    .await
    .expect("Failed to create toolset");

    // Get tools
    let tools = toolset.get_tools().await;

    // Verify tools were generated
    assert!(
        !tools.is_empty(),
        "No tools were generated from OpenAPI spec"
    );

    println!("Generated {} tools from Petstore API:", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name(), tool.description());
    }

    // Verify specific tools exist
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

    // Check for some expected operations
    let expected_operations = vec!["findPetsByStatus", "getPetById", "addPet"];
    for op in expected_operations {
        assert!(
            tool_names.contains(&op),
            "Expected operation '{}' not found in generated tools. Available: {:?}",
            op,
            tool_names
        );
    }
}

#[tokio::test]
async fn test_openapi_tool_declaration() {
    // Create toolset
    let toolset = OpenApiToolSet::from_url(
        "petstore".to_string(),
        "https://petstore3.swagger.io/api/v3/openapi.json",
        None,
    )
    .await
    .expect("Failed to create toolset");

    // Get tools
    let tools = toolset.get_tools().await;

    // Verify each tool has a declaration
    for tool in tools {
        let declaration = tool.declaration();

        assert_eq!(
            declaration.name(),
            tool.name(),
            "Declaration name doesn't match tool name"
        );
        assert!(
            !declaration.description().is_empty(),
            "Tool '{}' has empty description",
            tool.name()
        );

        // Verify parameters schema is valid JSON
        assert!(
            declaration.parameters().is_object(),
            "Tool '{}' parameters is not a JSON object",
            tool.name()
        );

        // Verify the parameters have properties
        let params = declaration.parameters();
        assert!(
            params.get("properties").is_some(),
            "Tool '{}' has no properties field",
            tool.name()
        );
        assert!(
            params.get("required").is_some(),
            "Tool '{}' has no required field",
            tool.name()
        );
    }
}

#[tokio::test]
async fn test_parameter_schema_generation() {
    // Create toolset
    let toolset = OpenApiToolSet::from_url(
        "petstore".to_string(),
        "https://petstore3.swagger.io/api/v3/openapi.json",
        None,
    )
    .await
    .expect("Failed to create toolset");

    let tools = toolset.get_tools().await;

    // Find getPetById which has path parameter
    let get_pet_by_id = tools
        .iter()
        .find(|t| t.name() == "getPetById")
        .expect("getPetById not found");

    let declaration = get_pet_by_id.declaration();
    let params = declaration.parameters();
    let properties = params.get("properties").unwrap();

    // Should have petId parameter
    assert!(
        properties.get("petId").is_some(),
        "getPetById should have petId parameter"
    );

    let pet_id_schema = &properties["petId"];
    assert_eq!(
        pet_id_schema.get("type").and_then(|v| v.as_str()),
        Some("integer"),
        "petId should be integer type"
    );

    // Find findPetsByStatus which has query parameter with enum
    let find_by_status = tools
        .iter()
        .find(|t| t.name() == "findPetsByStatus")
        .expect("findPetsByStatus not found");

    let declaration = find_by_status.declaration();
    let params = declaration.parameters();
    let properties = params.get("properties").unwrap();

    // Should have status parameter
    assert!(
        properties.get("status").is_some(),
        "findPetsByStatus should have status parameter"
    );

    let status_schema = &properties["status"];
    assert_eq!(
        status_schema.get("type").and_then(|v| v.as_str()),
        Some("string"),
        "status should be string type"
    );

    // Should have enum values
    assert!(
        status_schema.get("enum").is_some(),
        "status parameter should have enum values"
    );

    println!("✅ Parameter schema generation working correctly!");
    println!(
        "getPetById parameters: {}",
        serde_json::to_string_pretty(&params).unwrap()
    );
}
