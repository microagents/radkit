use axum::{middleware, response::IntoResponse, Router};
use radkit::agents::Agent;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::{
    auth::{AuthContext, AuthExtractor},
    routes::{create_routes, ServerState},
};

/// A2A Protocol server for Radkit agents
pub struct A2AServer {
    agent: Arc<Agent>,
    auth_extractor: Arc<dyn AuthExtractor>,
}

impl A2AServer {
    /// Create a new A2A server builder
    pub fn builder(agent: Agent) -> A2AServerBuilder {
        A2AServerBuilder::new(agent)
    }

    /// Display server startup information including agent card details
    fn display_server_info(
        &self,
        local_addr: &std::net::SocketAddr,
        agent_card: &a2a_types::AgentCard,
    ) {
        tracing::info!("🚀 A2A Server Starting");
        tracing::info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        tracing::info!("📡 Server listening at: http://{}", local_addr);
        tracing::info!("🤖 Agent: {} ({})", agent_card.name, agent_card.description);
        tracing::info!(
            "📋 Agent Card available at: http://{}/.well-known/agent-card.json",
            local_addr
        );

        // Display agent card details
        tracing::info!("📊 Agent Card Details:");
        tracing::info!("  Name: {}", agent_card.name);
        tracing::info!("  Description: {}", agent_card.description);
        tracing::info!(
            "  Version: {}",
            if agent_card.version.is_empty() {
                "⚠️  Not set"
            } else {
                &agent_card.version
            }
        );
        tracing::info!(
            "  URL: {}",
            if agent_card.url.is_empty() {
                "⚠️  Not set"
            } else {
                &agent_card.url
            }
        );
        tracing::info!(
            "  Streaming: {}",
            match agent_card.capabilities.streaming {
                Some(true) => "✅ Enabled",
                Some(false) => "❌ Disabled",
                None => "⚠️  Not set",
            }
        );
        tracing::info!(
            "  Push Notifications: {}",
            match agent_card.capabilities.push_notifications {
                Some(true) => "✅ Enabled",
                Some(false) => "❌ Disabled",
                None => "⚠️  Not set",
            }
        );

        if !agent_card.skills.is_empty() {
            tracing::info!("  Skills: {} configured", agent_card.skills.len());
            for skill in &agent_card.skills {
                tracing::info!("    • {} ({})", skill.name, skill.id);
            }
        } else {
            tracing::info!("  Skills: None configured");
        }
    }

    /// Validate agent card configuration and warn about potential issues
    fn validate_agent_card(
        &self,
        local_addr: &std::net::SocketAddr,
        agent_card: &a2a_types::AgentCard,
    ) {
        let mut warnings = Vec::new();
        let server_url = format!("http://{}", local_addr);

        // Check for empty/missing critical fields
        if agent_card.version.is_empty() {
            warnings.push(
                "⚠️  Version is empty - other agents may have trouble identifying compatibility"
                    .to_string(),
            );
        }

        if agent_card.url.is_empty() {
            warnings.push(
                "⚠️  URL is empty - other agents will not know how to reach this agent".to_string(),
            );
        } else {
            // Check URL mismatch
            let card_url = agent_card.url.trim_end_matches('/');
            let server_url_trimmed = server_url.trim_end_matches('/');

            if card_url != server_url_trimmed {
                warnings.push(format!(
                    "ℹ️ URL specified in the Card ({}) differs from server address ({})",
                    card_url, server_url_trimmed
                ));
            }
        }

        if agent_card.name.is_empty() {
            warnings.push("⚠️  AgentCard name is empty".to_string());
        }

        if agent_card.description.is_empty() {
            warnings.push("⚠️  AgentCard description is empty".to_string());
        }

        match agent_card.capabilities.streaming {
            Some(false) => {
                warnings.push("ℹ️  Streaming is disabled".to_string());
            }
            None => {
                warnings.push("ℹ️  Streaming capability not specified".to_string());
            }
            Some(true) => {} // No warning for enabled streaming
        }

        if agent_card.skills.is_empty() {
            warnings.push("ℹ️  No skills configured.".to_string());
        }

        // Display warnings if any
        if !warnings.is_empty() {
            tracing::warn!("⚠️  AgentCard Warnings:");
            tracing::warn!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            for warning in &warnings {
                tracing::warn!("  {}", warning);
            }

            // Provide helpful suggestions
            tracing::info!("💡 Suggestions:");
            if agent_card.url.is_empty() || agent_card.url != server_url {
                tracing::info!("  • Set the correct URL using .with_agent_card_config(|card| card.with_url(\"{}\"))", server_url);
            }
            if agent_card.version.is_empty() {
                tracing::info!("  • Set a version using .with_agent_card_config(|card| card.with_version(\"1.0.0\"))");
            }
            tracing::info!(
                "  • Update your agent card configuration before deploying to production"
            );
        }
    }

