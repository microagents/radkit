//! A2A server example with JWT authentication

use radkit::agents::Agent;
use radkit::models::MockLlm;
use radkit_axum::{async_trait, A2AServer, AuthContext, AuthExtractor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,    // user_id
    tenant: String, // app_name
    exp: usize,     // expiration
}

/// JWT authentication extractor
struct JwtAuth {
    // In production, this would be a proper JWT library with secret key
    // For demo purposes, we're just parsing a simple JSON payload
}

#[async_trait]
impl AuthExtractor for JwtAuth {
    async fn extract(
        &self,
        parts: &mut axum::http::request::Parts,
    ) -> Result<AuthContext, radkit_axum::auth::AuthError> {
        // Extract JWT from Authorization header
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?;

        // In production, you would:
        // 1. Decode and verify the JWT signature
        // 2. Check expiration
        // 3. Extract claims

        // For demo, we'll just parse base64-encoded JSON
        let claims = decode_demo_jwt(token)?;

        Ok(AuthContext {
            app_name: claims.tenant,
            user_id: claims.sub,
        })
    }
}

fn decode_demo_jwt(token: &str) -> Result<Claims, radkit_axum::auth::AuthError> {
    // This is just for demo - in production use a proper JWT library
    // Expected format: base64(json_claims)

    let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, token)
        .map_err(|_| radkit_axum::auth::AuthError::InvalidToken)?;

    let claims: Claims =
        serde_json::from_slice(&decoded).map_err(|_| radkit_axum::auth::AuthError::InvalidToken)?;

    // Check expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    if claims.exp < now {
        return Err(radkit_axum::auth::AuthError::Failed(
            "Token expired".to_string(),
        ));
    }

    Ok(claims)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create an agent with tools and comprehensive agent card
    let agent = Agent::builder(
        "You are a secure enterprise assistant.",
        MockLlm::new("gpt-4".to_string()),
    )
    .with_card(|card| {
        card.with_name("Enterprise A2A Agent")
            .with_description("A secure agent with JWT authentication")
            .with_version("1.0.0")
            .with_url("https://api.example.com")
            .with_streaming(true)
            .with_push_notifications(false)
            .with_state_transition_history(false)
            .with_default_input_modes(vec![
                "text/plain".to_string(),
                "application/json".to_string(),
            ])
            .with_default_output_modes(vec![
                "text/plain".to_string(),
                "application/json".to_string(),
            ])
            .add_skill_with("data-analysis", "Data Analysis", |skill| {
                skill
                    .with_description("Analyzes business data and provides insights")
                    .add_tag("analytics")
                    .add_tag("business")
                    .add_example("Analyze Q4 sales data")
                    .add_example("Generate revenue forecast")
            })
            .add_skill_with("document-generation", "Document Generation", |skill| {
                skill
                    .with_description("Creates business documents and reports")
                    .add_tag("documents")
                    .add_tag("reports")
                    .add_example("Create quarterly report")
                    .add_example("Draft executive summary")
            })
            .with_provider("Example Corp", "https://example.com")
            .with_documentation_url("https://docs.example.com/agent")
            .add_security_requirement(std::collections::HashMap::from([(
                "bearerAuth".to_string(),
                vec![],
            )]))
            .with_security_schemes(std::collections::HashMap::from([(
                "bearerAuth".to_string(),
                a2a_types::SecurityScheme::Http(a2a_types::HTTPAuthSecurityScheme {
                    scheme_type: "http".to_string(),
                    scheme: "bearer".to_string(),
                    bearer_format: Some("JWT".to_string()),
                    description: Some("JWT authentication".to_string()),
                }),
            )]))
            .with_authenticated_extended_card(true)
    })
    .with_builtin_task_tools()
    .build();

    // Create the A2A server with JWT authentication
    let server = A2AServer::builder(agent).with_auth(JwtAuth {}).build();

    // Generate a demo JWT for testing
    let demo_claims = Claims {
        sub: "user123".to_string(),
        tenant: "acme-corp".to_string(),
        exp: (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600) as usize, // 1 hour from now
    };

    let demo_token = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        serde_json::to_string(&demo_claims)?,
    );

    // Start the server
    println!("Starting Enterprise A2A server on http://localhost:3000");
    println!("Agent Card available at: http://localhost:3000/.well-known/agent-card.json");
    println!("\nDemo JWT token (valid for 1 hour):");
    println!("{}", demo_token);
    println!("\nTest with:");
    println!("  curl -X POST http://localhost:3000/message/send \\");
    println!("    -H 'Authorization: Bearer {}' \\", demo_token);
    println!("    -H 'Content-Type: application/json' \\");
    println!("    -d '{{\"jsonrpc\":\"2.0\",\"method\":\"message/send\",\"params\":{{\"message\":{{\"role\":\"user\",\"parts\":[{{\"kind\":\"text\",\"text\":\"Analyze our Q4 performance\"}}]}}}},\"id\":1}}'");

    server.serve("0.0.0.0:3000").await?;

    Ok(())
}

// Add base64 dependency for this example
// In Cargo.toml dev-dependencies: base64 = "0.22"
