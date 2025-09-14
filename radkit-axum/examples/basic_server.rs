//! Basic A2A server example with simple header-based authentication

use radkit::agents::Agent;
use radkit::models::MockLlm;
use radkit_axum::{async_trait, A2AServer, AuthContext, AuthExtractor};
use std::sync::Arc;
use tracing_subscriber;

/// Simple API key authentication extractor
struct ApiKeyAuth {
    valid_keys: Vec<String>,
}

#[async_trait]
impl AuthExtractor for ApiKeyAuth {
    async fn extract(
        &self,
        parts: &mut axum::http::request::Parts,
    ) -> Result<AuthContext, radkit_axum::auth::AuthError> {
        // Extract API key from Authorization header
        let api_key = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(radkit_axum::auth::AuthError::MissingCredentials)?;

        // Validate API key (in production, this would check a database)
        if !self.valid_keys.contains(&api_key.to_string()) {
            return Err(radkit_axum::auth::AuthError::InvalidToken);
        }

        // Map API key to app_name and user_id (in production, from database)
        let (app_name, user_id) = match api_key {
            "test-key-1" => ("app1", "user1"),
            "test-key-2" => ("app2", "user2"),
            _ => ("default", "anonymous"),
        };

        Ok(AuthContext {
            app_name: app_name.to_string(),
            user_id: user_id.to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create an agent with some basic configuration and agent card
    let agent = Agent::builder(
        "You are a helpful assistant.",
        MockLlm::new("mock-model".to_string()),
    )
    .with_card(|card| {
        card.with_name("Example A2A Agent")
            .with_description("A demonstration agent showing A2A protocol implementation")
            .with_version("0.1.0")
            .with_url("http://localhost:3000")
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
            .add_skill_with("general-assistance", "General Assistance", |skill| {
                skill
                    .with_description("Helps with general questions and tasks")
                    .add_tag("general")
                    .add_tag("help")
                    .add_example("What is the weather today?")
                    .add_example("Help me write an email")
            })
            .with_provider("Radkit Examples", "https://github.com/microagents/radkit")
            .with_documentation_url("https://github.com/microagents/radkit/blob/main/README.md")
            .add_security_requirement(std::collections::HashMap::from([(
                "bearer".to_string(),
                vec![],
            )]))
            .with_security_schemes(std::collections::HashMap::from([(
                "bearer".to_string(),
                a2a_types::SecurityScheme::Http(a2a_types::HTTPAuthSecurityScheme {
                    scheme_type: "http".to_string(),
                    scheme: "bearer".to_string(),
                    bearer_format: Some("API Key".to_string()),
                    description: Some("API key authentication".to_string()),
                }),
            )]))
            .with_authenticated_extended_card(false)
    })
    .with_builtin_task_tools()
    .build();

    // Create the A2A server with API key authentication
    let server = A2AServer::builder(agent)
        .with_auth(ApiKeyAuth {
            valid_keys: vec!["test-key-1".to_string(), "test-key-2".to_string()],
        })
        .build();

    // Start the server
    println!("Starting A2A server on http://localhost:3000");
    println!("Agent Card available at: http://localhost:3000/.well-known/agent-card.json");
    println!("\nTest with:");
    println!("  curl -X POST http://localhost:3000/message/send \\");
    println!("    -H 'Authorization: Bearer test-key-1' \\");
    println!("    -H 'Content-Type: application/json' \\");
    println!("    -d '{{\"jsonrpc\":\"2.0\",\"method\":\"message/send\",\"params\":{{\"message\":{{\"role\":\"user\",\"parts\":[{{\"kind\":\"text\",\"text\":\"Hello\"}}]}}}},\"id\":1}}'");

    server.serve("0.0.0.0:3000").await?;

    Ok(())
}