    /// Convert the server into an Axum router
    pub fn into_router(self) -> Router {
        let state = ServerState {
            agent: self.agent.clone(),
        };

        let auth_extractor = self.auth_extractor.clone();

        // Create the routes
        let app = create_routes(state)
            // Add authentication middleware
            .layer(middleware::from_fn(
                move |req: axum::extract::Request, next: middleware::Next| {
                    let extractor = auth_extractor.clone();
                    async move {
                        // Extract auth context
                        let (mut parts, body) = req.into_parts();
                        match extractor.extract(&mut parts).await {
                            Ok(auth) => {
                                // Insert auth context into extensions
                                parts.extensions.insert(auth);
                                let req = axum::extract::Request::from_parts(parts, body);
                                Ok(next.run(req).await)
                            }
                            Err(e) => {
                                // Convert auth error to response
                                Err(e.into_response())
                            }
                        }
                    }
                },
            ))
            // Add CORS support
            .layer(CorsLayer::permissive());

        app
    }

    /// Run the server on the specified address
    pub async fn serve(self, addr: impl tokio::net::ToSocketAddrs) -> Result<(), std::io::Error> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;

        // Get agent card for validation and display
        let agent_card = &self.agent.agent_card;

        // Display server startup information
        self.display_server_info(&local_addr, agent_card);

        // Validate and warn about potential issues
        self.validate_agent_card(&local_addr, agent_card);

        let app = self.into_router();
        axum::serve(listener, app).await?;
        Ok(())
    }
}

/// Builder for configuring an A2A server
pub struct A2AServerBuilder {
    agent: Agent,
    auth_extractor: Option<Arc<dyn AuthExtractor>>,
}

impl A2AServerBuilder {
    fn new(agent: Agent) -> Self {
        Self {
            agent,
            auth_extractor: None,
        }
    }

    /// Set the authentication extractor
    pub fn with_auth<E: AuthExtractor + 'static>(mut self, extractor: E) -> Self {
        self.auth_extractor = Some(Arc::new(extractor));
        self
    }

    /// Configure the agent's card (useful for setting URL, version, etc.)
    pub fn with_agent_card_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(a2a_types::AgentCard) -> a2a_types::AgentCard,
    {
        self.agent.agent_card = f(self.agent.agent_card.clone());
        self
    }

    /// Build the A2A server
    pub fn build(mut self) -> A2AServer {
        // Configure default server settings if the agent card doesn't have them
        let mut updated_card = self.agent.agent_card.clone();

        // Set default server URL if empty
        if updated_card.url.is_empty() {
            updated_card = updated_card.with_url("http://localhost:3000");
        }

        // Set default version if empty
        if updated_card.version.is_empty() {
            updated_card = updated_card.with_version("0.1.0");
        }

        // Ensure streaming capability is enabled for servers
        updated_card = updated_card.with_streaming(true);

        self.agent.agent_card = updated_card;

        // Use a default auth extractor if none provided (for development)
        let auth_extractor = self
            .auth_extractor
            .unwrap_or_else(|| Arc::new(DefaultAuthExtractor));

        A2AServer {
            agent: Arc::new(self.agent),
            auth_extractor,
        }
    }
}

/// Default auth extractor for development (extracts from headers)
struct DefaultAuthExtractor;

#[axum::async_trait]
impl AuthExtractor for DefaultAuthExtractor {
    async fn extract(
        &self,
        parts: &mut axum::http::request::Parts,
    ) -> Result<AuthContext, crate::auth::AuthError> {
        // Try to extract from X-App-Name and X-User-Id headers (for development)
        let app_name = parts
            .headers
            .get("X-App-Name")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("default")
            .to_string();

        let user_id = parts
            .headers
            .get("X-User-Id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("anonymous")
            .to_string();

        Ok(AuthContext { app_name, user_id })
    }
}
